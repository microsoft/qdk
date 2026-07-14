// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    sources: Vec<Source>,
    /// The common prefix of the sources
    /// e.g. if the sources all start with `/Users/microsoft/code/qsharp/src`, then this value is
    /// `/Users/microsoft/code/qsharp/src`.
    common_prefix: Option<Arc<str>>,
    entry: Option<Source>,
}

impl SourceMap {
    pub fn new(
        sources: impl IntoIterator<Item = (SourceName, SourceContents)>,
        entry: Option<Arc<str>>,
    ) -> Self {
        let mut offset_sources = Vec::new();

        let entry_source = entry.map(|contents| Source {
            name: "<entry>".into(),
            contents,
            offset: 0,
        });

        let mut offset = next_offset(entry_source.as_ref());
        for (name, contents) in sources {
            let source = Source {
                name,
                contents,
                offset,
            };
            offset = next_offset(Some(&source));
            offset_sources.push(source);
        }

        // Each source has a name, which is a string. The project root dir is calculated as the
        // common prefix of all of the sources.
        // Calculate the common prefix.
        let common_prefix: String = longest_common_prefix(
            &offset_sources
                .iter()
                .map(|source| source.name.as_ref())
                .collect::<Vec<_>>(),
        )
        .to_string();

        let common_prefix: Arc<str> = Arc::from(common_prefix);

        Self {
            sources: offset_sources,
            common_prefix: if common_prefix.is_empty() {
                None
            } else {
                Some(common_prefix)
            },
            entry: entry_source,
        }
    }

    #[must_use]
    pub fn entry(&self) -> Option<&Source> {
        self.entry.as_ref()
    }

    pub fn push(&mut self, name: SourceName, contents: SourceContents) -> u32 {
        let offset = next_offset(self.sources.last());

        self.sources.push(Source {
            name,
            contents,
            offset,
        });

        offset
    }

    #[must_use]
    pub fn find_by_offset(&self, offset: u32) -> Option<&Source> {
        self.sources
            .iter()
            .rev()
            .chain(&self.entry)
            .find(|source| source.contains_offset(offset))
    }

    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.name.as_ref() == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Source> {
        self.sources.iter()
    }

    /// Returns the sources as an iter, but with the project root directory subtracted
    /// from the individual source names.
    pub fn relative_sources(&self) -> impl Iterator<Item = Source> + '_ {
        self.sources.iter().map(move |source| {
            let name = source.name.as_ref();
            let relative_name = self.relative_name(name);

            Source {
                name: relative_name.into(),
                contents: source.contents.clone(),
                offset: source.offset,
            }
        })
    }

    #[must_use]
    pub fn relative_name<'a>(&'a self, name: &'a str) -> &'a str {
        if let Some(common_prefix) = &self.common_prefix {
            name.strip_prefix(common_prefix.as_ref()).unwrap_or(name)
        } else {
            name
        }
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub name: SourceName,
    pub contents: SourceContents,
    pub offset: u32,
}

impl Source {
    #[must_use]
    pub fn contains_offset(&self, offset: u32) -> bool {
        let end = self
            .offset
            .checked_add(
                u32::try_from(self.contents.len()).expect("contents length should fit into u32"),
            )
            .expect("source end should fit into u32");
        (self.offset..=end).contains(&offset)
    }
}

pub type SourceName = Arc<str>;

pub type SourceContents = Arc<str>;

/// Returns the shared path prefix of the supplied source names.
///
/// When source names diverge, the common text is truncated through its last
/// path separator (`/`, `\`, or `:`), so the result does not contain a partial
/// path component. Identical source names retain the complete name. A single
/// source returns its containing path, and an empty slice returns an empty
/// string.
///
/// Comparison is bytewise and linear in the compared input. This is UTF-8 safe
/// because returned slices end only after an ASCII path separator or at the end
/// of the first source name.
#[must_use]
pub fn longest_common_prefix<'a>(strs: &'a [&'a str]) -> &'a str {
    if strs.len() == 1 {
        return truncate_to_path_separator(strs[0]);
    }

    let Some(first) = strs.first() else {
        return "";
    };

    // Carry forward the shortest prefix shared with `first`. A mismatch
    // shortens the prefix to its byte offset; if `zip` reaches the end of
    // either input without a mismatch, the shorter input bounds the prefix.
    let common_prefix_len = strs.iter().skip(1).fold(first.len(), |prefix_len, string| {
        first.as_bytes()[..prefix_len]
            .iter()
            .zip(string.as_bytes())
            .position(|(left, right)| left != right)
            .unwrap_or_else(|| prefix_len.min(string.len()))
    });

    if common_prefix_len == first.len() {
        first
    } else {
        truncate_to_path_separator_at(first, common_prefix_len)
    }
}

/// Truncates a source name through its final path separator.
fn truncate_to_path_separator(prefix: &str) -> &str {
    truncate_to_path_separator_at(prefix, prefix.len())
}

/// Truncates the first `end` bytes through the final path separator.
///
/// `end` may fall within a multibyte character. The returned boundary remains
/// valid UTF-8 because each recognized separator is a single-byte ASCII
/// character.
fn truncate_to_path_separator_at(prefix: &str, end: usize) -> &str {
    let bytes = &prefix.as_bytes()[..end];
    let last_separator_index = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .or_else(|| bytes.iter().rposition(|byte| *byte == b'\\'))
        .or_else(|| bytes.iter().rposition(|byte| *byte == b':'));
    if let Some(last_separator_index) = last_separator_index {
        // Return the prefix up to and including the last path separator
        return &prefix[0..=last_separator_index];
    }
    // If there's no path separator in the prefix, return an empty string
    ""
}

fn next_offset(last_source: Option<&Source>) -> u32 {
    // Leave a gap of 1 between each source so that offsets at EOF
    // get mapped to the correct source
    last_source.map_or(0, |s| {
        1 + s.offset + u32::try_from(s.contents.len()).expect("contents length should fit into u32")
    })
}
