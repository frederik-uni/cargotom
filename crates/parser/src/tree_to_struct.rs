use std::sync::Arc;

use crate::{
    dependencies::get_dependencies,
    structure::{Positioned, Target, Tree},
};

use super::{
    features::get_features,
    structure::{Cargo, DependencyKind},
};

impl Cargo {
    pub(crate) fn update_struct(&mut self, tree: &Tree, target: Arc<Vec<Positioned<Target>>>) {
        for value in tree.0.iter() {
            match value.key.value.as_str() {
                // unimportant
                "package" => {}
                "lib" => {}
                "bin" => {}
                "example" => {}
                "test" => {}
                "bench" => {}
                // important position
                "dependencies" => {
                    let deps =
                        get_dependencies(&value.value, DependencyKind::Normal, target.clone())
                            .unwrap_or_default();
                    self.positioned_info.dependencies.extend(deps);
                }
                "dev-dependencies" => {
                    let deps =
                        get_dependencies(&value.value, DependencyKind::Development, target.clone())
                            .unwrap_or_default();
                    self.positioned_info.dependencies.extend(deps);
                }
                "build-dependencies" => {
                    let deps =
                        get_dependencies(&value.value, DependencyKind::Build, target.clone())
                            .unwrap_or_default();
                    self.positioned_info.dependencies.extend(deps);
                }
                "target" => {
                    for tree in value.value.as_tree().unwrap().0.iter() {
                        let key = &tree.key.value;
                        let targets = if key.ends_with(")") {
                            if let Some(v) = key.strip_prefix("cfg(") {
                                let value = v.strip_suffix(")").unwrap();
                                Arc::new(vec![Positioned {
                                    start: tree.key.range.start + 4,
                                    end: tree.key.range.end - 1,
                                    data: Target::Unknown(value.to_string()),
                                }])
                            } else {
                                Arc::new(vec![Positioned {
                                    start: tree.key.range.start,
                                    end: tree.key.range.end,
                                    data: Target::Unknown(key.to_string()),
                                }])
                            }
                        } else {
                            Arc::new(vec![Positioned {
                                start: tree.key.range.start,
                                end: tree.key.range.end,
                                data: Target::Unknown(key.to_string()),
                            }])
                        };
                        self.update_struct(tree.value.as_tree().unwrap(), targets);
                    }
                }
                "features" => {
                    self.positioned_info
                        .features
                        .extend(get_features(&value.value).unwrap_or_default());
                }
                // important info
                "workspace" => {
                    self.generate_workspace(&value.value, target.clone());
                }
                // unimportant
                "badges" => {}
                "lints" => {}
                "patch" => {}
                "replace" => {}
                "profile" => {}
                _ => {}
            }
        }
    }
}

// cargo-features — Unstable, nightly-only features.
