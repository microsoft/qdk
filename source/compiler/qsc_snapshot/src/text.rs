// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Bounded S-expression syntax. Domain crates define records and variants;
//! this module does not interpret circuit or graph semantics.

use std::{collections::BTreeSet, fmt::Write};

pub use lexpr::Value;

use crate::{Budget, Error, Format, Kind, MAX_WIRE_DEPTH, MODEL_VERSION, header, payload};

pub struct Writer {
    text: String,
    depth: usize,
}

impl Writer {
    pub fn new(kind: Kind) -> Result<Self, Error> {
        let mut writer = Self {
            text: String::from_utf8(header(kind, Format::Text)).map_err(Error::malformed)?,
            depth: 0,
        };
        writer.open("qdk")?;
        writer.atom(MODEL_VERSION);
        Ok(writer)
    }

    pub fn open(&mut self, name: &str) -> Result<(), Error> {
        if self.depth >= MAX_WIRE_DEPTH {
            return Err(Error::DepthLimit);
        }
        self.separator();
        self.text.push('(');
        self.text.push_str(name);
        self.depth += 1;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        self.depth = self
            .depth
            .checked_sub(1)
            .ok_or_else(|| Error::malformed("unbalanced writer"))?;
        self.text.push(')');
        Ok(())
    }

    pub fn field(
        &mut self,
        name: &str,
        write: impl FnOnce(&mut Self) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.open(name)?;
        write(self)?;
        self.close()
    }

    fn separator(&mut self) {
        if !self.text.ends_with(['\n', '(']) {
            self.text.push(' ');
        }
    }

    pub fn atom(&mut self, value: impl std::fmt::Display) {
        self.separator();
        write!(self.text, "{value}").expect("writing to a String is infallible");
    }

    pub fn string(&mut self, value: &str) -> Result<(), Error> {
        self.separator();
        self.text
            .push_str(&lexpr::to_string(&Value::string(value)).map_err(Error::malformed)?);
        Ok(())
    }

    pub fn real(&mut self, bits: u64) -> Result<(), Error> {
        let value = f64::from_bits(bits);
        if value.is_finite() {
            self.separator();
            let mut decimal = format!("{value:?}");
            if !decimal.contains('.') {
                let index = decimal.find('e').unwrap_or(decimal.len());
                decimal.insert_str(index, ".0");
            }
            self.text.push_str(&decimal);
        } else {
            self.open("real64-bits")?;
            self.string(&format!("{bits:016x}"))?;
            self.close()?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<String, Error> {
        self.close()?;
        if self.depth != 0 {
            return Err(Error::malformed("unclosed text record"));
        }
        Ok(self.text)
    }
}

pub struct Parsed {
    root: Value,
    items: usize,
}

impl Parsed {
    pub fn record(&self, name: &str) -> Result<Record<'_>, Error> {
        let root = list(&self.root)?;
        let [tag, version, document] = root.as_slice() else {
            return Err(Error::malformed("expected (qdk VERSION DOCUMENT)"));
        };
        if symbol(tag)? != "qdk" {
            return Err(Error::malformed("expected qdk model"));
        }
        let version = u32(version)?;
        if version != MODEL_VERSION {
            return Err(Error::ModelVersion(version));
        }
        Record::new(document, name)
    }

    #[must_use]
    pub fn budget(&self) -> Budget {
        Budget::new(self.items)
    }
}

pub fn parse(kind: Kind, bytes: &[u8]) -> Result<Parsed, Error> {
    let (format, payload) = payload(kind, bytes)?;
    if format != Format::Text {
        return Err(Error::malformed("expected a text envelope"));
    }
    let text = std::str::from_utf8(payload).map_err(Error::malformed)?;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for byte in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else {
            match byte {
                b'"' => quoted = true,
                b'(' => {
                    depth += 1;
                    if depth > MAX_WIRE_DEPTH {
                        return Err(Error::DepthLimit);
                    }
                }
                b')' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| Error::malformed("unbalanced text"))?;
                }
                b';' | b'\'' | b'`' | b',' | b'#' | b'[' | b']' => {
                    return Err(Error::malformed("unsupported S-expression reader syntax"));
                }
                _ => {}
            }
        }
    }
    if depth != 0 || quoted {
        return Err(Error::malformed("unterminated text"));
    }
    Ok(Parsed {
        root: parse_lists(text)?,
        // Each syntax/domain list item requires at least one input byte.
        items: text.len(),
    })
}

fn parse_lists(text: &str) -> Result<Value, Error> {
    let mut frames: Vec<Vec<Value>> = Vec::new();
    let mut root = None;
    let mut offset = 0;
    let bytes = text.as_bytes();
    while offset < bytes.len() {
        let value = match bytes[offset] {
            b' ' | b'\n' | b'\r' | b'\t' => {
                offset += 1;
                continue;
            }
            b'(' => {
                frames.push(Vec::new());
                offset += 1;
                continue;
            }
            b')' => {
                offset += 1;
                Value::list(
                    frames
                        .pop()
                        .ok_or_else(|| Error::malformed("unbalanced text"))?,
                )
            }
            first => {
                let start = offset;
                offset += 1;
                if first == b'"' {
                    let mut escaped = false;
                    while offset < bytes.len() {
                        let byte = bytes[offset];
                        offset += 1;
                        if escaped {
                            escaped = false;
                        } else if byte == b'\\' {
                            escaped = true;
                        } else if byte == b'"' {
                            break;
                        }
                    }
                } else {
                    while offset < bytes.len()
                        && !bytes[offset].is_ascii_whitespace()
                        && !matches!(bytes[offset], b'(' | b')')
                    {
                        offset += 1;
                    }
                }
                let token = &text[start..offset];
                if token == "." {
                    return Err(Error::malformed("improper lists are not supported"));
                }
                lexpr::from_str(token).map_err(Error::malformed)?
            }
        };
        if let Some(frame) = frames.last_mut() {
            frame.push(value);
        } else if root.replace(value).is_some() {
            return Err(Error::malformed("trailing text data"));
        }
    }
    if !frames.is_empty() {
        return Err(Error::malformed("unterminated list"));
    }
    root.ok_or_else(|| Error::malformed("empty text"))
}

