use std::sync::Arc;

use crate::{
    dependencies::get_dependencies,
    structure::{DependencyKind, Positioned, Target, Value},
    Cargo,
};

fn get_workspace_members(value: &Value) -> Option<Vec<String>> {
    let value = value
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str())
        .map(|s| s.data)
        .collect::<Vec<_>>();
    Some(value)
}

impl Cargo {
    pub(crate) fn generate_workspace(
        &mut self,
        value: &Value,
        targets: Arc<Vec<Positioned<Target>>>,
    ) -> Option<Vec<Positioned<()>>> {
        let tree = value.as_tree()?;
        for tree_value in tree.0.iter() {
            match tree_value.key.value.as_str() {
                "members" | "default-members" => {
                    self.info.add_workspace_members(
                        get_workspace_members(&tree_value.value).unwrap_or_default(),
                    );
                }
                "dependencies" => {
                    let deps = get_dependencies(
                        &tree_value.value,
                        DependencyKind::Normal,
                        targets.clone(),
                    )
                    .unwrap_or_default();
                    self.positioned_info.dependencies.extend(deps);
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

        None
    }
}
