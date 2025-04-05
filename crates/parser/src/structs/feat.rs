use crate::{
    toml::{Feature, FeatureArgKind, Positioned},
    tree::Value,
};

pub fn get_features(value: &Value) -> Option<Vec<Positioned<Feature>>> {
    let tree = value.as_tree()?;
    let mut out = vec![];
    for tree_value in tree.nodes.iter() {
        let name = tree_value.key.to_positioned();
        match &tree_value.value {
            Value::Array { value, .. } => {
                let args = value
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|v| v.into())
                    .collect::<Vec<_>>();
                let max = args
                    .iter()
                    .map(|v: &FeatureArgKind| v.range().end)
                    .max()
                    .unwrap_or(tree_value.key.range.end);
                out.push(Positioned {
                    start: tree_value.key.range.start,
                    end: tree_value.key.range.end.max(max),
                    data: Feature { name, args },
                });
            }
            Value::NoContent => out.push(Positioned {
                start: tree_value.key.range.start,
                end: tree_value.key.range.end,
                data: Feature { name, args: vec![] },
            }),
            _ => continue,
        };
    }
    Some(out)
}
