use std::path::PathBuf;
use std::sync::Arc;

use info_provider::api::InfoProvider;
use parser::{Db, Indent};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, Command, CompletionOptions,
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    ExecuteCommandParams, MessageType, OneOf, Position, Range, ServerCapabilities, ServerInfo,
    SignatureHelpOptions, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextEdit, Url,
};
use tower_lsp::{
    async_trait,
    lsp_types::{InitializeParams, InitializeResult},
    Client, LanguageServer, LspService, Server,
};

pub struct Context {
    pub client: Client,
    db: Arc<RwLock<Db>>,
    info: Arc<InfoProvider>,
    sort: bool,
}

macro_rules! crate_version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}

#[async_trait]
impl LanguageServer for Context {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut lock = self.db.write().await;
        for v in params.workspace_folders.unwrap_or_default() {
            let mut root = v.uri;
            if !root.as_str().ends_with('/') {
                root = Url::parse(&(root.as_str().to_owned() + "/")).unwrap();
            }
            lock.add_file(&root.join("Cargo.toml").unwrap());
        }
        if let Some(root) = params.root_uri {
            let mut root = root;
            if !root.as_str().ends_with('/') {
                root = Url::parse(&(root.as_str().to_owned() + "/")).unwrap();
            }
            lock.add_file(&root.join("Cargo.toml").unwrap());
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "\"".to_string(),
                        ".".to_string(),
                        ":".to_string(),
                        "-".to_string(),
                    ]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["\"".to_string()]),
                    ..Default::default()
                }),
                position_encoding: None,
                selection_range_provider: None,
                hover_provider: None,
                definition_provider: None,
                type_definition_provider: None,
                implementation_provider: None,
                references_provider: None,
                document_highlight_provider: None,
                document_symbol_provider: None,
                workspace_symbol_provider: None,
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                code_lens_provider: None,
                document_formatting_provider: Some(OneOf::Left(true)),
                document_range_formatting_provider: None,
                document_on_type_formatting_provider: None,
                rename_provider: None,
                document_link_provider: None,
                color_provider: None,
                folding_range_provider: None,
                declaration_provider: None,
                execute_command_provider: Some(tower_lsp::lsp_types::ExecuteCommandOptions {
                    commands: vec![
                        "open_url".to_string(),
                        "cargo-update".to_string(),
                        "open-src".to_string(),
                    ],
                    ..Default::default()
                }),
                workspace: None,
                call_hierarchy_provider: None,
                semantic_tokens_provider: None,
                moniker_provider: None,
                linked_editing_range_provider: None,
                inline_value_provider: None,
                inlay_hint_provider: None,
                diagnostic_provider: None,
                experimental: None,
            },
            server_info: Some(ServerInfo {
                name: "CargoTom".to_string(),
                version: Some(crate_version!().to_owned()),
            }),
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if uri.to_string().ends_with("/Cargo.lock") {
            let mut lock = self.db.write().await;
            lock.update_lock(uri);
            return;
        }
        if !self.shoud_allow_user(&uri) {
            return;
        }
        {
            let mut lock = self.db.write().await;
            for change in params.content_changes {
                let range = change.range.map(|v| {
                    (
                        (v.start.line as usize, v.start.character as usize),
                        (v.end.line as usize, v.end.character as usize),
                    )
                });
                lock.update(&uri, range, &change.text);
            }
            lock.reload(uri);
        }

        let _ = self.client.inlay_hint_refresh().await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        if uri.to_string().ends_with("/Cargo.lock") {
            let mut lock = self.db.write().await;
            lock.update_lock(uri);
            return;
        }
        if !self.shoud_allow_user(&uri) {
            return;
        }
        {
            let mut lock = self.db.write().await;
            lock.update(&uri, None, &params.text_document.text);
            lock.reload(uri);
        }
        let _ = self.client.inlay_hint_refresh().await;
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if !self.shoud_allow_user(&params.text_document.uri) {
            return Ok(None);
        }
        let mut actions = vec![];

        let mut actions_last = vec![CodeActionOrCommand::CodeAction(CodeAction {
            title: "Update All".to_string(),
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: "Update All".to_string(),
                command: "cargo-update".to_string(),
                arguments: None,
            }),
            ..CodeAction::default()
        })];

        let uri = params.text_document.uri;

        if params.range.start.line == 0 || params.range.end.line == 0 {
            actions.extend(self.first_line_actions().await);
        }
        let lock = self.db.read().await;
        if let Some(dep) = lock.get_dependency(
            &uri,
            (
                params.range.start.line as usize,
                params.range.start.character as usize,
            ),
            (
                params.range.end.line as usize,
                params.range.end.character as usize,
            ),
        ) {
            if let parser::toml::DepSource::Version { value, registry } = &dep.data.source {
                let version_info = self
                    .info
                    .get_info(
                        registry.as_ref().map(|v| v.value.data.as_str()),
                        &dep.data.name.data,
                    )
                    .await;
                match version_info {
                    Ok(data) => {
                        if let Some(last) = data.last() {
                            if let Some(upgrade_dep) =
                                self.upgrade_dep(&uri, &value.value, last.ver(), &lock)
                            {
                                actions.push(CodeActionOrCommand::CodeAction(upgrade_dep));
                            }
                        }
                    }
                    _ => {}
                }
                let version = &value.value.data;

                let name = &dep.data.name.data;
                let action = CodeAction {
                    title: "Open Docs".to_string(),
                    kind: Some(CodeActionKind::EMPTY),
                    command: Some(Command {
                        title: "Open Docs".to_string(),
                        command: "open_url".to_string(),
                        arguments: Some(vec![serde_json::Value::String(format!(
                            "https://docs.rs/{name}/{version}/"
                        ))]),
                    }),
                    ..CodeAction::default()
                };
                actions_last.push(CodeActionOrCommand::CodeAction(action));
                let action = CodeAction {
                    title: "Open Source".to_string(),
                    kind: Some(CodeActionKind::EMPTY),
                    command: Some(Command {
                        title: "Open Source".to_string(),
                        command: "open-src".to_string(),
                        arguments: Some(vec![
                            serde_json::Value::String(name.clone()),
                            serde_json::Value::String(version.clone()),
                        ]),
                    }),
                    ..CodeAction::default()
                };
                actions_last.push(CodeActionOrCommand::CodeAction(action));
                let action = CodeAction {
                    title: "Open crates.io".to_string(),
                    kind: Some(CodeActionKind::EMPTY),
                    command: Some(Command {
                        title: "Open crates.io".to_string(),
                        command: "open_url".to_string(),
                        arguments: Some(vec![serde_json::Value::String(format!(
                            "https://crates.io/crates/{name}"
                        ))]),
                    }),
                    ..CodeAction::default()
                };
                actions_last.push(CodeActionOrCommand::CodeAction(action));
            }
            if let Some(a) = self.dep_actions(&uri, dep, &lock) {
                actions.extend(a.into_iter().map(CodeActionOrCommand::CodeAction));
            }
        }

        actions.extend(actions_last);
        Ok(Some(actions))
    }

    async fn execute_command(
        &self,
        mut params: ExecuteCommandParams,
    ) -> tower_lsp::jsonrpc::Result<Option<serde_json::Value>> {
        if params.command == "cargo-update" {
            let _ = std::process::Command::new("cargo").arg("update").spawn();
            return Ok(None);
        }

        if params.command == "open-src" {
            let name = params.arguments.get(0).and_then(|arg| arg.as_str());
            let version = params.arguments.get(1).and_then(|arg| arg.as_str());
            let mut src = if let Some(name) = name {
                self.info.get_crate_repository(name).await
            } else {
                None
            };
            if src.is_none() {
                match (name, version) {
                    (Some(name), Some(version)) => {
                        src = Some(format!(
                            "https://docs.rs/crate/{}/{}/source/",
                            name, version
                        ));
                    }
                    _ => {}
                }
            }
            if let Some(src) = src {
                params.command = "open_url".to_string();
                params.arguments = vec![serde_json::Value::String(src)]
            }
        }

        if params.command == "open_url" {
            let mut args = params.arguments.iter();
            if let Some(url) = args.next().and_then(|arg| arg.as_str()) {
                if let Err(e) = webbrowser::open(url) {
                    self.client
                        .show_message(
                            MessageType::WARNING,
                            format!("failed to open browser {}", e.to_string()),
                        )
                        .await;
                    return Err(tower_lsp::jsonrpc::Error::invalid_params(
                        "failed to open browser",
                    ));
                }
            } else {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "URL argument missing",
                ));
            }
        } else {
            return Err(tower_lsp::jsonrpc::Error::method_not_found());
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let temp = self.db.read().await;
        let data = temp
            .format(
                &params.text_document.uri,
                self.sort,
                params.options.insert_final_newline.unwrap_or(true),
                match params.options.insert_spaces {
                    true => Indent::Spaces(params.options.tab_size),
                    false => Indent::Tab,
                },
            )
            .unwrap_or_default()
            .into_iter()
            .map(|(range, text)| TextEdit {
                range: Range::new(
                    Position::new(range.0 .0 as u32, range.0 .1 as u32),
                    Position::new(range.1 .0 as u32, range.1 .1 as u32),
                ),
                new_text: text,
            })
            .collect();
        Ok(Some(data))
    }
}

pub async fn main(path: PathBuf) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (client, server) = LspService::build(|client| Context {
        client,
        db: Default::default(),
        info: Arc::new(InfoProvider::new(50)),
        sort: false,
    })
    .finish();

    Server::new(stdin, stdout, server).serve(client).await;
}
