// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A qodec as the YAML documents of its bundle, one per artifact: the text
//! `:yaml` prints, and `:edit`, `:set` and `:unset` change.

use serde_yaml::Value;
use std::fmt::Write;

/// One document of a bundle: an artifact and its YAML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    /// The artifact's file name, e.g. `h_all.gadget.yaml`. The manifest comes
    /// first.
    pub name: String,
    /// The artifact's YAML, unindented, ending in a newline.
    pub body: String,
}

/// Splits `bundle`, the text of [`qodec::Qodec::to_bundle_string`], into its
/// documents.
pub fn split(bundle: &str) -> Result<Vec<Doc>, String> {
    let mut chunks: Vec<Vec<&str>> = vec![Vec::new()];
    for line in bundle.lines() {
        if line == "---" {
            chunks.push(Vec::new());
        } else {
            chunks.last_mut().expect("starts with a chunk").push(line);
        }
    }
    chunks
        .into_iter()
        .filter(|lines| lines.iter().any(|l| !l.trim().is_empty()))
        .map(|lines| doc(&lines))
        .collect()
}

/// Parses the lines of one `{name: body}` document.
fn doc(lines: &[&str]) -> Result<Doc, String> {
    let text = lines.join("\n");
    let value: Value = serde_yaml::from_str(&text).map_err(|e| e.to_string())?;
    let entry = match value {
        Value::Mapping(map) if map.len() == 1 => map.into_iter().next(),
        _ => None,
    };
    let Some((Value::String(name), body)) = entry else {
        return Err(format!(
            "a bundle document is not `{{name: body}}`:\n{text}"
        ));
    };
    // Keep the text as written when the body is indented under its name.
    let indented: Option<Vec<&str>> = lines[1..]
        .iter()
        .map(|l| l.strip_prefix("  ").or(l.trim().is_empty().then_some("")))
        .collect();
    let body = match indented {
        Some(body) if lines[0].trim_end().ends_with(':') && !body.is_empty() => {
            format!("{}\n", body.join("\n").trim_end())
        }
        _ => to_yaml(&body)?,
    };
    Ok(Doc { name, body })
}

/// The bundle of `docs`, which [`qodec::Qodec::from_bundle_str`] loads.
pub fn join(docs: &[Doc]) -> String {
    docs.iter().map(render).collect()
}

/// `doc` as a bundle document.
pub fn render(doc: &Doc) -> String {
    let name = to_yaml(&Value::String(doc.name.clone())).unwrap_or_else(|_| doc.name.clone());
    let mut text = format!("---\n{}:\n", name.trim_end());
    for line in doc.body.lines() {
        if line.is_empty() {
            text.push('\n');
        } else {
            writeln!(text, "  {line}").expect("writing to a string succeeds");
        }
    }
    text
}

fn to_yaml(value: &Value) -> Result<String, String> {
    serde_yaml::to_string(value).map_err(|e| e.to_string())
}

/// The documents `item` names, by index: a document name, `qodec` for the
/// manifest, a layer (`top`, `bottom`, or its index) for its instruction set,
/// a gadget's mnemonic, a code's block type, or the `name` of an instruction
/// set or code.
pub fn find(docs: &[Doc], item: &str) -> Result<usize, String> {
    if let Some(k) = docs.iter().position(|d| d.name == item) {
        return Ok(k);
    }
    let layers = manifest_layers(docs);
    let layer = match item {
        "top" => Some(0),
        "bottom" => layers.len().checked_sub(1),
        _ => item.parse().ok(),
    };
    let mut files: Vec<String> = Vec::new();
    if matches!(item, "qodec" | "manifest") {
        files.extend(docs.first().map(|d| d.name.clone()));
    }
    if let Some(isa) = layer
        .and_then(|k| layers.get(k))
        .and_then(|l| l.isa.clone())
    {
        files.push(isa);
    }
    for layer in &layers {
        files.extend(
            layer
                .gadgets
                .iter()
                .chain(&layer.codes)
                .filter(|(key, _)| key == item)
                .map(|(_, file)| file.clone()),
        );
    }
    for d in docs {
        let named = serde_yaml::from_str::<Value>(&d.body)
            .ok()
            .and_then(|v| v.get("name").and_then(Value::as_str).map(|n| n == item));
        if named == Some(true) {
            files.push(d.name.clone());
        }
    }
    let mut found: Vec<usize> = files
        .iter()
        .filter_map(|f| docs.iter().position(|d| &d.name == f))
        .collect();
    found.sort_unstable();
    found.dedup();
    match found.as_slice() {
        [k] => Ok(*k),
        [] => Err(format!(
            "no item `{item}`; `:yaml` lists the qodec's documents"
        )),
        _ => {
            let names: Vec<&str> = found.iter().map(|&k| docs[k].name.as_str()).collect();
            Err(format!(
                "`{item}` could be any of {}; name the document",
                names.join(", ")
            ))
        }
    }
}

