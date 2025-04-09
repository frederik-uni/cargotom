use taplo::{
    dom::{node::Table, Node},
    parser::parse,
    rowan::TextRange,
};

use super::structure::{CargoRawData, Key, RangeExclusive, Tree, TreeValue, Value};

impl CargoRawData {
    pub(crate) fn generate_tree(&mut self) {
        let tree = parse(&self.string);
        let dom = tree.into_dom();
        self.tree = dom.as_table().map(Tree::from).unwrap_or_default();
    }
}

impl From<&Table> for Tree {
    fn from(table: &Table) -> Self {
        Tree(
            table
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
                        value: match value.range() == Some(key.range) {
                            true => value,
                            false => value,
                        },
                        key,
                    }
                })
                .collect(),
        )
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
            taplo::dom::Node::Array(arr) => {
                Value::Array(arr.items().get().iter().map(Self::from).collect())
            }
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
