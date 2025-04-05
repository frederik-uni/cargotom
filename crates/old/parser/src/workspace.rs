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

impl Cargo {}
