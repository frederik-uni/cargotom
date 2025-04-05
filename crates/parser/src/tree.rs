use taplo::{
    dom::{
        node::{Table, TableKind},
        Node,
    },
    rowan::TextRange,
};

use crate::toml::Positioned;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tree {
    pub nodes: Vec<TreeValue>,
    pub kind: TableKind,
    pub pos: RangeExclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeValue {
    pub key: Key,
    pub value: Value,
    pub pos: RangeExclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) range: RangeExclusive,
    pub(crate) value: String,
}

impl Key {
    pub fn to_positioned(&self) -> Positioned<String> {
        Positioned {
            start: self.range.start,
            end: self.range.end,
            data: self.value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Tree {
        value: Tree,
        range: RangeExclusive,
    },
    NoContent,
    Array {
        value: Vec<Value>,
        range: RangeExclusive,
    },
    String {
        value: String,
        range: RangeExclusive,
    },
    Bool {
        value: bool,
        range: RangeExclusive,
    },
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub struct RangeExclusive {
    pub start: u32,
    pub end: u32,
}

impl<T> From<Positioned<T>> for RangeExclusive {
    fn from(value: Positioned<T>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl<T> From<&Positioned<T>> for RangeExclusive {
    fn from(value: &Positioned<T>) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl RangeExclusive {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }

    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset as u32 && offset as u32 <= self.end
    }

    pub fn join(&self, other: &RangeExclusive) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

impl From<&Table> for Tree {
    fn from(table: &Table) -> Self {
        let nodes: Vec<TreeValue> = table
            .entries()
            .get()
            .iter()
            .map(|(key, value)| {
                let key = Key {
                    range: key.text_ranges().next().unwrap().into(),
                    value: key.value().to_string(),
                };
                let value = Value::from(value);
                TreeValue {
                    pos: value
                        .range()
                        .map(|v| key.range.join(&v))
                        .unwrap_or(key.range),
                    value: match value.range() == Some(key.range) {
                        true => Value::NoContent,
                        false => value,
                    },
                    key,
                }
            })
            .collect();
        Self {
            nodes,
            kind: table.kind(),
            pos: match table.inner.syntax.as_ref().unwrap() {
                taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
            }
            .into(),
        }
    }
}

impl From<&Node> for Value {
    fn from(node: &Node) -> Self {
        match node {
            taplo::dom::Node::Table(table) => Value::Tree {
                value: Tree::from(table),
                range: match table.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            },
            taplo::dom::Node::Array(arr) => Value::Array {
                value: arr.items().get().iter().map(Self::from).collect(),
                range: match arr.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            },
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
            taplo::dom::Node::Integer(_) => Value::Unknown,
            taplo::dom::Node::Float(_) => Value::Unknown,
            taplo::dom::Node::Date(_) => Value::Unknown,
        }
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

pub fn str_to_positioned(str: &str, range: &RangeExclusive) -> Positioned<String> {
    Positioned {
        start: range.start,
        end: range.end,
        data: str.to_string(),
    }
}

impl TreeValue {
    pub fn range(&self) -> RangeExclusive {
        let mut min: u32 = self.key.range.start;
        let mut max: u32 = self.key.range.end;
        if let Some(range) = self.value.range() {
            min = min.min(range.start);
            max = max.max(range.end);
        }
        RangeExclusive {
            start: min,
            end: max,
        }
    }
}

impl Value {
    pub fn as_str(&self) -> Option<Positioned<String>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array { .. } => None,
            Value::String { value, range } => Some(str_to_positioned(value, range)),
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }
    pub fn as_tree(&self) -> Option<&Tree> {
        match self {
            Value::Tree { value, .. } => Some(value),
            Value::NoContent => None,
            Value::Array { .. } => None,
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array { value, .. } => Some(value),
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }

    pub fn as_bool(&self) -> Option<Positioned<bool>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array { .. } => None,
            Value::String { .. } => None,
            Value::Bool { value, range } => Some(Positioned {
                start: range.start,
                end: range.end,
                data: *value,
            }),
            Value::Unknown => None,
        }
    }

    pub fn range(&self) -> Option<RangeExclusive> {
        match self {
            Value::Tree { range, value } => {
                let mut min: u32 = range.start;
                let mut max: u32 = range.end;
                for item in value.nodes.iter() {
                    min = min.min(item.key.range.start);
                    max = max.max(item.key.range.end);

                    if let Some(range) = item.value.range() {
                        min = min.min(range.start);
                        max = max.max(range.end);
                    }
                }

                Some(RangeExclusive {
                    start: min,
                    end: max,
                })
            }
            Value::NoContent => None,
            Value::Array { .. } => None,
            Value::String { range, .. } => Some(*range),
            Value::Bool { range, .. } => Some(*range),
            Value::Unknown => None,
        }
    }
}
