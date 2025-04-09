use std::fmt::Display;

use taplo::{
    dom::{
        node::{DomNode, Table, TableKind},
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

#[derive(Debug)]
pub enum Type {
    TreeKey(String),
    String(String),
    Bool(bool),
    Unknown,
}

impl Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::TreeKey(key) => write!(f, "{}", key),
            Type::String(value) => write!(f, "{}", value),
            Type::Bool(value) => write!(f, "{}", value),
            Type::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Debug)]
pub struct PathValue {
    pub tyoe: Type,
    pub range: RangeExclusive,
}

impl PathValue {
    pub fn is_value(&self, last: Option<&Self>) -> bool {
        if let Some(last) = last {
            if self.range.start == last.range.start {
                return false;
            }
        }
        matches!(self.tyoe, Type::String(_) | Type::Bool(_) | Type::Unknown)
    }
}

impl Tree {
    pub fn path(&self, cursor: usize) -> Vec<PathValue> {
        //TODO: check if in content
        self.nodes
            .iter()
            .find_map(|v| match v.key.ranges.iter().find(|v| v.contains(cursor)) {
                Some(r) => Some(vec![PathValue {
                    tyoe: Type::TreeKey(v.key.value.clone()),
                    range: r.clone(),
                }]),
                None => {
                    let items = v.value.path(cursor);
                    match items.is_empty() {
                        true => None,
                        false => {
                            let start = items.first().unwrap().range.start;

                            let mut out = vec![PathValue {
                                tyoe: Type::TreeKey(v.key.value.clone()),
                                range: v.key.closest_range(start),
                            }];
                            out.extend(items);
                            Some(out)
                        }
                    }
                }
            })
            .or(self
                .nodes
                .iter()
                .find_map(|v| match v.pos.contains(cursor) {
                    true => Some(vec![PathValue {
                        tyoe: Type::TreeKey(v.key.value.clone()),
                        range: v.key.closest_range(v.pos.end),
                    }]),
                    false => None,
                }))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeValue {
    pub key: Key,
    pub value: Value,
    pub pos: RangeExclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) ranges: Vec<RangeExclusive>,
    pub(crate) value: String,
}

impl Key {
    pub fn closest_range(&self, value_start: u32) -> RangeExclusive {
        self.ranges
            .iter()
            .filter(|v| v.end <= value_start)
            .max_by(|a, b| a.end.cmp(&b.end))
            .unwrap_or(self.ranges.first().unwrap())
            .clone()
    }
    pub fn to_positioned(&self, value_start: u32) -> Positioned<String> {
        let range = self.closest_range(value_start);
        Positioned {
            start: range.start,
            end: range.end,
            data: self.value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Tree {
        value: Tree,
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
    Unknown(RangeExclusive),
}

impl Value {
    pub fn path(&self, position: usize) -> Vec<PathValue> {
        match self {
            Value::Tree { value, .. } => value.path(position),
            Value::NoContent => vec![],
            Value::Array { value, .. } => value
                .iter()
                .find_map(|v| {
                    let items = v.path(position);
                    match items.is_empty() {
                        true => None,
                        false => Some(items),
                    }
                })
                .unwrap_or_default(),
            Value::String { value, range } => match range.contains(position) {
                true => vec![PathValue {
                    tyoe: Type::String(value.to_owned()),
                    range: range.clone(),
                }],
                false => vec![],
            },
            Value::Bool { value, range } => match range.contains(position) {
                true => vec![PathValue {
                    tyoe: Type::Bool(*value),
                    range: range.clone(),
                }],
                false => vec![],
            },
            Value::Unknown(r) => vec![PathValue {
                tyoe: Type::Unknown,
                range: r.clone(),
            }],
        }
    }
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
                    ranges: key.text_ranges().map(|v| v.into()).collect(),
                    value: key.value().to_string(),
                };
                let range_ = RangeExclusive::from(value.syntax().unwrap().text_range());

                let value = Value::from(value);
                let closest_range = key.closest_range(range_.end);
                let range = value.range();

                TreeValue {
                    pos: range
                        .map(|v| v.join(&closest_range))
                        .unwrap_or(closest_range)
                        .join(&range_),
                    value: match range == Some(closest_range) {
                        true => Value::NoContent,
                        false => value,
                    },
                    key,
                }
            })
            .collect();
        let v = match table.inner.syntax.as_ref().unwrap() {
            taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
            taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
        }
        .into();
        let pos = nodes
            .iter()
            .map(|v| v.pos)
            .reduce(|a, b| a.join(&b))
            .map(|v| v.join(&v))
            .unwrap_or(v);
        Self {
            nodes,
            kind: table.kind(),
            pos,
        }
    }
}

impl From<&Node> for Value {
    fn from(node: &Node) -> Self {
        match node {
            taplo::dom::Node::Table(table) => Value::Tree {
                value: Tree::from(table),
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
            taplo::dom::Node::Integer(i) => Value::Unknown(
                match i.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            ),
            taplo::dom::Node::Float(f) => Value::Unknown(
                match f.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            ),
            taplo::dom::Node::Date(d) => Value::Unknown(
                match d.inner.syntax.as_ref().unwrap() {
                    taplo::rowan::NodeOrToken::Node(node) => node.text_range(),
                    taplo::rowan::NodeOrToken::Token(token) => token.text_range(),
                }
                .into(),
            ),
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
        let max: u32 = self.pos.end;
        let min: u32 = self.pos.start;
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
            Value::Unknown(_) => None,
        }
    }
    pub fn as_tree(&self) -> Option<&Tree> {
        match self {
            Value::Tree { value, .. } => Some(value),
            Value::NoContent => None,
            Value::Array { .. } => None,
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown(_) => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array { value, .. } => Some(value),
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown(_) => None,
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
            Value::Unknown(_) => None,
        }
    }

    pub fn range(&self) -> Option<RangeExclusive> {
        match self {
            Value::Tree { value } => Some(value.pos),
            Value::NoContent => None,
            Value::Array { range, .. } => Some(*range),
            Value::String { range, .. } => Some(*range),
            Value::Bool { range, .. } => Some(*range),
            Value::Unknown(r) => Some(*r),
        }
    }
}
