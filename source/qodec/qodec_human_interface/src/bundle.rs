// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A qodec as text: the YAML documents of its bundle, one per artifact, as
//! written, comments and all. The text is what the person edits; the `qodec`
//! crate's model is derived from it.

use serde_yaml::Value;
use std::{
    fmt::Write,
    path::{Path, PathBuf},
};

/// One artifact and its YAML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    /// The artifact's path, relative to the manifest, e.g. `idle.gadget.yaml`.
    /// The manifest comes first.
    pub name: String,
    /// Comment lines written before the document, kept as they are.
    pub lead: String,
    /// The artifact's YAML, unindented, ending in a newline.
    pub body: String,
    /// Whether it is a file of its own that a bundle refers to, such as an
    /// instruction set shared by several qodecs. It is saved to its file.
    pub external: bool,
}

impl Doc {
    /// The body's lines.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.body.lines().map(str::to_string).collect()
    }

    /// Replaces the body with `lines`.
    pub fn set_lines(&mut self, lines: &[String]) {
        let mut body = lines.join("\n");
        body.push('\n');
        self.body = body;
    }
}

/// Where a qodec came from, and so where it is saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// One bundle file, with the text before its first document.
    Bundle {
        /// The file.
        path: PathBuf,
        /// Comments before the first document.
        preamble: String,
    },
    /// A manifest; each artifact is a file next to it.
    Manifest(PathBuf),
}

/// The documents of the qodec at `path`, and where they came from: the
/// documents of a bundle, or a manifest and its files, and then every other
/// YAML file they refer to, so that the documents make up the whole qodec.
pub fn load(path: &Path) -> Result<(Vec<Doc>, Source), String> {
    let read = |file: &Path| {
        std::fs::read_to_string(file).map_err(|e| format!("cannot read `{}`: {e}", file.display()))
    };
    let text = read(path)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let (mut docs, source) = if text.lines().any(|l| l == "---") {
        let (preamble, docs) = split(&text)?;
        let source = Source::Bundle {
            path: path.to_path_buf(),
            preamble,
        };
        (docs, source)
    } else {
        let name = path.file_name().map_or_else(
            || "qodec.yaml".to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let doc = Doc {
            name,
            lead: String::new(),
            body: ended(text),
            external: true,
        };
        (vec![doc], Source::Manifest(path.to_path_buf()))
    };
    // Breadth first, so files come in the order the manifest names them.
    let mut next = 0;
    while next < docs.len() {
        let from = docs[next].name.clone();
        let mut strings = Vec::new();
        if let Ok(value) = serde_yaml::from_str::<Value>(&docs[next].body) {
            collect_strings(&value, &mut strings);
        }
        next += 1;
        for s in strings {
            let target = s.split('#').next().unwrap_or("");
            if target.contains(char::is_whitespace)
                || !Path::new(target)
                    .extension()
                    .is_some_and(|e| e == "yaml" || e == "yml")
            {
                continue;
            }
            let name = resolve(&from, target);
            let known = docs.iter().any(|d| resolve("qodec.yaml", &d.name) == name);
            let file = dir.join(&name);
            if !known && file.is_file() {
                docs.push(Doc {
                    name,
                    lead: String::new(),
                    body: ended(read(&file)?),
                    external: true,
                });
            }
        }
    }
    Ok((docs, source))
}

fn ended(text: String) -> String {
    if text.ends_with('\n') {
        text
    } else {
        format!("{text}\n")
    }
}

/// Every string in `value`, keys included.
fn collect_strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => out.push(s.clone()),
        Value::Sequence(items) => items.iter().for_each(|v| collect_strings(v, out)),
        Value::Mapping(m) => {
            for (k, v) in m {
                collect_strings(k, out);
                collect_strings(v, out);
            }
        }
        Value::Tagged(t) => collect_strings(&t.value, out),
        _ => {}
    }
}

