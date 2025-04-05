use std::{collections::HashMap, sync::Arc};

use parser::{
    toml::{DepSource, Dependency, DependencyKind, Positioned},
    tree::RangeExclusive,
    Db,
};
use rust_version::RustVersion;
use tokio::sync::RwLockReadGuard;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use crate::lsp::Context;

impl Context {
    pub fn upgrade_dep(
        &self,
        uri: &Url,
        version: &Positioned<String>,
        ver: Option<RustVersion>,
        lock: &RwLockReadGuard<Db>,
    ) -> Option<CodeAction> {
        let ver = ver?;
        let version_parsed = RustVersion::try_from(version.data.as_str()).ok()?;
        if ver <= version_parsed {
            return None;
        }
        let start = lock.get_offset(&uri, version.start as usize)?;
        let end = lock.get_offset(&uri, version.end as usize)?;

        Some(CodeAction {
            title: "Upgrade".to_owned(),
            kind: Some(CodeActionKind::EMPTY),
            edit: Some(WorkspaceEdit {
                changes: Some(
                    vec![(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range::new(
                                Position {
                                    line: start.0 as u32,
                                    character: start.1 as u32,
                                },
                                Position {
                                    line: end.0 as u32,
                                    character: end.1 as u32,
                                },
                            ),
                            new_text: format!("\"{}\"", ver.to_owned()),
                        }],
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        })
    }
    fn dep_workspace_actions(
        &self,
        uri: &Url,
        dep: &Positioned<parser::toml::Dependency>,
        range: &Range,
        lock: &RwLockReadGuard<Db>,
    ) -> Option<Vec<CodeAction>> {
        if let DepSource::Workspace(_) = dep.data.source {
            return None;
        }
        let workspace_uri = lock.get_workspace(uri)?;
        let workspace = lock.get_toml(workspace_uri)?;
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        let dep_name = &dep.data.name.data;
        if workspace
            .dependencies
            .iter()
            .find(|v| &v.data.name.data == dep_name)
            .is_none()
        {
            let last = workspace.dependencies.last()?;
            let line = lock.get_line(workspace_uri, last.end as usize)? as u32 + 1;
            let dep = Dependency {
                name: dep.data.name.clone(),
                kind: DependencyKind::Normal,
                source: dep.data.source.clone(),
                features: Positioned::new(0, 0, Vec::new()),
                features_key_range: None,
                default_features: None,
                typing_keys: Vec::new(),
                optional: None,
                expanded: true,
                target: Arc::default(),
            };
            changes
                .entry(workspace_uri.clone())
                .or_default()
                .push(TextEdit {
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position { line, character: 0 },
                    },
                    new_text: format!("{}\n", dep.to_string()),
                });
        }
        let mut data = dep.data.clone();
        data.source = DepSource::Workspace(RangeExclusive::default());
        changes.entry(uri.clone()).or_default().push(TextEdit {
            range: range.clone(),
            new_text: data.to_string(),
        });
        let make = CodeAction {
            title: "Make to workspace dependency".to_owned(),
            kind: Some(CodeActionKind::EMPTY),
            edit: {
                Some(WorkspaceEdit {
                    change_annotations: None,
                    document_changes: None,
                    changes: Some(changes),
                })
            },
            ..CodeAction::default()
        };
        Some(vec![make])
    }
    pub fn dep_actions(
        &self,
        uri: &Url,
        dep: &Positioned<parser::toml::Dependency>,
        lock: &RwLockReadGuard<Db>,
    ) -> Option<Vec<CodeAction>> {
        let start = lock.get_offset(uri, dep.start as usize)?;
        let end = lock.get_offset(uri, dep.end as usize)?;
        let mut data1 = dep.data.clone();
        let mut data2 = dep.data.clone();
        let mut res = vec![];
        let range = Range::new(
            Position {
                line: start.0 as u32,
                character: start.1 as u32,
            },
            Position {
                line: end.0 as u32,
                character: end.1 as u32,
            },
        );
        if let Some(v) = self.dep_workspace_actions(uri, dep, &range, lock) {
            res.extend(v);
        }
        let expand = CodeAction {
            title: match data1.expanded {
                true => "Collapse",
                false => "Expand",
            }
            .to_owned(),
            kind: Some(CodeActionKind::EMPTY),
            edit: {
                data1.expanded = !data1.expanded;
                data1.optional = None;
                Some(WorkspaceEdit {
                    change_annotations: None,
                    document_changes: None,
                    changes: Some(
                        vec![(
                            uri.clone(),
                            vec![TextEdit {
                                range,
                                new_text: data1.to_string(),
                            }],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                })
            },
            ..CodeAction::default()
        };
        let optional = CodeAction {
            title: match data2.optional.map(|v| v.data).unwrap_or_default() {
                true => "Remove optional",
                false => "Make optional",
            }
            .to_owned(),
            kind: Some(CodeActionKind::EMPTY),
            edit: {
                if let Some(opt) = &mut data2.optional {
                    opt.data = !opt.data;
                } else {
                    data2.optional = Some(Positioned::new(0, 0, true))
                }
                Some(WorkspaceEdit {
                    change_annotations: None,
                    document_changes: None,
                    changes: Some(
                        vec![(
                            uri.clone(),
                            vec![TextEdit {
                                range,
                                new_text: data2.to_string(),
                            }],
                        )]
                        .into_iter()
                        .collect(),
                    ),
                })
            },
            ..CodeAction::default()
        };
        res.push(expand);
        res.push(optional);
        Some(res)
    }
}
