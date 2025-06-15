use std::sync::Arc;

use taplo::dom::node::TableKind;

use crate::{
    toml::{DepSource, Dependency, DependencyKind, OptionalKey, Positioned, Target, WithKey},
    tree::{str_to_positioned, Value},
    Tree,
};

pub fn get_dependencies(
    value: &Value,
    kind: DependencyKind,
    targets: Arc<Vec<Positioned<Target>>>,
) -> Option<Vec<Positioned<Dependency>>> {
    let tree = value.as_tree()?;
    let mut out = vec![];
    for dep_tree in tree.nodes.iter() {
        let name = dep_tree.key.to_positioned(dep_tree.pos.end);
        let mut dep = Dependency {
            name,
            package: None,
            kind,
            expanded: true,
            source: DepSource::None,
            features: Positioned::new(0, 0, vec![]),
            optional: None,
            target: targets.clone(),
            default_features: None,
            features_key_range: None,
            typing_keys: vec![],
        };
        match &dep_tree.value {
            Value::Tree { value, .. } => {
                dependency_tree_format_parser(value, &mut dep);
                dep.expanded = value.kind == TableKind::Inline
            }
            Value::String { value, range } => {
                dep.source = DepSource::Version {
                    value: OptionalKey::no_key(str_to_positioned(value, range)),
                    registry: None,
                };
                dep.expanded = false;
            }
            Value::NoContent => {
                dep.expanded = false;
            }
            _ => continue,
        }
        let range = dep_tree.range();
        out.push(Positioned {
            start: range.start,
            end: range.end,
            data: dep,
        });
    }
    Some(out)
}

fn dependency_tree_format_parser(value: &Tree, dep: &mut Dependency) {
    for tree_value in value.nodes.iter() {
        match tree_value.key.value.as_str() {
            "version" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_version(OptionalKey::with_key(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => {
                    let pos = tree_value.key.closest_range(tree_value.pos.end);
                    dep.source.set_version(OptionalKey::with_key(
                        pos,
                        Positioned {
                            start: pos.end,
                            end: pos.end,
                            data: String::new(),
                        },
                    ));
                    continue;
                }
            },
            "registry" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_registry(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "git" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_git(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "path" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_path(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "branch" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_branch(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "tag" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_tag(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "rev" => match tree_value.value.as_str() {
                Some(value) => dep.source.set_rev(WithKey::new(
                    tree_value.key.closest_range(tree_value.pos.end),
                    value,
                )),
                None => continue,
            },
            "features" => match &tree_value.value {
                Value::Array { value, range } => {
                    let range = tree_value.pos.join(&range);
                    dep.features.start = range.start;
                    dep.features.end = range.end;
                    dep.features_key_range = Some(tree_value.key.closest_range(tree_value.pos.end));
                    for feature in value.iter() {
                        let feature = feature.as_str();
                        if let Some(feature) = feature {
                            dep.features.data.push(feature);
                        }
                    }
                }
                _ => {
                    dep.features_key_range = Some(tree_value.key.closest_range(tree_value.pos.end));
                    continue;
                }
            },
            "default-features" | "default_features" => match tree_value.value.as_bool() {
                Some(value) => dep.default_features = Some(value),
                None => continue,
            },
            "package" => match tree_value.value.as_str() {
                Some(value) => dep.package = Some(value),
                None => continue,
            },
            "optional" => match tree_value.value.as_bool() {
                Some(value) => dep.optional = Some(value),
                None => continue,
            },
            "workspace" => dep.source.set_workspace(match tree_value.value.range() {
                Some(v) => v.join(&tree_value.pos),
                None => tree_value.pos,
            }),
            _ => {
                if matches!(tree_value.value, Value::Unknown(_)) {
                    dep.typing_keys
                        .push(tree_value.key.to_positioned(tree_value.pos.end));
                } else {
                    dep.typing_keys
                        .push(tree_value.key.to_positioned(tree_value.pos.end));
                }
            }
        }
    }
}