/// Writes the documents of `docs` that differ from `saved` back where they
/// came from: a bundle's own documents into the bundle, and every other
/// document into its own file.
pub fn save(docs: &[Doc], saved: &[Doc], source: &Source) -> Result<String, String> {
    let changed = |doc: &Doc| !saved.contains(doc);
    let (dir, root_file) = match source {
        Source::Bundle { path, .. } | Source::Manifest(path) => {
            (path.parent().unwrap_or(Path::new(".")), path)
        }
    };
    let mut written = Vec::new();
    if let Source::Bundle { path, preamble } = source {
        let own: Vec<Doc> = docs.iter().filter(|d| !d.external).cloned().collect();
        let before: Vec<&Doc> = saved.iter().filter(|d| !d.external).collect();
        if own.len() != before.len() || own.iter().any(changed) {
            std::fs::write(path, format!("{preamble}{}", join(&own)))
                .map_err(|e| format!("cannot write `{}`: {e}", path.display()))?;
            written.push(path.display().to_string());
        }
    }
    for (k, doc) in docs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.external && changed(d))
    {
        let file = if k == 0 {
            root_file.clone()
        } else {
            dir.join(&doc.name)
        };
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, &doc.body)
            .map_err(|e| format!("cannot write `{}`: {e}", file.display()))?;
        written.push(file.display().to_string());
    }
    Ok(match written.as_slice() {
        [] => "Nothing to save.".to_string(),
        [one] => format!("Saved `{one}`."),
        many => format!("Saved {} files: `{}`.", many.len(), many.join("`, `")),
    })
}

/// Splits a bundle into the text before its first document and its documents.
pub fn split(bundle: &str) -> Result<(String, Vec<Doc>), String> {
    let mut chunks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in bundle.lines() {
        if line == "---" {
            chunks.push(Vec::new());
        } else {
            chunks.last_mut().expect("starts with a chunk").push(line);
        }
    }
    let mut preamble = String::new();
    let mut docs = Vec::new();
    for (i, lines) in chunks.into_iter().enumerate() {
        let is_doc = lines
            .iter()
            .any(|l| !l.trim().is_empty() && !l.starts_with('#'));
        if !is_doc {
            if i == 0 {
                for l in lines {
                    writeln!(preamble, "{l}").expect("writing to a string succeeds");
                }
            }
            continue;
        }
        docs.push(doc(&lines)?);
    }
    Ok((preamble, docs))
}

/// Parses the lines of one `{name: body}` document.
fn doc(lines: &[&str]) -> Result<Doc, String> {
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .expect("a document has a line");
    let mut lead = String::new();
    for l in &lines[..start] {
        writeln!(lead, "{l}").expect("writing to a string succeeds");
    }
    let lines = &lines[start..];
    let key = lines[0].trim_end();
    let indented: Option<Vec<&str>> = lines[1..]
        .iter()
        .map(|l| l.strip_prefix("  ").or(l.trim().is_empty().then_some("")))
        .collect();
    if let (Some(name), Some(body)) = (key.strip_suffix(':'), indented)
        && !body.is_empty()
    {
        let name: String = serde_yaml::from_str(name).map_err(|e| e.to_string())?;
        return Ok(Doc {
            name,
            lead,
            body: format!("{}\n", body.join("\n").trim_end()),
            external: false,
        });
    }
    // Not in the usual shape: parse it, and write the body anew.
    let value: Value = serde_yaml::from_str(&lines.join("\n")).map_err(|e| e.to_string())?;
    let entry = match value {
        Value::Mapping(map) if map.len() == 1 => map.into_iter().next(),
        _ => None,
    };
    let Some((Value::String(name), body)) = entry else {
        return Err("a bundle document is not `{name: body}`".to_string());
    };
    Ok(Doc {
        name,
        lead,
        body: serde_yaml::to_string(&body).map_err(|e| e.to_string())?,
        external: false,
    })
}

