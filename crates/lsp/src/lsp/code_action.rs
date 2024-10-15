use parser::structure::Source;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse, Command,
    MessageType, Range, TextEdit,
};

use crate::context::Context;
use crate::util::{get_byte_index_from_position, new_workspace_edit};

impl Context {
    pub async fn code_action_(
        &self,
        params: CodeActionParams,
    ) -> Option<Result<Option<CodeActionResponse>>> {
        let mut actions = vec![];

        if params.range.start.line == 0 || params.range.end.line == 0 {
            self.client
                .log_message(MessageType::INFO, format!("{:?}", params))
                .await;
            let action = CodeAction {
                title: "Open LSP docs".to_string(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "Open LSP docs".to_string(),
                    command: "open_url".to_string(),
                    arguments: Some(vec![serde_json::Value::String(
                        "https://github.com/frederik-uni/cargotom/blob/main/README.md".to_owned(),
                    )]),
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
            let action = CodeAction {
                title: "Report LSP issue/Suggest feature".to_string(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "Report LSP issue/Suggest feature".to_string(),
                    command: "open_url".to_string(),
                    arguments: Some(vec![serde_json::Value::String(
                        "https://github.com/frederik-uni/cargotom/issues".to_owned(),
                    )]),
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
        if let Some(toml) = self
            .toml_store
            .read()
            .await
            .get(&params.text_document.uri.to_string())
        {
            let uri = params.text_document.uri;
            let byte_offset_start =
                get_byte_index_from_position(toml.text(), params.range.start) as u32;
            let byte_offset_end =
                get_byte_index_from_position(toml.text(), params.range.end) as u32;
            let dep = match toml.get_dependency_by_range(byte_offset_start, byte_offset_end) {
                Some(v) => v,
                None => return Some(Ok(Some(actions))),
            };
            if dep.data.name.end == dep.end {
                let start = toml.byte_offset_to_position(dep.data.name.end);
                let action = CodeAction {
                    title: "Make Workspace dependency".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(new_workspace_edit(
                        uri.clone(),
                        vec![TextEdit::new(
                            Range::new(start, start),
                            " = { workspace = true }".to_string(),
                        )],
                    )),
                    ..CodeAction::default()
                };
                actions.push(CodeActionOrCommand::CodeAction(action));
            }

            let start = toml.byte_offset_to_position(dep.data.name.end);
            let end = toml.byte_offset_to_position(dep.end);
            let range = Range::new(start, end);

            if let Source::Version { value, .. } = &dep.data.source {
                let action = match value.key.is_none() {
                    true => CodeAction {
                        title: "Expand dependency specification".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(new_workspace_edit(
                            uri.clone(),
                            vec![TextEdit::new(
                                range,
                                format!(" = {} version = \"{}\" {}", '{', value.value.data, '}'),
                            )],
                        )),
                        ..CodeAction::default()
                    },
                    false => CodeAction {
                        title: "Collapse dependency specification".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(new_workspace_edit(
                            uri.clone(),
                            vec![TextEdit::new(range, format!(" = \"{}\"", value.value.data))],
                        )),
                        ..CodeAction::default()
                    },
                };
                actions.push(CodeActionOrCommand::CodeAction(action));
            }

            let crate_name = &dep.data.name.data;
            let action = CodeAction {
                title: "Open Docs".to_string(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "Open Docs".to_string(),
                    command: "open_url".to_string(),
                    arguments: Some(vec![serde_json::Value::String(format!(
                        "https://docs.rs/{crate_name}/latest/{crate_name}/"
                    ))]),
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
            let action = CodeAction {
                title: "Open crates.io".to_string(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "Open crates.io".to_string(),
                    command: "open_url".to_string(),
                    arguments: Some(vec![serde_json::Value::String(format!(
                        "https://crates.io/crates/{crate_name}/"
                    ))]),
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
            let update = toml.needs_update(&self.crates).await.unwrap_or_default();
            if !update.is_empty() {
                if let Some((range, new)) = update.iter().find(|(name, _)| &name.data == crate_name)
                {
                    let mut start = toml.byte_offset_to_position(range.start);
                    start.character += 1;
                    let mut end = toml.byte_offset_to_position(range.end);
                    end.character -= 1;
                    let edit = TextEdit::new(Range::new(start, end), new.to_string());
                    let action = CodeAction {
                        title: "Upgrade".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(new_workspace_edit(uri.clone(), vec![edit])),
                        ..CodeAction::default()
                    };
                    actions.insert(0, CodeActionOrCommand::CodeAction(action));
                }
                let changes = update
                    .into_iter()
                    .map(|(range, new)| {
                        let mut start = toml.byte_offset_to_position(range.start);
                        start.character += 1;
                        let mut end = toml.byte_offset_to_position(range.end);
                        end.character -= 1;
                        TextEdit::new(Range::new(start, end), new)
                    })
                    .collect();

                let action = CodeAction {
                    title: "Upgrade All".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    edit: Some(new_workspace_edit(uri, changes)),
                    ..CodeAction::default()
                };
                actions.push(CodeActionOrCommand::CodeAction(action));
            }
            let action = CodeAction {
                title: "Update All".to_string(),
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: "Update All".to_string(),
                    command: "cargo-update".to_string(),
                    arguments: None,
                }),
                ..CodeAction::default()
            };
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
        Some(Ok(Some(actions)))
    }
}
