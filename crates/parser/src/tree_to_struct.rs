use std::sync::Arc;

use crate::{
    structs::{deps::get_dependencies, feat::get_features},
    toml::{Dependency, DependencyKind, Positioned, Target, Toml},
    tree::Value,
    Tree,
};

pub fn to_struct(tree: &Tree, target: Arc<Vec<Positioned<Target>>>) -> Toml {
    let mut dep = vec![];
    let mut features = vec![];
    let mut tar = vec![];
    let mut mem = vec![];
    for value in tree.nodes.iter() {
        match value.key.value.as_str() {
            "profile" | "badges" | "lints" | "patch" | "replace" | "bench" | "test" | "example"
            | "package" | "lib" | "bin" => { /* ignore */ }
            "dependencies" => {
                let deps = get_dependencies(&value.value, DependencyKind::Normal, target.clone())
                    .unwrap_or_default();
                dep.extend(deps);
            }
            "dev-dependencies" => {
                let deps =
                    get_dependencies(&value.value, DependencyKind::Development, target.clone())
                        .unwrap_or_default();
                dep.extend(deps);
            }
            "build-dependencies" => {
                let deps = get_dependencies(&value.value, DependencyKind::Build, target.clone())
                    .unwrap_or_default();
                dep.extend(deps);
            }
            "target" => {
                for tree in value.value.as_tree().unwrap().nodes.iter() {
                    let key = &tree.key.value;
                    let range = tree.key.closest_range(tree.pos.end);
                    let targets = if key.ends_with(")") {
                        if let Some(v) = key.strip_prefix("cfg(") {
                            let value = v.strip_suffix(")").unwrap();
                            Arc::new(vec![Positioned {
                                start: range.start + 4,
                                end: range.end - 1,
                                data: Target::Unknown(value.to_string()),
                            }])
                        } else {
                            Arc::new(vec![Positioned {
                                start: range.start,
                                end: range.end,
                                data: Target::Unknown(key.to_string()),
                            }])
                        }
                    } else {
                        Arc::new(vec![Positioned {
                            start: range.start,
                            end: range.end,
                            data: Target::Unknown(key.to_string()),
                        }])
                    };
                    tar.push(to_struct(tree.value.as_tree().unwrap(), targets));
                }
            }
            "features" => {
                features.extend(get_features(&value.value).unwrap_or_default());
            }
            "workspace" => {
                mem.push(generate_workspace(&value.value, target.clone()));
            }
            _ => {}
        }
    }
    let workspace = !mem.is_empty();
    let (children, dep_w) = mem.into_iter().filter_map(|v| v).fold(
        (Vec::new(), Vec::new()),
        |(mut acc_strs, mut acc_deps), (strs, deps)| {
            acc_strs.extend(strs);
            acc_deps.extend(deps);
            (acc_strs, acc_deps)
        },
    );
    let tar = tar.into_iter().reduce(|acc, i| acc.join(i));
    let r = Toml {
        workspace,
        children,
        dependencies: match workspace {
            true => dep_w,
            false => dep,
        },
        features,
    };
    match tar {
        Some(tar) => r.join(tar),
        None => r,
    }
}

fn get_workspace_members(value: &Value) -> Option<Vec<String>> {
    let value = value
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.data)
        .collect::<Vec<_>>();
    Some(value)
}

pub(crate) fn generate_workspace(
    value: &Value,
    targets: Arc<Vec<Positioned<Target>>>,
) -> Option<(Vec<String>, Vec<Positioned<Dependency>>)> {
    let mut depend = vec![];
    let mut mem = vec![];
    let tree = value.as_tree()?;
    for tree_value in tree.nodes.iter() {
        match tree_value.key.value.as_str() {
            "members" | "default-members" => {
                mem.extend(get_workspace_members(&tree_value.value).unwrap_or_default());
            }
            "dependencies" => {
                let deps =
                    get_dependencies(&tree_value.value, DependencyKind::Normal, targets.clone())
                        .unwrap_or_default();
                depend.extend(deps);
            }
            // unimportant keys
            "resolver" => {}
            "exclude" => {}
            "package" => {}
            "lints" => {}
            "metadata" => {}
            _ => {}
        }
    }

    Some((mem, depend))
}