/// The bundle of `docs`.
#[must_use]
pub fn join(docs: &[Doc]) -> String {
    let mut text = String::new();
    for doc in docs {
        let name = serde_yaml::to_string(&Value::String(doc.name.clone()))
            .unwrap_or_else(|_| doc.name.clone());
        write!(text, "---\n{}{}:\n", doc.lead, name.trim_end())
            .expect("writing to a string succeeds");
        for line in doc.body.lines() {
            if line.is_empty() {
                text.push('\n');
            } else {
                writeln!(text, "  {line}").expect("writing to a string succeeds");
            }
        }
    }
    text
}

/// `message`, about the bundle [`join`] made of `docs`, with each `line L
/// column C` of the bundle told as the document and line it is in.
#[must_use]
pub fn relocate(docs: &[Doc], message: &str) -> String {
    // The bundle line each document's body starts at, 1-based.
    let mut starts = Vec::new();
    let mut line = 1;
    for doc in docs {
        line += 2 + doc.lead.lines().count();
        starts.push(line);
        line += doc.body.lines().count();
    }
    let place = |at: usize, column: usize| -> Option<String> {
        let d = starts.iter().rposition(|&s| s <= at)?;
        Some(format!(
            "{} line {} column {}",
            docs[d].name,
            at - starts[d] + 1,
            column.saturating_sub(2).max(1)
        ))
    };
    let mut out = String::new();
    let mut rest = message;
    while let Some(i) = rest.find("line ") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 5..];
        let digits = tail.len() - tail.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let after = &tail[digits..];
        let column = after.strip_prefix(" column ").map(|c| {
            let n = c.len() - c.trim_start_matches(|ch: char| ch.is_ascii_digit()).len();
            (c[..n].parse::<usize>().ok(), 8 + n)
        });
        match (tail[..digits].parse::<usize>().ok(), column) {
            (Some(at), Some((Some(col), used))) if let Some(p) = place(at, col) => {
                out.push_str(&p);
                rest = &after[used..];
            }
            _ => {
                out.push_str("line ");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// What a manifest's layer lists: its instruction set file, its codes and its
/// gadgets, each by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayerFiles {
    /// The instruction set file.
    pub isa: Option<String>,
    /// Block type -> code file.
    pub codes: Vec<(String, String)>,
    /// Mnemonic -> gadget file.
    pub gadgets: Vec<(String, String)>,
}

/// The layers a manifest body lists.
#[must_use]
pub fn manifest_layers(manifest: &str) -> Vec<LayerFiles> {
    let Ok(value) = serde_yaml::from_str::<Value>(manifest) else {
        return Vec::new();
    };
    let Some(layers) = value.get("layers").and_then(Value::as_sequence) else {
        return Vec::new();
    };
    let map = |v: Option<&Value>| -> Vec<(String, String)> {
        v.and_then(Value::as_mapping)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    };
    layers
        .iter()
        .map(|layer| LayerFiles {
            isa: layer
                .get("instruction_set")
                .and_then(Value::as_str)
                .map(str::to_string),
            codes: map(layer.get("codes")),
            gadgets: map(layer.get("gadgets")),
        })
        .collect()
}

/// The path of `file`, named relative to `from`, relative to the manifest:
/// how a bundle names the artifact that `from` refers to as `file`.
#[must_use]
pub fn resolve(from: &str, file: &str) -> String {
    let base = Path::new(from).parent().unwrap_or(Path::new(""));
    normalize(&base.join(file))
}

fn normalize(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for c in path.components() {
        match c {
            std::path::Component::ParentDir => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else {
                    parts.push("..".to_string());
                }
            }
            std::path::Component::CurDir => {}
            other => parts.push(other.as_os_str().to_string_lossy().to_string()),
        }
    }
    parts.join("/")
}

/// `body` with `item` added at the end of the top-level list `key`, keeping
/// the list's indentation; the key is added if it is missing. `item` is the
/// YAML of one entry, without its `- `.
#[must_use]
pub fn append_item(body: &str, key: &str, item: &str) -> String {
    append(body, key, &format!("- {item}"))
}

