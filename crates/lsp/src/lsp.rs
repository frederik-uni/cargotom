use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use info_provider::InfoProvider;
use parser::config::Config;
use parser::toml::{DepSource, OptionalKey, Positioned};
use parser::{Db, Indent};
use rust_version::RustVersion;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionProviderCapability, CodeActionResponse, Command, CompletionItem, CompletionItemKind,
    CompletionOptions, CompletionParams, CompletionResponse, CompletionTextEdit,
    DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, ExecuteCommandParams, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InlayHint, InlayHintKind, InlayHintParams, MarkupKind, MessageType,
    OneOf, Position, Range, ServerCapabilities, ServerInfo, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit, Url,
    WorkspaceEdit, WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities,
};
use tower_lsp::{
    async_trait,
    lsp_types::{InitializeParams, InitializeResult},
    Client, LanguageServer, LspService, Server,
};

pub struct Context {
    pub client: Client,
    db: Arc<RwLock<Db>>,
    pub info: Arc<InfoProvider>,
}

macro_rules! try_option {
    ($expr:expr) => {
        match $expr {
            Some(val) => val,
            None => return Ok(None),
        }
    };
}

macro_rules! crate_version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}
fn load_config(params: &InitializeParams) -> Config {
    params
        .initialization_options
        .clone()
        .map(serde_json::from_value)
        .and_then(|v| v.ok())
        .unwrap_or_default()
}