/// The files a manifest layer refers to.
struct LayerFiles {
    isa: Option<String>,
    codes: Vec<(String, String)>,
    gadgets: Vec<(String, String)>,
}

/// The layers of the manifest, `docs[0]`, as far as they refer to files.
fn manifest_layers(docs: &[Doc]) -> Vec<LayerFiles> {
    let Some(manifest) = docs
        .first()
        .and_then(|d| serde_yaml::from_str::<Value>(&d.body).ok())
    else {
        return Vec::new();
    };
    let files = |layer: &Value, key: &str| -> Vec<(String, String)> {
        layer
            .get(key)
            .and_then(Value::as_mapping)
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| Some((k.as_str()?.to_string(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    };
    manifest
        .get("layers")
        .and_then(Value::as_sequence)
        .map(|layers| {
            layers
                .iter()
                .map(|l| LayerFiles {
                    isa: l
                        .get("instruction_set")
                        .and_then(Value::as_str)
                        .map(String::from),
                    codes: files(l, "codes"),
                    gadgets: files(l, "gadgets"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One step of a [`Path`].
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Key(String),
    Index(usize),
}

/// A place in a YAML document: keys after dots, indices in brackets, and
/// keys with dots in quoted brackets, e.g. `circuit.source[0]` or
/// `frames["out[0].z[0]"]`. The empty path is the whole document.
#[derive(Debug, PartialEq, Eq)]
struct Path(Vec<Step>);

impl Path {
    fn parse(text: &str) -> Result<Self, String> {
        let bad = |why: &str| format!("bad path `{text}`: {why}");
        let mut steps = Vec::new();
        let mut rest = text;
        while !rest.is_empty() {
            if let Some(inner) = rest.strip_prefix('[') {
                let (step, after) = if let Some(quoted) = inner.strip_prefix('"') {
                    let end = quoted.find("\"]").ok_or_else(|| bad("unclosed `[\"`"))?;
                    (Step::Key(quoted[..end].to_string()), &quoted[end + 2..])
                } else {
                    let end = inner.find(']').ok_or_else(|| bad("unclosed `[`"))?;
                    let index = inner[..end]
                        .trim()
                        .parse()
                        .map_err(|_| bad("an index is a number, a key is quoted"))?;
                    (Step::Index(index), &inner[end + 1..])
                };
                steps.push(step);
                rest = after;
            } else {
                let key = rest.strip_prefix('.').unwrap_or(rest);
                if !steps.is_empty() && key.len() == rest.len() {
                    return Err(bad("expected `.` or `[` after a step"));
                }
                let end = key.find(['.', '[']).unwrap_or(key.len());
                if end == 0 {
                    return Err(bad("empty key"));
                }
                steps.push(Step::Key(key[..end].to_string()));
                rest = &key[end..];
            }
        }
        Ok(Self(steps))
    }
}

/// `body` with the value at `path` set to `value`, a YAML value. Missing keys
/// are added, with an empty mapping or list on the way as needed, and an index
/// one past the end appends.
pub fn set(body: &str, path: &str, value: &str) -> Result<String, String> {
    let path = Path::parse(path)?;
    let value: Value =
        serde_yaml::from_str(value).map_err(|e| format!("`{value}` is not a YAML value: {e}"))?;
    let mut doc = parse(body)?;
    let Some((target, parents)) = path.0.split_last() else {
        return to_yaml(&value);
    };
    match (descend(&mut doc, parents, Some(target))?, target) {
        (Value::Mapping(map), Step::Key(key)) => {
            map.insert(Value::String(key.clone()), value);
        }
        (Value::Sequence(list), &Step::Index(k)) if k < list.len() => list[k] = value,
        (Value::Sequence(list), &Step::Index(k)) if k == list.len() => list.push(value),
        (Value::Sequence(list), Step::Index(k)) => {
            return Err(format!(
                "index {k} is past the end of a list of {}",
                list.len()
            ));
        }
        (_, step) => return Err(mismatch(step)),
    }
    to_yaml(&doc)
}

/// `body` without the key or list entry at `path`.
pub fn unset(body: &str, path: &str) -> Result<String, String> {
    let path = Path::parse(path)?;
    let mut doc = parse(body)?;
    let Some((target, parents)) = path.0.split_last() else {
        return Err("name what to remove; the path is empty".to_string());
    };
    match (descend(&mut doc, parents, None)?, target) {
        (Value::Mapping(map), Step::Key(key)) => {
            map.remove(key.as_str())
                .ok_or_else(|| format!("no key `{key}` to remove"))?;
        }
        (Value::Sequence(list), &Step::Index(k)) if k < list.len() => {
            list.remove(k);
        }
        (Value::Sequence(list), Step::Index(k)) => {
            return Err(format!("no entry {k} in a list of {}", list.len()));
        }
        (_, step) => return Err(mismatch(step)),
    }
    to_yaml(&doc)
}

fn parse(body: &str) -> Result<Value, String> {
    serde_yaml::from_str(body).map_err(|e| format!("the document is not valid YAML: {e}"))
}

/// The value at `steps` below `value`. With `then`, the step after them, a
/// missing key, or an index one past the end, gets an empty mapping or list
/// for the next step to go into.
fn descend<'v>(
    mut value: &'v mut Value,
    steps: &[Step],
    then: Option<&Step>,
) -> Result<&'v mut Value, String> {
    for (k, step) in steps.iter().enumerate() {
        // What a missing value for `step` starts as, if it may be added.
        let empty = || match then.and(steps.get(k + 1).or(then)) {
            Some(Step::Key(_)) => Some(Value::Mapping(serde_yaml::Mapping::new())),
            Some(Step::Index(_)) => Some(Value::Sequence(Vec::new())),
            None => None,
        };
        value = match (value, step) {
            (Value::Mapping(map), Step::Key(key)) => {
                if !map.contains_key(key.as_str()) {
                    let empty = empty().ok_or_else(|| format!("no key `{key}`"))?;
                    map.insert(Value::String(key.clone()), empty);
                }
                map.get_mut(key.as_str()).expect("the key is there")
            }
            (Value::Sequence(list), &Step::Index(index)) => {
                let len = list.len();
                if index == len
                    && let Some(empty) = empty()
                {
                    list.push(empty);
                }
                list.get_mut(index)
                    .ok_or_else(|| format!("no entry {index} in a list of {len}"))?
            }
            (_, step) => return Err(mismatch(step)),
        };
    }
    Ok(value)
}

fn mismatch(step: &Step) -> String {
    match step {
        Step::Key(key) => format!("`{key}` is a key, but the value there is not a mapping"),
        Step::Index(k) => format!("[{k}] is an index, but the value there is not a list"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE: &str = "---\nqodec.yaml:\n  name: q\n  layers:\n  - instruction_set: a.isa.yaml\n    gadgets:\n      g: g.gadget.yaml\n    codes:\n      blk: c.code.yaml\n  - instruction_set: b+c.isa.yaml\n---\na.isa.yaml:\n  name: A\n  description: |\n    two\n\n    paragraphs\n---\nb+c.isa.yaml:\n  name: B\n---\nc.code.yaml:\n  name: rep\n---\ng.gadget.yaml:\n  circuit:\n    source:\n    - H: [0]\n";

    #[test]
    fn bundles_split_into_documents_and_join_back() {
        let docs = split(BUNDLE).expect("splits");
        let names: Vec<&str> = docs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "qodec.yaml",
                "a.isa.yaml",
                "b+c.isa.yaml",
                "c.code.yaml",
                "g.gadget.yaml"
            ]
        );
        assert_eq!(
            docs[1].body,
            "name: A\ndescription: |\n  two\n\n  paragraphs\n"
        );
        assert_eq!(join(&docs), BUNDLE);
    }

    #[test]
    fn items_name_documents() {
        let docs = split(BUNDLE).expect("splits");
        let find = |item| find(&docs, item).map(|k| docs[k].name.as_str());
        assert_eq!(find("g.gadget.yaml"), Ok("g.gadget.yaml"));
        assert_eq!(find("qodec"), Ok("qodec.yaml"));
        assert_eq!(find("top"), Ok("a.isa.yaml"));
        assert_eq!(find("bottom"), Ok("b+c.isa.yaml"));
        assert_eq!(find("1"), Ok("b+c.isa.yaml"));
        assert_eq!(find("g"), Ok("g.gadget.yaml"));
        assert_eq!(find("blk"), Ok("c.code.yaml"));
        assert_eq!(find("rep"), Ok("c.code.yaml"));
        assert_eq!(find("A"), Ok("a.isa.yaml"));
        assert!(find("nothing").is_err());
    }

    #[test]
    fn paths_set_and_unset_values() {
        let body = "circuit:\n  source:\n  - H: [0]\nchecks: []\n";
        let set = |path, value| set(body, path, value);
        assert_eq!(
            set("circuit.source[0]", "{S: [1]}"),
            Ok("circuit:\n  source:\n  - S:\n    - 1\nchecks: []\n".to_string())
        );
        assert_eq!(
            set("checks[0]", "[a, b]"),
            Ok("circuit:\n  source:\n  - H:\n    - 0\nchecks:\n- - a\n  - b\n".to_string())
        );
        assert_eq!(
            set("frames[\"out[0].z[0]\"]", "[x]"),
            Ok("circuit:\n  source:\n  - H:\n    - 0\nchecks: []\nframes:\n  out[0].z[0]:\n  - x\n".to_string())
        );
        assert_eq!(
            set("circuit.inputs[0].q", "c"),
            Ok(
                "circuit:\n  source:\n  - H:\n    - 0\n  inputs:\n  - q: c\nchecks: []\n"
                    .to_string()
            )
        );
        assert!(set("checks[1]", "x").is_err());
        assert!(set("circuit.source.key", "x").is_err());
        assert!(set("checks.key", "x").is_err());
        assert_eq!(
            unset(body, "circuit.source[0]"),
            Ok("circuit:\n  source: []\nchecks: []\n".to_string())
        );
        assert_eq!(
            unset(body, "checks"),
            Ok("circuit:\n  source:\n  - H:\n    - 0\n".to_string())
        );
        assert!(unset(body, "nothing").is_err());
    }

    #[test]
    fn paths_parse_keys_indices_and_quoted_keys() {
        use Step::{Index, Key};
        let path = |text| Path::parse(text).map(|p| p.0);
        assert_eq!(path(""), Ok(vec![]));
        assert_eq!(
            path("a.b[2][\"c.d\"].e"),
            Ok(vec![
                Key("a".into()),
                Key("b".into()),
                Index(2),
                Key("c.d".into()),
                Key("e".into())
            ])
        );
        for bad in ["a..b", "a[x]", "a[0", "a[\"b]", "a[0]b", ".a."] {
            assert!(path(bad).is_err(), "{bad}");
        }
    }
}