/// `body` with the `key: value` entry added to the top-level map `key`.
#[must_use]
pub fn append_entry(body: &str, key: &str, entry: &str) -> String {
    append(body, key, entry)
}

fn append(body: &str, key: &str, line: &str) -> String {
    append_path(body, &[key], line)
}

/// `body` with `item` added at the end of the list at `path`, a key under a
/// key: `["circuit", "source"]`. The keys are added if they are missing.
#[must_use]
pub fn append_nested(body: &str, path: &[&str], item: &str) -> String {
    append_path(body, path, &format!("- {item}"))
}

/// The line of the key `path`, each key nested in the one before.
fn find_path(lines: &[String], path: &[&str]) -> Option<usize> {
    let mut from = 0;
    let mut end = lines.len();
    let mut indent = 0;
    let mut at = None;
    for (depth, key) in path.iter().enumerate() {
        let header = format!("{key}:");
        let found = (from..end).find(|&i| {
            let l = &lines[i];
            let ind = l.len() - l.trim_start().len();
            (if depth == 0 { ind == 0 } else { ind > indent })
                && (l.trim_start() == header || l.trim_start().starts_with(&format!("{header} ")))
        })?;
        indent = lines[found].len() - lines[found].trim_start().len();
        from = found + 1;
        end = (from..lines.len())
            .find(|&i| {
                let l = &lines[i];
                !l.trim().is_empty()
                    && !l.trim_start().starts_with('#')
                    && (l.len() - l.trim_start().len()) <= indent
                    && !l[indent..].starts_with("- ")
            })
            .unwrap_or(lines.len());
        at = Some(found);
    }
    at
}

fn append_path(body: &str, path: &[&str], line: &str) -> String {
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let Some(at) = find_path(&lines, path) else {
        // Add the missing keys at the end, nested.
        let mut depth = 0;
        for (d, key) in path.iter().enumerate() {
            if find_path(&lines, &path[..=d]).is_none() {
                lines.push(format!("{}{key}:", "  ".repeat(d)));
            }
            depth = d + 1;
        }
        lines.push(format!("{}{line}", "  ".repeat(depth)));
        return lines.join("\n") + "\n";
    };
    let base = lines[at].len() - lines[at].trim_start().len();
    let key = path.last().expect("a path has a key");
    let header = format!("{}{key}:", " ".repeat(base));
    // An inline empty value, such as `checks: []`, becomes a block.
    let inline = lines[at][header.len()..].trim().to_string();
    if matches!(inline.as_str(), "[]" | "{}") {
        lines[at] = header;
    } else if !inline.is_empty() && !inline.starts_with('#') {
        // An inline list or map: parse it, add the item, and write it as a
        // block.
        lines[at] = header;
        let parsed: Value = serde_yaml::from_str(&inline).unwrap_or(Value::Null);
        let mut items: Vec<String> = Vec::new();
        match parsed {
            Value::Sequence(seq) => {
                for v in seq {
                    items.push(format!("- {}", flow(&v)));
                }
            }
            Value::Mapping(map) => {
                for (k, v) in map {
                    items.push(format!("{}: {}", flow(&k), flow(&v)));
                }
            }
            _ => {}
        }
        for (i, item) in items.into_iter().enumerate() {
            lines.insert(at + 1 + i, format!("{}{item}", " ".repeat(base + 2)));
        }
    }
    // The block runs while lines are indented deeper, blank, comments, or
    // `- ` entries at the key's own indentation.
    let mut end = at + 1;
    let mut indent: Option<String> = None;
    while end < lines.len() {
        let l = &lines[end];
        let ind = l.len() - l.trim_start().len();
        let entry_at_base = ind == base && l.trim_start().starts_with("- ");
        if l.trim().is_empty() || ind > base || l.trim_start().starts_with('#') || entry_at_base {
            if indent.is_none() && !l.trim().is_empty() && !l.trim_start().starts_with('#') {
                indent = Some(l[..l.len() - l.trim_start().len()].to_string());
            }
            end += 1;
        } else {
            break;
        }
    }
    while end > at + 1 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let indent = indent.unwrap_or_else(|| " ".repeat(base + 2));
    lines.insert(end, format!("{indent}{line}"));
    lines.join("\n") + "\n"
}

