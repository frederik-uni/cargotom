use serde::{Deserialize, Serialize};
use taplo::{
    dom::{node::Table, Node},
    parser::parse,
    rowan::TextRange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tree(Vec<TreeValue>);

impl Tree {
    pub fn by_key(&self, key: &Key) -> Option<&Tree> {
        match self.0.iter().find(|v| &v.key == key) {
            Some(_) => Some(self),
            None => self.0.iter().find_map(|v| v.value.by_key(key)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeValue {
    key: Key,
    value: Value,
}

impl Value {
    pub fn as_str(&self) -> Option<String> {
        match self {
            Value::String { value, .. } => Some(value.to_string()),
            _ => None,
        }
    }
    fn by_key(&self, key: &Key) -> Option<&Tree> {
        match self {
            Value::Tree(v) => v.by_key(key),
            Value::NoContent => None,
            Value::Array(v) => v.iter().find_map(|v| v.by_key(key)),
            Value::String { .. } => None,
            Value::Bool { .. } => None,
        }
    }
}

impl Value {
    fn find(&self, str: &str) -> Vec<&TreeValue> {
        match self {
            Value::Tree(tree) => tree.find(str),
            Value::Array(v) => v.iter().flat_map(|v| v.find(str)).collect(),
            _ => vec![],
        }
    }

    fn get_item_by_pos(&self, byte_offset: u32) -> Option<Vec<KeyOrValue<'_>>> {
        let mut path = vec![];
        match self {
            Value::Tree(v) => {
                if let Some(mut v) = v.get_item_by_pos(byte_offset) {
                    path.append(&mut v);
                }
            }
            Value::Array(arr) => {
                let i = arr.iter().find_map(|v| v.get_item_by_pos(byte_offset));
                if let Some(mut v) = i {
                    path.append(&mut v);
                }
            }
            Value::NoContent => {}
            Value::String { range, .. } => {
                if range.contains_inclusive(byte_offset) {
                    path.push(KeyOrValue::Value(self))
                }
            }
            Value::Bool { range, .. } => {
                if range.contains_inclusive(byte_offset) {
                    path.push(KeyOrValue::Value(self))
                }
            }
        }
        match path.is_empty() {
            true => None,
            false => Some(path),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    range: RangeExclusive,
    pub(crate) value: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub struct RangeExclusive {
    start: u32,
    end: u32,
}

impl RangeExclusive {
    pub fn contains_inclusive(&self, pos: u32) -> bool {
        self.start <= pos && pos <= self.end
    }
}

impl From<TextRange> for RangeExclusive {
    fn from(value: TextRange) -> Self {
        RangeExclusive {
            start: u32::from(value.start()),
            end: u32::from(value.end()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Tree(Tree),
    NoContent,
    Array(Vec<Value>),
    String {
        value: String,
        range: RangeExclusive,
    },
    Bool {
        value: bool,
        range: RangeExclusive,
    },
}

impl Value {
    fn range(&self) -> Option<&RangeExclusive> {
        match self {
            Value::Tree(_) => None,
            Value::NoContent => None,
            Value::Array(_) => None,
            Value::String { range, .. } => Some(range),
            Value::Bool { range, .. } => Some(range),
        }
    }
    fn from_node(node: &Node) -> Option<Self> {
        Some(match node {
            taplo::dom::Node::Table(table) => Value::Tree(Tree::from_table(table)),
            taplo::dom::Node::Array(arr) => Value::Array(
                arr.items()
                    .get()
                    .iter()
                    .filter_map(Self::from_node)
                    .collect(),
            ),
            taplo::dom::Node::Bool(b) => Value::Bool {
                value: b.value(),
                range: match b.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            },
            taplo::dom::Node::Str(s) => {
                let range = match s.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into();
                Value::String {
                    range,
                    value: s.value().to_string(),
                }
            }
            taplo::dom::Node::Invalid(invalid) => {
                let range = match invalid.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                };
                let v = match invalid.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text().to_string(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text().to_string(),
                };
                Value::String {
                    value: v,
                    range: range.into(),
                }
            }
            taplo::dom::Node::Integer(_) => None?,
            taplo::dom::Node::Float(_) => None?,
            taplo::dom::Node::Date(_) => None?,
        })
    }
}

#[derive(Debug)]
pub enum KeyOrValue<'a> {
    Key(&'a Key),
    Value(&'a Value),
}

impl<'a> KeyOrValue<'a> {
    pub fn owned(&self) -> KeyOrValueOwned {
        match self {
            KeyOrValue::Key(v) => KeyOrValueOwned::Key((*v).clone()),
            KeyOrValue::Value(v) => KeyOrValueOwned::Value((*v).clone()),
        }
    }
}

#[derive(Debug)]
pub enum KeyOrValueOwned {
    Key(Key),
    Value(Value),
}

impl KeyOrValueOwned {
    pub fn as_key(&self) -> Option<&Key> {
        match self {
            KeyOrValueOwned::Key(key) => Some(key),
            KeyOrValueOwned::Value(_) => None,
        }
    }
}

impl Tree {
    pub fn get(&self, str: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|v| v.key.value.as_str() == str)
            .map(|v| &v.value)
    }
    pub fn find(&self, str: &str) -> Vec<&TreeValue> {
        let mut out = vec![];
        for item in &self.0 {
            if item.key.value.as_str() == str {
                out.push(item);
            }
            out.append(&mut item.value.find(str));
        }
        out
    }
    pub fn get_item_by_pos(&self, byte_offset: u32) -> Option<Vec<KeyOrValue<'_>>> {
        let mut path = vec![];
        for item in &self.0 {
            if item.key.range.contains_inclusive(byte_offset) {
                return Some(vec![KeyOrValue::Key(&item.key)]);
            } else if let Some(mut v) = item.value.get_item_by_pos(byte_offset) {
                path.push(KeyOrValue::Key(&item.key));
                path.append(&mut v);
                return Some(path);
            }
        }
        None
    }
    fn from_table(table: &Table) -> Self {
        Tree(
            table
                .entries()
                .get()
                .iter()
                .filter_map(|(key, value)| {
                    let key = Key {
                        range: key.text_ranges().next().unwrap().into(),
                        value: key.value().to_string(),
                    };

                    Value::from_node(value).map(|value| TreeValue {
                        value: match value.range() == Some(&key.range) {
                            true => Value::NoContent,
                            false => value,
                        },
                        key,
                    })
                })
                .collect(),
        )
    }
}
pub(crate) fn parse_toml(src: &str) -> Tree {
    let tree = parse(src);
    let dom = tree.into_dom();
    let table = dom.as_table().unwrap();
    Tree::from_table(table)
}

pub fn get_after_key<'a, 'b>(
    key: &str,
    items: &'b [KeyOrValue<'a>],
) -> Option<Vec<&'b KeyOrValue<'a>>> {
    let mut found = false;
    let mut vec = vec![];
    for item in items.iter() {
        if found {
            vec.push(item);
        } else if match item {
            KeyOrValue::Key(k) => k.value == key,
            KeyOrValue::Value(_) => false,
        } {
            found = true;
        }
    }
    match vec.is_empty() {
        true => None,
        false => Some(vec),
    }
}