/// Borrows field syntax so a domain reader can reject duplicate, unknown and
/// unconsumed fields without translating through a generic semantic map.
pub struct Record<'a> {
    fields: Vec<(&'a str, &'a Value)>,
    used: BTreeSet<&'a str>,
}

impl<'a> Record<'a> {
    pub fn new(value: &'a Value, name: &str) -> Result<Self, Error> {
        let values = list(value)?;
        let Some((tag, fields)) = values.split_first() else {
            return Err(Error::malformed("empty record"));
        };
        if symbol(tag)? != name {
            return Err(Error::malformed(format!("expected {name} record")));
        }
        let mut names = BTreeSet::new();
        let mut result = Vec::with_capacity(fields.len());
        for field in fields {
            let pair = list(field)?;
            let [name, value] = pair.as_slice() else {
                return Err(Error::malformed("expected (FIELD VALUE)"));
            };
            let name = symbol(name)?;
            if !names.insert(name) {
                return Err(Error::malformed(format!("duplicate field {name}")));
            }
            result.push((name, *value));
        }
        Ok(Self {
            fields: result,
            used: BTreeSet::new(),
        })
    }

    pub fn field(&mut self, name: &str) -> Result<&'a Value, Error> {
        let Some((key, value)) = self.fields.iter().find(|(key, _)| *key == name) else {
            return Err(Error::malformed(format!("missing field {name}")));
        };
        self.used.insert(key);
        Ok(value)
    }

    pub fn finish(self) -> Result<(), Error> {
        if let Some((name, _)) = self
            .fields
            .iter()
            .find(|(name, _)| !self.used.contains(name))
        {
            return Err(Error::malformed(format!("unknown field {name}")));
        }
        Ok(())
    }
}

pub fn list(value: &Value) -> Result<Vec<&Value>, Error> {
    let mut values = Vec::new();
    let mut remaining = value;
    while let Some(pair) = remaining.as_cons() {
        values.push(pair.car());
        remaining = pair.cdr();
    }
    if !remaining.is_null() {
        return Err(Error::malformed("expected a proper list"));
    }
    Ok(values)
}

pub fn sequence<'a>(value: &'a Value, budget: &mut Budget) -> Result<Vec<&'a Value>, Error> {
    let values = list(value)?;
    let Some((tag, rest)) = values.split_first() else {
        return Err(Error::malformed("expected list"));
    };
    if symbol(tag)? != "list" {
        return Err(Error::malformed("expected list"));
    }
    budget.charge(rest.len())?;
    Ok(rest.to_vec())
}

pub fn variant(value: &Value) -> Result<(&str, Vec<&Value>), Error> {
    let values = list(value)?;
    let Some((tag, rest)) = values.split_first() else {
        return Err(Error::malformed("expected variant"));
    };
    Ok((symbol(tag)?, rest.to_vec()))
}

pub fn symbol(value: &Value) -> Result<&str, Error> {
    value
        .as_symbol()
        .ok_or_else(|| Error::malformed("expected symbol"))
}

pub fn string(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::malformed("expected string"))
}

pub fn u64(value: &Value) -> Result<u64, Error> {
    value
        .as_u64()
        .ok_or_else(|| Error::malformed("expected unsigned integer"))
}

pub fn u32(value: &Value) -> Result<u32, Error> {
    u64(value)?.try_into().map_err(Error::malformed)
}

pub fn i64(value: &Value) -> Result<i64, Error> {
    value
        .as_i64()
        .ok_or_else(|| Error::malformed("expected signed integer"))
}

pub fn boolean(value: &Value) -> Result<bool, Error> {
    match symbol(value)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(Error::malformed("expected true or false")),
    }
}

pub fn real(value: &Value) -> Result<u64, Error> {
    if let Some(number) = value.as_number().filter(|n| n.is_f64()) {
        let real = number
            .as_f64()
            .ok_or_else(|| Error::malformed("invalid real"))?;
        if real.is_finite() {
            return Ok(real.to_bits());
        }
    }
    let (tag, args) = variant(value)?;
    let [bits] = args.as_slice() else {
        return Err(Error::malformed("expected one real64 bit string"));
    };
    let bits = bits
        .as_str()
        .ok_or_else(|| Error::malformed("expected hexadecimal string"))?;
    if tag != "real64-bits" || bits.len() != 16 || !bits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::malformed("expected sixteen hexadecimal real64 bits"));
    }
    u64::from_str_radix(bits, 16).map_err(Error::malformed)
}