/// `value` as one line of flow YAML.
#[must_use]
pub fn flow(value: &Value) -> String {
    match value {
        Value::Sequence(seq) => {
            format!("[{}]", seq.iter().map(flow).collect::<Vec<_>>().join(", "))
        }
        Value::Mapping(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(k, v)| format!("{}: {}", flow(k), flow(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Value::String(s) => {
            let plain: Result<Value, _> = serde_yaml::from_str(s);
            if matches!(plain, Ok(Value::String(ref p)) if p == s)
                && !s.contains([',', '[', ']', '{', '}', ':', '#'])
            {
                s.clone()
            } else {
                format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
        }
        other => {
            serde_yaml::to_string(other).map_or_else(|_| String::new(), |s| s.trim().to_string())
        }
    }
}

/// `body` with the top-level key `key` replaced by `block`, the lines of its
/// new value (each indented by two spaces), or added if missing.
#[must_use]
pub fn replace_block(body: &str, key: &str, block: &[String]) -> String {
    let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
    let header = format!("{key}:");
    let at = lines
        .iter()
        .position(|l| l == &header || l.starts_with(&format!("{header} ")));
    let new: Vec<String> = std::iter::once(header)
        .chain(block.iter().map(|l| format!("  {l}")))
        .collect();
    match at {
        None => lines.extend(new),
        Some(at) => {
            let mut end = at + 1;
            while end < lines.len() {
                let l = &lines[end];
                if l.trim().is_empty()
                    || l.starts_with(' ')
                    || l.starts_with('#')
                    || l.starts_with("- ")
                {
                    end += 1;
                } else {
                    break;
                }
            }
            while end > at + 1 && lines[end - 1].trim().is_empty() {
                end -= 1;
            }
            lines.splice(at..end, new);
        }
    }
    lines.join("\n") + "\n"
}

/// The manifest body with gadget `mnemonic` -> `file` added to layer `k`.
#[must_use]
pub fn add_gadget(manifest: &str, k: usize, mnemonic: &str, file: &str) -> String {
    let mut value: Value = match serde_yaml::from_str(manifest) {
        Ok(v) => v,
        Err(_) => return manifest.to_string(),
    };
    // Prefer an edit of the text that keeps its comments: find the layer's
    // `gadgets:` line.
    let lines: Vec<&str> = manifest.lines().collect();
    let starts: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.trim_start().starts_with("- instruction_set:"))
        .map(|(i, _)| i)
        .collect();
    if let Some(&start) = starts.get(k) {
        let end = starts.get(k + 1).copied().unwrap_or(lines.len());
        let item_indent = lines[start].len() - lines[start].trim_start().len() + 2;
        if let Some(g) = (start..end).find(|&i| lines[i].trim_start() == "gadgets:") {
            let entry_indent = lines
                .get(g + 1)
                .filter(|l| !l.trim().is_empty())
                .map_or(item_indent + 2, |l| l.len() - l.trim_start().len());
            let mut last = g;
            while last + 1 < end
                && (lines[last + 1].len() - lines[last + 1].trim_start().len()) >= entry_indent
                && !lines[last + 1].trim().is_empty()
            {
                last += 1;
            }
            let mut out: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
            out.insert(
                last + 1,
                format!("{}{mnemonic}: {file}", " ".repeat(entry_indent)),
            );
            return out.join("\n") + "\n";
        }
        let mut last = start;
        while last + 1 < end && !lines[last + 1].trim().is_empty() {
            last += 1;
        }
        let mut out: Vec<String> = lines.iter().map(|l| (*l).to_string()).collect();
        out.insert(last + 1, format!("{}gadgets:", " ".repeat(item_indent)));
        out.insert(
            last + 2,
            format!("{}{mnemonic}: {file}", " ".repeat(item_indent + 2)),
        );
        return out.join("\n") + "\n";
    }
    // Fall back to writing the manifest anew.
    if let Some(layer) = value
        .get_mut("layers")
        .and_then(Value::as_sequence_mut)
        .and_then(|l| l.get_mut(k))
        .and_then(Value::as_mapping_mut)
    {
        let gadgets = layer
            .entry(Value::String("gadgets".to_string()))
            .or_insert_with(|| Value::Mapping(serde_yaml::Mapping::new()));
        if let Some(g) = gadgets.as_mapping_mut() {
            g.insert(
                Value::String(mnemonic.to_string()),
                Value::String(file.to_string()),
            );
        }
    }
    serde_yaml::to_string(&value).unwrap_or_else(|_| manifest.to_string())
}

#[cfg(test)]
mod tests {
    use super::{add_gadget, append_item, join, replace_block, resolve, split};

    #[test]
    fn a_bundle_round_trips_with_its_comments() {
        let bundle = "# preamble\n---\nqodec.yaml:\n  name: x\n  # a comment\n  layers: []\n---\n# before\nc.code.yaml:\n  name: c\n";
        let (preamble, docs) = split(bundle).expect("splits");
        assert_eq!(preamble, "# preamble\n");
        assert_eq!(docs[0].body, "name: x\n# a comment\nlayers: []\n");
        assert_eq!(docs[1].lead, "# before\n");
        assert_eq!(format!("{preamble}{}", join(&docs)), bundle);
    }

    #[test]
    fn items_go_at_the_end_of_their_list() {
        let body = "circuit: {source: []}\nchecks:\n  - [a]\n  # note\nreadouts:\n  - [b]\n";
        assert_eq!(
            append_item(body, "checks", "[c]"),
            "circuit: {source: []}\nchecks:\n  - [a]\n  # note\n  - [c]\nreadouts:\n  - [b]\n"
        );
        assert_eq!(
            append_item("checks: []\n", "checks", "[c]"),
            "checks:\n  - [c]\n"
        );
        assert_eq!(
            append_item("x: 1\n", "frames", "[c]"),
            "x: 1\nframes:\n  - [c]\n"
        );
        assert_eq!(
            super::append_nested(
                "circuit:\n  source:\n    - R: [0]\nchecks: []\n",
                &["circuit", "source"],
                "M: [0]"
            ),
            "circuit:\n  source:\n    - R: [0]\n    - M: [0]\nchecks: []\n"
        );
        assert_eq!(
            super::append_nested("circuit:\n  source: []\n", &["circuit", "source"], "R: [0]"),
            "circuit:\n  source:\n    - R: [0]\n"
        );

        assert_eq!(
            replace_block(
                "a: 1\nreadouts:\n  - [x]\nb: 2\n",
                "readouts",
                &["- [y]".to_string()]
            ),
            "a: 1\nreadouts:\n  - [y]\nb: 2\n"
        );
    }

    #[test]
    fn a_gadget_is_added_to_its_layer() {
        let manifest = "name: q\nlayers:\n  - instruction_set: a.isa.yaml # top\n    gadgets:\n      h: h.gadget.yaml\n  - instruction_set: b.isa.yaml\n";
        assert_eq!(
            add_gadget(manifest, 0, "x", "x.gadget.yaml"),
            "name: q\nlayers:\n  - instruction_set: a.isa.yaml # top\n    gadgets:\n      h: h.gadget.yaml\n      x: x.gadget.yaml\n  - instruction_set: b.isa.yaml\n"
        );
        assert_eq!(
            resolve("c4/idle.gadget.yaml", "../c4.code.yaml"),
            "c4.code.yaml"
        );
    }
}
