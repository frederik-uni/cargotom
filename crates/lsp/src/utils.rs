use tower_lsp::lsp_types::{CodeAction, CodeActionKind, CodeActionOrCommand, Command, Url};

use crate::lsp::Context;

impl Context {
    pub fn shoud_allow_user(&self, uri: &Url) -> bool {
        let uri = uri.to_string();
        uri.ends_with("/Cargo.toml")
    }

    pub async fn first_line_actions(&self) -> Vec<CodeActionOrCommand> {
        let mut actions = vec![];
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

        let action = CodeAction {
            title: "Open Cargo manifest docs".to_string(),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: "Open Cargo manifest docs".to_string(),
                command: "open_url".to_string(),
                arguments: Some(vec![serde_json::Value::String(
                    "https://doc.rust-lang.org/cargo/reference/manifest.html".to_owned(),
                )]),
            }),
            ..CodeAction::default()
        };
        actions.push(CodeActionOrCommand::CodeAction(action));
        actions
    }
}