#[async_trait]
impl LanguageServer for Context {
    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(
                MessageType::INFO,
                "aquire write lock did_change_workspace_folders",
            )
            .await;
        let mut lock = self.db.write().await;
        self.client
            .log_message(
                MessageType::INFO,
                "aquired write lock did_change_workspace_folders",
            )
            .await;
        for remove in params.event.removed {
            lock.remove_workspace(&remove.uri);
        }
        for add in params.event.added {
            let mut root = add.uri;
            if !root.as_str().ends_with('/') {
                root = Url::parse(&(root.as_str().to_owned() + "/")).unwrap();
            }
            lock.try_init(&root.join("Cargo.toml").unwrap()).await;
        }
    }
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let c = self.db.clone();
        let config = load_config(&params);
        self.client
            .log_message(MessageType::INFO, "aquire write lock init")
            .await;
        let mut lock = self.db.write().await;
        self.client
            .log_message(MessageType::INFO, "aquired write lock init")
            .await;
        lock.config = config;
        self.info.set_per_page(lock.config.per_page).await;
        self.info.set_offline(lock.config.offline).await;

        lock.sel = Some(c);
        for v in params.workspace_folders.unwrap_or_default() {
            let mut root = v.uri;
            if !root.as_str().ends_with('/') {
                root = Url::parse(&(root.as_str().to_owned() + "/")).unwrap();
            }
            lock.try_init(&root.join("Cargo.toml").unwrap()).await;
        }
        if let Some(root) = params.root_uri {
            let mut root = root;
            if !root.as_str().ends_with('/') {
                root = Url::parse(&(root.as_str().to_owned() + "/")).unwrap();
            }
            lock.try_init(&root.join("Cargo.toml").unwrap()).await;
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
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                call_hierarchy_provider: None,
                semantic_tokens_provider: None,
                moniker_provider: None,
                linked_editing_range_provider: None,
                inline_value_provider: None,
                inlay_hint_provider: Some(OneOf::Left(true)),
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
            self.client
                .log_message(MessageType::INFO, "aquire write lock change")
                .await;
            let mut lock = self.db.write().await;
            self.client
                .log_message(MessageType::INFO, "aquired write lock change")
                .await;
            lock.update_lock(uri).await;
            return;
        }
        if !self.shoud_allow_user(&uri) {
            return;
        }
        {
            self.client
                .log_message(MessageType::INFO, "aquire write lock change")
                .await;
            let mut lock = self.db.write().await;
            self.client
                .log_message(MessageType::INFO, "aquired write lock change")
                .await;
            for change in params.content_changes {
                let range = change.range.map(|v| {
                    (
                        (v.start.line as usize, v.start.character as usize),
                        (v.end.line as usize, v.end.character as usize),
                    )
                });
                lock.update(&uri, range, &change.text);
            }
            lock.reload(uri).await;
        }

        let _ = self.client.inlay_hint_refresh().await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        if uri.to_string().ends_with("/Cargo.lock") {
            self.client
                .log_message(MessageType::INFO, "aquire write lock open")
                .await;
            let mut lock = self.db.write().await;
            self.client
                .log_message(MessageType::INFO, "aquired write lock open")
                .await;
            lock.update_lock(uri).await;
            return;
        }
        if !self.shoud_allow_user(&uri) {
            return;
        }
        {
            self.client
                .log_message(MessageType::INFO, "aquire write lock open")
                .await;
            let mut lock = self.db.write().await;
            self.client
                .log_message(MessageType::INFO, "aquired write lock open")
                .await;
            lock.update(&uri, None, &params.text_document.text);
            lock.reload(uri).await;
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
        self.client
            .log_message(MessageType::INFO, "aquire read lock code_action")
            .await;
        let lock = self.db.read().await;
        self.client
            .log_message(MessageType::INFO, "aquired read lock code_action")
            .await;
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

                let open_page = |actions_last: &mut Vec<CodeActionOrCommand>, name, url| {
                    let action = CodeAction {
                        title: format!("Open {name}"),
                        kind: Some(CodeActionKind::EMPTY),
                        command: Some(Command {
                            title: format!("Open {name}"),
                            command: "open_url".to_string(),
                            arguments: Some(vec![serde_json::Value::String(url)]),
                        }),
                        ..CodeAction::default()
                    };
                    actions_last.push(CodeActionOrCommand::CodeAction(action));
                };

                if lock.config.offline {
                    let info = self.info.get_local(name).await;
                    if let Some(info) = info {
                        if let Some(repo) = info.repository {
                            open_page(&mut actions_last, "Repository", repo);
                        }
                        if let Some(documentation) = info.documentation {
                            open_page(&mut actions_last, "Documentation", documentation);
                        }
                        if let Some(homepage) = info.homepage {
                            open_page(&mut actions_last, "Homepage", homepage);
                        }
                    }
                } else {
                    open_page(
                        &mut actions_last,
                        "Documentation",
                        format!("https://docs.rs/{name}/{version}/"),
                    );
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
                }
                open_page(
                    &mut actions_last,
                    "crates.io",
                    format!("https://crates.io/crates/{name}"),
                );
            } else if let DepSource::Workspace(range) = &dep.data.source {
                let workspace_uri = lock.get_workspace(&uri);
                let workspace = workspace_uri.as_ref().and_then(|v| lock.get_toml(v));
                if let (Some(workspace_uri), Some(workspace)) = (workspace_uri, &workspace) {
                    if workspace
                        .dependencies
                        .iter()
                        .find(|v| v.data.name.data == dep.data.name.data)
                        .is_none()
                    {
                        if let Ok(Some(info)) = self
                            .info
                            .get_info(None, &dep.data.name.data)
                            .await
                            .map(|v| {
                                v.into_iter().rfind(|v| match lock.config.stable_version {
                                    false => true,
                                    true => v.ver().map(|v| v.is_pre_release()) == Some(false),
                                })
                            })
                        {
                            if let Some(last) = workspace.dependencies.last() {
                                let line = lock.get_line(&workspace_uri, last.end as usize);
                                if let Some(line) = line {
                                    actions.insert(
                                        0,
                                        CodeActionOrCommand::CodeAction(CodeAction {
                                            title: "Add to workspace".to_owned(),
                                            edit: Some(WorkspaceEdit {
                                                changes: Some(
                                                    vec![(
                                                        workspace_uri.to_owned(),
                                                        vec![TextEdit {
                                                            range: Range::new(
                                                                Position {
                                                                    line: line as u32 + 1,
                                                                    character: 0,
                                                                },
                                                                Position {
                                                                    line: line as u32 + 1,
                                                                    character: 0,
                                                                },
                                                            ),
                                                            new_text: format!(
                                                                "{} = \"{}\"",
                                                                dep.data.name.data, info.vers
                                                            ),
                                                        }],
                                                    )]
                                                    .into_iter()
                                                    .collect::<HashMap<_, _>>(),
                                                ),
                                                ..Default::default()
                                            }),
                                            ..Default::default()
                                        }),
                                    );
                                }
                            }
                        }
                    }
                }
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
        self.client
            .log_message(MessageType::INFO, "aquire read lock formatting")
            .await;
        let temp = self.db.read().await;
        self.client
            .log_message(MessageType::INFO, "aquired read lock formatting")
            .await;
        let data = temp
            .format(
                &params.text_document.uri,
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

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        if !self.shoud_allow_user(&params.text_document.uri) {
            return Ok(None);
        }
        self.client
            .log_message(MessageType::INFO, "aquire read lock inlay_hint")
            .await;
        let v = self
            .db
            .read()
            .await
            .hints(&params.text_document.uri)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|v| InlayHint {
                position: Position::new(v.0 .0 as u32, v.0 .1 as u32),
                label: tower_lsp::lsp_types::InlayHintLabel::String(v.1.label()),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: None,
                padding_left: Some(true),
                padding_right: Some(true),
                data: None,
            })
            .collect();
        self.client
            .log_message(MessageType::INFO, "aquired read lock inlay hint")
            .await;
        Ok(Some(v))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        if !self.shoud_allow_user(&uri) {
            return Ok(None);
        }
        self.client
            .log_message(MessageType::INFO, "aquire read lock hover")
            .await;
        let lock = self.db.read().await;
        self.client
            .log_message(MessageType::INFO, "aquired read lock hover")
            .await;

        if let Some(h) = self
            .hover_dep(&uri, params.text_document_position_params.position, &lock)
            .await
        {
            return Ok(Some(h));
        }

        let path = lock
            .get_path(
                &uri,
                params.text_document_position_params.position.line,
                params.text_document_position_params.position.character,
            )
            .await;
        if let (Some(last), Some(path)) = (path.as_ref().and_then(|v| v.last()), &path) {
            let p = path
                .iter()
                .map(|v| v.tyoe.to_string())
                .collect::<Vec<String>>();
            let detail = try_option!(lock.static_data.get_detail(&p, 0, last.is_value()));
            let start = try_option!(lock.get_offset(&uri, last.range.start as usize));
            let end = try_option!(lock.get_offset(&uri, last.range.end as usize));

            return Ok(Some(Hover {
                contents: HoverContents::Markup(tower_lsp::lsp_types::MarkupContent {
                    kind: MarkupKind::PlainText,
                    value: detail,
                }),
                range: Some(Range::new(
                    Position {
                        line: start.0 as u32,
                        character: start.1 as u32,
                    },
                    Position {
                        line: end.0 as u32,
                        character: end.1 as u32,
                    },
                )),
            }));
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        if !self.shoud_allow_user(&uri) {
            return Ok(None);
        }

        let lock = self.db.read().await;
        let toml = match lock.get_toml(&uri) {
            Some(v) => v,
            None => return Ok(None),
        };
        let workspace = lock.get_workspace(&uri).and_then(|uri| lock.get_toml(uri));
        let pos = params.text_document_position.position;
        let pos = match lock.get_byte(&uri, pos.line as usize, pos.character as usize) {
            Some(v) => v,
            None => return Ok(None),
        };
        if let Some(dep) = toml.dependencies.iter().find(|v| v.contains(pos)) {
            if dep.data.name.contains(pos) {
                let end = pos.saturating_sub(dep.data.name.start as usize);
                let slice = dep.data.name.data.get(..end).unwrap_or(&dep.data.name.data);
                let info = self.info.search(slice).await.unwrap_or_default();
                let start = try_option!(lock.get_offset(&uri, dep.start as usize));
                let end = try_option!(lock.get_offset(&uri, dep.end as usize));

                let out = Ok(Some(CompletionResponse::Array(
                    info.into_iter()
                        .enumerate()
                        .map(|(i, v)| CompletionItem {
                            label: format!("{i}. {}", v.name.clone()),
                            // kind: Some(CompletionItemKind::MODULE),
                            detail: v.description,
                            // preselect: Some(v.exact_match),
                            sort_text: Some(format!("{:06}", i)),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                                range: Range {
                                    start: Position {
                                        line: start.0 as u32,
                                        character: start.1 as u32,
                                    },
                                    end: Position {
                                        line: end.0 as u32,
                                        character: end.1 as u32,
                                    },
                                },
                                new_text: {
                                    let mut dep = dep.data.clone();
                                    dep.name.data = v.name;
                                    if let DepSource::None = dep.source {
                                        match workspace.is_some() {
                                            true => {
                                                dep.source =
                                                    DepSource::Workspace(Default::default())
                                            }
                                            false => {
                                                let ver = match lock.config.stable_version {
                                                    true => v.max_stable_version.or(v.max_version),
                                                    false => v.max_version.or(v.max_stable_version),
                                                }
                                                .unwrap_or_default();
                                                dep.source = DepSource::Version {
                                                    value: OptionalKey::no_key(Positioned::new(
                                                        0, 0, ver,
                                                    )),
                                                    registry: None,
                                                }
                                            }
                                        }
                                    }
                                    dep.to_string()
                                },
                            })),
                            ..Default::default()
                        })
                        .collect(),
                )));
                self.client
                    .log_message(MessageType::INFO, format!("{:#?}", out))
                    .await;
                return out;
            }
            if let DepSource::Version { value, registry } = &dep.data.source {
                if value.value.contains(pos) {
                    let start = try_option!(lock.get_offset(&uri, dep.start as usize));
                    let end = try_option!(lock.get_offset(&uri, dep.end as usize));
                    let end_ = pos.saturating_sub(value.value.start as usize + 1);
                    let slice = value.value.data.get(..end_).unwrap_or(&value.value.data);
                    let info = self
                        .info
                        .get_info(
                            registry.as_ref().map(|v| v.value.data.as_str()),
                            &dep.data.name.data,
                        )
                        .await
                        .unwrap_or_default();
                    return Ok(Some(CompletionResponse::Array(
                        info.into_iter()
                            .rev()
                            .filter(|v| v.vers.starts_with(slice))
                            .enumerate()
                            .map(|(i, v)| CompletionItem {
                                label: v.vers.clone(),
                                kind: Some(CompletionItemKind::MODULE),
                                detail: None,
                                sort_text: Some(format!("{:06}", i)),
                                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
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
                                    new_text: {
                                        let mut dep = dep.data.clone();
                                        dep.source.set_version(OptionalKey::no_key(
                                            Positioned::new(0, 0, v.vers),
                                        ));
                                        dep.to_string()
                                    },
                                })),
                                ..Default::default()
                            })
                            .collect(),
                    )));
                }
            }

            if let Some(feat) = dep.data.features.data.iter().find(|v| v.contains(pos)) {
                let end = pos.saturating_sub(feat.start as usize + 1);
                let slice = feat.data.get(..end).unwrap_or(&feat.data);
                let src = if let DepSource::Workspace(_) = &dep.data.source {
                    workspace.and_then(|v| {
                        v.dependencies
                            .iter()
                            .find(|v| v.data.name.data == dep.data.name.data)
                            .map(|v| &v.data.source)
                    })
                } else {
                    Some(&dep.data.source)
                };
                if let Some(DepSource::Version { value, registry }) = &src {
                    let version = RustVersion::try_from(value.value.data.as_str()).ok();
                    let features = self
                        .info
                        .get_info(
                            registry.as_ref().map(|v| v.value.data.as_str()),
                            &dep.data.name.data,
                        )
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .rfind(|v| v.ver() == version)
                        .map(|v| v.feature_all())
                        .unwrap_or_default();

                    let start = try_option!(lock.get_offset(&uri, feat.start as usize));
                    let end = try_option!(lock.get_offset(&uri, feat.end as usize));
                    return Ok(Some(CompletionResponse::Array(
                        features
                            .into_iter()
                            .filter(|v| v.starts_with(slice))
                            .map(|v| CompletionItem {
                                label: v.clone(),
                                kind: Some(CompletionItemKind::MODULE),
                                detail: None,
                                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
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
                                    new_text: format!("\"{v}\""),
                                })),
                                ..Default::default()
                            })
                            .collect(),
                    )));
                }
            }
        }
        if let Some(feat) = toml.features.iter().find(|v| v.contains(pos)) {
            //TODO: features
        }

        Ok(None)
    }
}

pub async fn main(path: PathBuf) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let info = Arc::new(InfoProvider::new(50, false, path).await);
    let (client, server) = LspService::build(|client| Context {
        client: client.clone(),
        db: Db::new(client, info.clone()),
        info,
    })
    .finish();

    Server::new(stdin, stdout, server).serve(client).await;
}
