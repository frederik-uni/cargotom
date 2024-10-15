use serde::{Deserialize, Serialize};

use crate::util::str_to_positioned;

use super::positioned::Positioned;

#[derive(Default, Debug)]
pub struct CargoRawData {
    /// raw file content
    pub(crate) string: String,
    /// parsed file content
    pub(crate) tree: Tree,
}

impl CargoRawData {
    pub fn text(&self) -> &str {
        &self.string
    }
    pub fn text_mut(&mut self) -> &mut String {
        &mut self.string
    }
    pub fn new(content: String) -> Self {
        Self {
            string: content,
            tree: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Tree(pub Vec<TreeValue>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreeValue {
    pub key: Key,
    pub value: Value,
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Copy)]
pub struct RangeExclusive {
    pub start: u32,
    pub end: u32,
}

impl RangeExclusive {
    pub fn contains_inclusive(&self, pos: u32) -> bool {
        self.start <= pos && pos <= self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Value {
    Tree {
        value: Tree,
        range: RangeExclusive,
    },
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
    Unknown,
}

impl Value {
    pub fn as_str(&self) -> Option<Positioned<String>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array(_) => None,
            Value::String { value, range } => Some(str_to_positioned(value, range)),
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }
    pub fn as_tree(&self) -> Option<&Tree> {
        match self {
            Value::Tree { value, .. } => Some(value),
            Value::NoContent => None,
            Value::Array(_) => None,
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array(value) => Some(value),
            Value::String { .. } => None,
            Value::Bool { .. } => None,
            Value::Unknown => None,
        }
    }

    pub fn as_bool(&self) -> Option<Positioned<bool>> {
        match self {
            Value::Tree { .. } => None,
            Value::NoContent => None,
            Value::Array(_) => None,
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
                for item in value.0.iter() {
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
            Value::Array(_) => None,
            Value::String { range, .. } => Some(*range),
            Value::Bool { range, .. } => Some(*range),
            Value::Unknown => None,
        }
    }
}
