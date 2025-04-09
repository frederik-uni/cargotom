use std::{collections::HashMap, sync::Arc};

use indexmap::IndexMap;
use regex::bytes::Regex;
use serde::Deserialize;

pub type Elements = IndexMap<String, Element>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub description: Option<String>,
    pub contents: ElementKind,
    pub values: Option<Values>,
    pub default: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Values {
    NoDetail(Vec<String>),
    Detail(HashMap<String, String>),
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum ElementKind {
    Id(String),
    Complex(Elements),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Data {
    #[serde(rename = "$schema")]
    pub schema: Elements,
    #[serde(rename = "$components")]
    pub components: Elements,
}

#[derive(Debug)]
pub struct Val {
    ty: Vec<Types>,
    default: Option<String>,
    detail: Option<String>,
    values: Option<HashMap<String, Option<String>>>,
}

#[derive(Debug)]
pub enum Types {
    Map(Parsed),
    Element(Arc<Val>),
    String,
    Bool,
    Object,
    Array(Box<Types>),
}

impl Types {
    pub fn search(&self, keys: &[String], index: usize, is_value: bool) -> Option<String> {
        match self {
            Types::Map(parsed) => parsed.get_detail(keys, index, is_value),
            Types::Array(types) => types.search(keys, index, is_value),
            Types::Element(val) => val.ty.iter().find_map(|v| v.search(keys, index, is_value)),
            _ => None,
        }
    }
    pub fn end_doc(&self) -> Option<String> {
        match self {
            Types::Element(val) => val
                .detail
                .clone()
                .or(val.ty.iter().find_map(|v| v.end_doc())),
            Types::Array(types) => types.end_doc(),
            _ => None,
        }
    }
    pub fn end_doc_value(&self, key: &str) -> Option<String> {
        match self {
            Types::Element(val) => val
                .values
                .as_ref()
                .and_then(|v| v.get(key).cloned())
                .flatten()
                .or(val.ty.iter().find_map(|v| v.end_doc_value(key))),
            Types::Array(types) => types.end_doc_value(key),
            _ => None,
        }
    }
}

fn parse_type(v: &str, info: &HashMap<String, Arc<Val>>) -> Types {
    if v == "string" {
        Types::String
    } else if v == "bool" {
        Types::Bool
    } else if v == "object" {
        Types::Object
    } else if v.starts_with("array<") && v.ends_with('>') {
        let inner = &v[6..v.len() - 1];
        let inner_type = parse_type(inner, info);
        Types::Array(Box::new(inner_type))
    } else {
        let item = info.get(v).expect(v).clone();
        Types::Element(item)
    }
}

#[derive(Debug)]
pub enum Key {
    Exact(String),
    Pattern(Regex),
}

impl Key {
    fn is_match(&self, key: &str) -> bool {
        match self {
            Key::Exact(k) => k == key,
            Key::Pattern(r) => r.is_match(key.as_bytes()),
        }
    }
}

#[derive(Debug)]
pub struct Parsed {
    pub entries: Vec<(Key, Arc<Val>)>,
}

impl Parsed {
    pub fn get_detail(&self, keys: &[String], index: usize, is_value: bool) -> Option<String> {
        if index >= keys.len() {
            return None;
        }
        let key = &keys[index];
        let last = index == keys.len() - 1;
        let onel = index + 1 == keys.len() - 1;
        let item = &self.entries.iter().find(|(k, _)| k.is_match(&key))?.1;

        match (last, onel, is_value) {
            (true, true, _) => unreachable!(),
            (true, false, _) => item
                .detail
                .clone()
                .or(item.ty.iter().find_map(|v| v.end_doc())),
            (false, true, true) => {
                let next = &keys[index + 1];
                item.values
                    .as_ref()
                    .and_then(|v| v.get(next).cloned().flatten())
                    .or(item.ty.iter().find_map(|v| v.end_doc_value(next)))
            }
            (false, true, false) | (false, false, _) => item
                .ty
                .iter()
                .find_map(|v| v.search(keys, index + 1, is_value)),
        }
    }
}

pub fn parse_all() -> Parsed {
    let v: Data = serde_json::from_str(include_str!("../cargo.json")).unwrap();
    let mut map = HashMap::new();
    for (k, v) in v.components {
        let res = parse(v, &map);
        map.insert(k, Arc::new(res));
    }
    let mut out = vec![];
    for (k, v) in v.schema {
        let res = parse(v, &map);
        let c = Arc::new(res);
        if k == "package" {
            map.insert(format!("$package"), c.clone());
            out.push((Key::Exact(k), c));
        } else if let Some(v) = k.strip_prefix("$") {
            out.push((Key::Pattern(Regex::new(v).unwrap()), c));
        } else {
            out.push((Key::Exact(k), c));
        }
    }
    Parsed { entries: out }
}

fn parse(t: Element, info: &HashMap<String, Arc<Val>>) -> Val {
    let ty = match t.contents {
        ElementKind::Id(id) => id
            .split("|")
            .map(|v| v.trim())
            .map(|v| parse_type(v, info))
            .collect::<Vec<_>>(),
        ElementKind::Complex(hash_map) => {
            vec![Types::Map(Parsed {
                entries: hash_map
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            match k.strip_prefix("$") {
                                Some(v) => Key::Pattern(Regex::new(v).unwrap()),
                                None => Key::Exact(k),
                            },
                            Arc::new(parse(v, info)),
                        )
                    })
                    .collect(),
            })]
        }
    };
    Val {
        ty,
        default: t.default,
        detail: t.description,
        values: t.values.map(|v| match v {
            Values::NoDetail(v) => v.into_iter().map(|v| (v, None::<String>)).collect(),
            Values::Detail(v) => v.into_iter().map(|v| (v.0, Some(v.1))).collect(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use crate::static_structure::parse_all;

    #[tokio::test]
    async fn parse_test() {
        let all = parse_all();
        println!("{:#?}", all);
    }
}
