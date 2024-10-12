use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::crate_lookup::{CratesIoStorage, CratesIoStorageReader, Shared};
use crate::generate_tree::{
    get_after_key, parse_toml, Key, KeyOrValue, KeyOrValueOwned, RangeExclusive, Tree, TreeValue,
    Value,
};
use crate::helper::{crate_version, get_byte_index_from_position, new_workspace_edit, shared};
use crate::rust_version::RustVersion;

struct Backend {
    crates: Shared<Deamon>,
    client: Client,
    workspace_root: Shared<Option<PathBuf>>,
    path: PathBuf,
    toml_store: Arc<Mutex<HashMap<String, Store>>>,
}

enum Deamon {
    Deamon((CratesIoStorageReader, Config, PathBuf)),
    NoDeamon(CratesIoStorage),
    Starting,
}

fn start_daemon(port: u16, storage: &Path, stable: bool, offline: bool, per_page_web: u32) {
    let mut args = vec![
        "--daemon".to_string(),
        port.to_string(),
        "--storage".to_string(),
        storage.display().to_string(),
        "--per-page-web".to_string(),
        per_page_web.to_string(),
    ];
    if stable {
        args.push("--stable".to_string());
    }
    if offline {
        args.push("--offline".to_string());
    }

    // let current_exe = std::env::current_exe().expect("Failed to get current executable path");
    let current_exe = "/Users/frederik/.cargo/target/debug/cargotom";

    // Spawn a new process with the current executable and the arguments
    let v = std::process::Command::new(current_exe).args(&args).spawn();
}

impl Deamon {
    async fn handle_error(&self, e: tcp_struct::Error) {
        match e {
            tcp_struct::Error::StreamError(error) => match error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    if let Deamon::Deamon((_, config, path)) = &self {
                        start_daemon(
                            config.daemon_port,
                            &path,
                            config.stable,
                            config.offline,
                            config.per_page_web,
                        );
                    }
                }
                _ => {}
            },
            tcp_struct::Error::ApiMisMatch(_) => match self {
                Deamon::Deamon((daemon, config, path)) => {
                    let _ = daemon.stop().await;
                    start_daemon(
                        config.daemon_port,
                        &path,
                        config.stable,
                        config.offline,
                        config.per_page_web,
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }
    async fn search(&self, query: &str) -> Vec<(String, Option<String>, String)> {
        match self {
            Deamon::Deamon((v, _, _)) => match v.search(query).await {
                Ok(v) => v,
                Err(e) => {
                    self.handle_error(e).await;
                    Default::default()
                }
            },
            Deamon::NoDeamon(arc) => arc.search(query).await,
            Deamon::Starting => Default::default(),
        }
    }
    async fn get_version_local(&self, name: &str) -> Option<Vec<RustVersion>> {
        match self {
            Deamon::Deamon((daemon, _, _)) => match daemon.get_version_local(name).await {
                Ok(v) => v,
                Err(e) => {
                    self.handle_error(e).await;
                    Default::default()
                }
            },
            Deamon::NoDeamon(storage) => storage.get_version_local(name).await,
            Deamon::Starting => Default::default(),
        }
    }
    async fn get_versions(&self, name: &str, version_filter: &str) -> Option<Vec<RustVersion>> {
        match self {
            Deamon::Deamon((daemon, _, _)) => match daemon.get_versions(name, version_filter).await
            {
                Ok(v) => v,
                Err(e) => {
                    self.handle_error(e).await;
                    Default::default()
                }
            },
            Deamon::NoDeamon(storage) => storage.get_versions(name, version_filter).await,
            Deamon::Starting => Default::default(),
        }
    }

    async fn get_features_local(&self, name: &str, version: &str) -> Option<Vec<String>> {
        match self {
            Deamon::Deamon((daemon, _, _)) => {
                match daemon.get_features_local(name, version).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.handle_error(e).await;
                        Default::default()
                    }
                }
            }
            Deamon::NoDeamon(storage) => storage.get_features_local(name, version).await,
            Deamon::Starting => Default::default(),
        }
    }

    async fn get_features(&self, name: &str, version: &str, search: &str) -> Option<Vec<String>> {
        match self {
            Deamon::Deamon((daemon, _, _)) => {
                match daemon.get_features(name, version, search).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.handle_error(e).await;
                        Default::default()
                    }
                }
            }
            Deamon::NoDeamon(storage) => storage.get_features(name, version, search).await,
            Deamon::Starting => Default::default(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Config {
    #[serde(default = "true_default")]
    offline: bool,
    #[serde(default = "true_default")]
    stable: bool,
    #[serde(default = "per_page_web_default")]
    per_page_web: u32,
    #[serde(default = "true_default")]
    daemon: bool,
    #[serde(default = "daemon_port_default")]
    daemon_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            offline: true,
            stable: true,
            per_page_web: 25,
            daemon: true,
            daemon_port: 54218,
        }
    }
}

fn true_default() -> bool {
    true
}
fn per_page_web_default() -> u32 {
    25
}
fn daemon_port_default() -> u16 {
    8080
}

#[derive(Debug)]
struct Store {
    workspace_root: Shared<Option<PathBuf>>,
    workspace_members: Vec<PathBuf>,
    content: String,
    tree: Tree,
    crates_info: Vec<TreeValue>,
    features: HashMap<String, Vec<String>>,
}

impl Store {
    pub async fn new(s: String, workspace_root: Shared<Option<PathBuf>>) -> Self {
        let mut s = Self {
            tree: parse_toml(&s),
            content: s,
            crates_info: vec![],
            workspace_root,
            workspace_members: vec![],
            features: Default::default(),
        };
        s.crates().await;
        s
    }

    pub fn inner_string_range(&self, range: &RangeExclusive) -> Range {
        let mut start = self.byte_offset_to_position(range.start);
        start.character += 1;
        let mut end = self.byte_offset_to_position(range.end);
        end.character -= 1;
        Range::new(start, end)
    }

    pub fn byte_offset_to_position(&self, byte_offset: u32) -> Position {
        let byte_offset = byte_offset as usize;

        let content_slice = &self.content[..byte_offset];

        let line = content_slice.chars().filter(|&c| c == '\n').count() as u32;

        let line_start = content_slice.rfind('\n').map_or(0, |pos| pos + 1);
        let character = content_slice[line_start..].chars().count() as u32;
        Position::new(line, character)
    }

    pub async fn unknown_features(
        &self,
        crates: &Shared<Deamon>,
    ) -> Vec<(String, RangeExclusive, String)> {
        let mut res = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, _)) = cr.get_version() {
                let features = crates
                    .read()
                    .await
                    .get_features_local(crate_name, &version)
                    .await;
                let existing_features = cr.get_features();
                if let Some(features) = features {
                    let mut existing_features = existing_features
                        .into_iter()
                        .filter(|(name, _)| !features.contains(name))
                        .map(|(feature, range)| (crate_name.clone(), range, feature))
                        .collect::<Vec<_>>();
                    res.append(&mut existing_features);
                }
            }
        }
        res
    }

    pub async fn needs_update(
        &self,
        crates: &Shared<Deamon>,
    ) -> Option<Vec<(String, RangeExclusive, String)>> {
        let mut updates = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, range)) = cr.get_version() {
                let crate_version = RustVersion::try_from(version.as_str()).ok()?;
                let mut versions = crates
                    .read()
                    .await
                    .get_version_local(crate_name)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|v| v > &crate_version)
                    .collect::<Vec<_>>();
                versions.sort();

                if let Some(v) = versions.pop() {
                    updates.push((crate_name.clone(), range, v.to_string()));
                }
            }
        }
        Some(updates)
    }

    pub async fn invalid_versions(
        &self,
        crates: &Shared<Deamon>,
    ) -> Option<Vec<(String, RangeExclusive)>> {
        let mut updates = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, range)) = cr.get_version() {
                let crate_version = RustVersion::try_from(version.as_str()).ok()?;
                if let Some(v) = crates.read().await.get_version_local(crate_name).await {
                    if v.iter().find(|v| (*v) == &crate_version).is_none() {
                        updates.push((crate_name.clone(), range));
                    }
                }
            }
        }
        Some(updates)
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    async fn crates(&mut self) {
        let mut v = self.tree.find("dependencies");
        v.append(&mut self.tree.find("dev-dependencies"));
        let v = v
            .into_iter()
            .map(|v| match &v.value {
                Value::Tree { value, .. } => Ok(&value.0),
                Value::NoContent => Err("unexpected type: none"),
                Value::Array(_) => Err("unexpected type: array"),
                Value::String { .. } => Err("unexpected type: string"),
                Value::Bool { .. } => Err("unexpected type: bool"),
            })
            .flat_map(|v| v.ok())
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        self.crates_info = v;
        let v = self
            .tree
            .find("features")
            .iter()
            .filter_map(|v| v.value.as_tree())
            .flat_map(|v| &v.0)
            .flat_map(|v| {
                v.value.as_array().map(|a| {
                    (
                        v.key.value.clone(),
                        a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>(),
                    )
                })
            })
            .collect::<HashMap<_, _>>();
        self.features = v;

        let root = &*self.workspace_root.read().await;
        if let Some(root) = root {
            let ws: Vec<_> = self
                .tree
                .find("workspace")
                .iter()
                .filter_map(|v| v.value.as_tree())
                .flat_map(|v| &v.0)
                .filter(|v| v.key.value.as_str() == "members")
                .filter_map(|v| v.value.as_array())
                .flat_map(|v| v.into_iter().filter_map(|v| v.as_str()))
                .map(|v| root.join(v))
                .collect();
            self.workspace_members = ws;
        }
    }

    fn get_members(&self) -> Vec<Url> {
        self.workspace_members
            .iter()
            .filter_map(|path| Url::from_file_path(path).ok())
            .collect()
    }

    fn find_crate_by_byte_offset_range(
        &self,
        byte_offset_start: u32,
        byte_offset_end: u32,
    ) -> Option<&TreeValue> {
        self.crates_info
            .iter()
            .find(|v| v.is_in_range(byte_offset_start, byte_offset_end))
    }

    pub async fn update(&mut self, params: DidChangeTextDocumentParams) {
        for change in params.content_changes {
            if let Some(range) = change.range {
                let start = get_byte_index_from_position(&self.content, range.start);
                let end = get_byte_index_from_position(&self.content, range.end);

                self.content.replace_range(start..end, &change.text);
            } else {
                self.content.clone_from(&change.text)
            }
        }
        //TODO: dont parse whole toml every time
        self.tree = parse_toml(&self.content);
        self.crates().await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let config: Config = params
            .initialization_options
            .map(serde_json::from_value)
            .and_then(|v| v.ok())
            .unwrap_or_default();
        match config.daemon {
            true => {
                let daemon = CratesIoStorage::read(config.daemon_port, crate_version());
                match daemon.update(config).await {
                    Ok(_) => {}
                    Err(err) => match err {
                        tcp_struct::Error::StreamError(error) => match error.kind() {
                            std::io::ErrorKind::ConnectionRefused => start_daemon(
                                config.daemon_port,
                                &self.path,
                                config.stable,
                                config.offline,
                                config.per_page_web,
                            ),
                            _ => {}
                        },
                        tcp_struct::Error::ApiMisMatch(_) => {
                            let _ = daemon.stop().await;
                            start_daemon(
                                config.daemon_port,
                                &self.path,
                                config.stable,
                                config.offline,
                                config.per_page_web,
                            );
                        }
                        _ => {}
                    },
                }
                *self.crates.write().await = Deamon::Deamon((daemon, config, self.path.clone()));
            }
            false => {
                *self.crates.write().await = Deamon::NoDeamon(CratesIoStorage::new(
                    &self.path,
                    config.stable,
                    config.offline,
                    config.per_page_web,
                ));
            }
        }

        if let Some(root_uri) = params.root_uri {
            let path = root_uri.to_file_path().unwrap();
            let file = path.join("Cargo.toml");

            *self.workspace_root.write().await = Some(path);
            if let (Ok(text), Ok(uri)) = (read_to_string(&file), Url::from_file_path(file)) {
                let store = Store::new(text, self.workspace_root.clone()).await;
                let members = store.get_members();
                let _ = self.toml_store.lock().await.insert(uri.to_string(), store);
                self.make_sure_open(members).await;
            }
        }
        let capabilities = ServerCapabilities {
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
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
            execute_command_provider: Some(tower_lsp::lsp_types::ExecuteCommandOptions {
                commands: vec!["open_url".to_string()],
                ..Default::default()
            }),
            ..Default::default()
        };
        Ok(InitializeResult {
            capabilities,
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "LSP Initialized")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        if let Some(store) = self.toml_store.lock().await.get(
            &params
                .text_document_position_params
                .text_document
                .uri
                .to_string(),
        ) {
            let byte_offset = get_byte_index_from_position(
                store.text(),
                params.text_document_position_params.position,
            ) as u32;
            if let Some(item) = store.tree.get_item_by_pos(byte_offset) {
                if let Some(path) = get_after_key("dependencies", &item)
                    .or(get_after_key("dev-dependencies", &item))
                {
                    let path = path.iter().map(|v| v.owned()).collect::<Vec<_>>();

                    let over_features = path.iter().find(|v| match v {
                        KeyOrValueOwned::Key(key) => key.value == "features",
                        KeyOrValueOwned::Value(_) => false,
                    });

                    if let Some(KeyOrValueOwned::Key(key)) = over_features {
                        let range = match path.last().unwrap() {
                            KeyOrValueOwned::Key(key) => key.range.clone(),
                            KeyOrValueOwned::Value(value) => {
                                value.range().cloned().unwrap_or(key.range.clone())
                            }
                        };
                        let crate_name = &path.first().unwrap().as_key().unwrap().value;

                        if let Some(v) = store
                            .crates_info
                            .iter()
                            .find(|v| &v.key.value == crate_name)
                        {
                            if let Some((version, _)) = v.get_version() {
                                let features = self
                                    .crates
                                    .read()
                                    .await
                                    .get_features(crate_name, &version, "")
                                    .await
                                    .unwrap_or_default();
                                return Ok(Some(Hover {
                                    contents: HoverContents::Markup(MarkupContent {
                                        kind: MarkupKind::Markdown,
                                        value: features
                                            .iter()
                                            .map(|v| format!("- {}", v))
                                            .collect::<Vec<_>>()
                                            .join("\n"),
                                    }),
                                    range: Some(Range::new(
                                        store.byte_offset_to_position(range.start),
                                        store.byte_offset_to_position(range.end),
                                    )),
                                }));
                            }
                        } else {
                            return Ok(Some(Hover {
                                contents: HoverContents::Markup(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value: format!("{:?}", store.crates_info),
                                }),
                                range: Some(Range::new(
                                    store.byte_offset_to_position(key.range.start),
                                    store.byte_offset_to_position(key.range.end),
                                )),
                            }));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri_ = params.text_document.uri;
        let uri = uri_.to_string();
        if !uri.ends_with("/Cargo.toml") {
            return Ok(None);
        }
        let mut actions = vec![];
        if let Some(store) = self.toml_store.lock().await.get_mut(&uri) {
            let byte_offset_start =
                get_byte_index_from_position(store.text(), params.range.start) as u32;
            let byte_offset_end =
                get_byte_index_from_position(store.text(), params.range.end) as u32;

            if let Some(v) =
                store.find_crate_by_byte_offset_range(byte_offset_start, byte_offset_end)
            {
                let crate_name = &v.key.value;
                if matches!(v.value, Value::NoContent) {
                    let start = store.byte_offset_to_position(v.key.range.end);
                    let action = CodeAction {
                        title: "Make Workspace dependency".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(new_workspace_edit(
                            uri_.clone(),
                            vec![TextEdit::new(
                                Range::new(start, start),
                                " = { workspace = true }".to_string(),
                            )],
                        )),
                        ..CodeAction::default()
                    };
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
                if let (Some(range), Some((version, _))) = (v.value.range(), v.get_version()) {
                    let start = store.byte_offset_to_position(range.start);
                    let end = store.byte_offset_to_position(range.end);
                    let range = Range::new(start, end);
                    let action = match v.value.is_str() {
                        true => CodeAction {
                            title: "Expand dependency specification".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(new_workspace_edit(
                                uri_.clone(),
                                vec![TextEdit::new(
                                    range,
                                    format!("{} version = \"{}\" {}", '{', version, '}'),
                                )],
                            )),
                            ..CodeAction::default()
                        },
                        false => CodeAction {
                            title: "Collapse dependency specification".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(new_workspace_edit(
                                uri_.clone(),
                                vec![TextEdit::new(range, format!("\"{}\"", version))],
                            )),
                            ..CodeAction::default()
                        },
                    };
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }

                let update = store.needs_update(&self.crates).await.unwrap_or_default();
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

                if !update.is_empty() {
                    if let Some((_, range, new)) =
                        update.iter().find(|(name, _, _)| name == crate_name)
                    {
                        let mut start = store.byte_offset_to_position(range.start);
                        start.character += 1;
                        let mut end = store.byte_offset_to_position(range.end);
                        end.character -= 1;
                        let edit = TextEdit::new(Range::new(start, end), new.to_string());
                        let action = CodeAction {
                            title: "Upgrade".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            edit: Some(new_workspace_edit(uri_.clone(), vec![edit])),
                            ..CodeAction::default()
                        };
                        actions.insert(0, CodeActionOrCommand::CodeAction(action));
                    }
                    let changes = update
                        .into_iter()
                        .map(|(_, range, new)| {
                            let mut start = store.byte_offset_to_position(range.start);
                            start.character += 1;
                            let mut end = store.byte_offset_to_position(range.end);
                            end.character -= 1;
                            TextEdit::new(Range::new(start, end), new)
                        })
                        .collect();

                    let action = CodeAction {
                        title: "Upgrade All".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(new_workspace_edit(uri_, changes)),
                        ..CodeAction::default()
                    };
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }

                let action = CodeAction {
                    title: "Update All".to_string(),
                    kind: Some(CodeActionKind::EMPTY),
                    command: Some(Command {
                        title: "Update All".to_string(),
                        command: "cargo update".to_string(),
                        arguments: None,
                    }),
                    ..CodeAction::default()
                };
                actions.push(CodeActionOrCommand::CodeAction(action));
            }
        }

        Ok(Some(actions))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> tower_lsp::jsonrpc::Result<Option<serde_json::Value>> {
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

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri_ = params.text_document.uri.clone();
        let uri = uri_.to_string();
        if !uri.ends_with("/Cargo.toml") {
            return;
        }
        let members = if let Some(v) = self.toml_store.lock().await.get_mut(&uri) {
            v.update(params).await;
            v.get_members()
        } else {
            vec![]
        };
        self.make_sure_open(members).await;
        let diagnostics = self.analyze(&uri).await;
        self.client
            .publish_diagnostics(uri_, diagnostics, None)
            .await;
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        // let uri = params.text_document.uri.to_string();
        // self.toml_store.lock().await.remove(&uri);
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text = params.text_document.text;
        let uri = params.text_document.uri.to_string();
        let store = Store::new(text, self.workspace_root.clone()).await;
        let members = store.get_members();
        let _ = self.toml_store.lock().await.insert(uri.clone(), store);
        self.make_sure_open(members).await;
        let diagnostics = self.analyze(&uri).await;
        self.client
            .publish_diagnostics(params.text_document.uri, diagnostics, None)
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        if !uri.ends_with("/Cargo.toml") {
            return Ok(Some(CompletionResponse::Array(vec![])));
        }
        let lock = self.toml_store.lock().await;
        if let Some(store) = lock.get(&uri) {
            let byte_offset =
                get_byte_index_from_position(store.text(), params.text_document_position.position)
                    as u32;

            if let Some(info) = store.tree.get_item_by_pos(byte_offset) {
                if let Some(path) = get_after_key("dependencies", &info)
                    .or(get_after_key("dev-dependencies", &info))
                {
                    let path = path.iter().map(|v| v.owned()).collect::<Vec<_>>();
                    drop(lock);
                    match path.len() {
                        1 => {
                            let crate_ = &path[0];
                            let v = self
                                .complete_1(crate_.as_key(), &uri)
                                .await
                                .unwrap_or_default();
                            return Ok(Some(CompletionResponse::Array(v)));
                        }
                        2 => {
                            let crate_ = &path[0];
                            let part2 = &path[1];
                            let v = self
                                .complete_2(crate_.as_key(), part2, &uri)
                                .await
                                .unwrap_or_default();
                            return Ok(Some(CompletionResponse::Array(v)));
                        }
                        3 => {
                            let v = self
                                .complete_3(
                                    path[0].as_key(),
                                    path[1].as_key(),
                                    path[2].as_value(),
                                    &uri,
                                )
                                .await
                                .unwrap_or_default();
                            return Ok(Some(CompletionResponse::Array(v)));
                        }
                        _ => {
                            self.client
                                .log_message(MessageType::INFO, "unexpected ammount of args")
                                .await
                        }
                    };
                } else if let Some(path) = get_after_key("features", &info) {
                    if path.len() == 2 {
                        if let KeyOrValue::Value(val) = path[1] {
                            if let Some(query) = val.as_str() {
                                let mut existing = store
                                    .tree
                                    .by_array_child(*val)
                                    .and_then(|v| v.as_array())
                                    .and_then(|v| {
                                        Some(
                                            v.into_iter()
                                                .flat_map(|v| v.as_str())
                                                .collect::<Vec<_>>(),
                                        )
                                    })
                                    .unwrap_or_default();
                                if let Some(query) = query.strip_prefix("dep:") {
                                    let existing: Vec<_> = existing
                                        .iter()
                                        .filter_map(|v| v.strip_prefix("dep:"))
                                        .collect();
                                    let v = store
                                        .crates_info
                                        .iter()
                                        .map(|v| &v.key.value)
                                        .filter(|v| v.as_str().starts_with(query))
                                        .filter(|v| !existing.contains(&v.as_str()))
                                        .map(|v| CompletionItem {
                                            label: v.to_string(),
                                            ..Default::default()
                                        })
                                        .collect::<Vec<_>>();
                                    return Ok(Some(CompletionResponse::Array(v)));
                                } else {
                                    if let Some(v) = path[0].as_str() {
                                        existing.push(v);
                                    }
                                    let v = store
                                        .features
                                        .keys()
                                        .filter(|v| v.as_str().starts_with(query.as_str()))
                                        .filter(|v| !existing.contains(v))
                                        .map(|v| CompletionItem {
                                            label: v.to_string(),
                                            ..Default::default()
                                        })
                                        .collect();
                                    return Ok(Some(CompletionResponse::Array(v)));
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(Some(CompletionResponse::Array(vec![])))
    }
}

impl Backend {
    async fn get_root_dependencies(&self, uri2: &str) -> Option<Vec<String>> {
        if let Some(root) = &*self.workspace_root.read().await {
            let root = root.join("Cargo.toml");
            let uri = Url::from_file_path(root).unwrap().to_string();
            if uri == uri2 {
                return None;
            }
            let lock = self.toml_store.lock().await;
            let store = lock.get(&uri)?;
            Some(
                store
                    .crates_info
                    .iter()
                    .map(|v| v.key.value.clone())
                    .collect(),
            )
        } else {
            None
        }
    }
    async fn complete_1(&self, crate_name: Option<&Key>, uri: &str) -> Option<Vec<CompletionItem>> {
        let crate_name = crate_name?;
        let existing_crates = {
            let lock = self.toml_store.lock().await;
            if let Some(v) = lock.get(uri) {
                v.crates_info.iter().map(|v| v.key.value.clone()).collect()
            } else {
                vec![]
            }
        };

        let root_dep = self.get_root_dependencies(uri).await.unwrap_or_default();
        let mut result = self.crates.read().await.search(&crate_name.value).await;
        result.sort_by(|(name_a, _, _), (name_b, _, _)| name_a.len().cmp(&name_b.len()));
        Some(
            result
                .into_iter()
                .filter(|(crate_name_, _, _)| {
                    crate_name_ == &crate_name.value || !existing_crates.contains(crate_name_)
                })
                .map(|(name, detail, version)| CompletionItem {
                    label: name.clone(),
                    detail,
                    insert_text: Some(match root_dep.contains(&name) {
                        true => format!("{name} = {} workspace = true {}", '{', '}'),
                        false => format!("{name} = \"{version}\""),
                    }),
                    kind: Some(CompletionItemKind::SNIPPET),
                    ..Default::default()
                })
                .enumerate()
                .map(|(index, mut item)| {
                    item.sort_text = Some(format!("{:04}", index));
                    item
                })
                .collect::<Vec<_>>(),
        )
    }

    async fn complete_2(
        &self,
        crate_name: Option<&Key>,
        part2: &KeyOrValueOwned,
        uri: &str,
    ) -> Option<Vec<CompletionItem>> {
        let crate_name = crate_name?;
        match part2 {
            KeyOrValueOwned::Key(key) => {
                let (data, range, is_missing) = {
                    let lock = self.toml_store.lock().await;
                    let toml = lock.get(uri)?;
                    let cr =
                        toml.find_crate_by_byte_offset_range(key.range.start, key.range.end)?;
                    let v = cr.value.as_tree()?;
                    let (a, range) = match key.value.is_empty() {
                        true => {
                            if v.0.len() == 1 {
                                if let Some(f) = v.0.first() {
                                    if matches!(f.value, Value::NoContent) {
                                        Some((f.key.value.to_string(), f.key.range))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        false => v
                            .get(&key.value)
                            .and_then(|v| v.as_str_value().map(|b| (b.0.to_string(), *b.1))),
                    }?;
                    let content = &toml.content[range.end as usize..];
                    let nl = content.find("\n");
                    let close = content.find("}");
                    let is_missing = match (nl, close) {
                        (None, None) => true,
                        (None, Some(_)) => false,
                        (Some(_), None) => true,
                        (Some(a), Some(b)) => b > a,
                    };
                    let range = Range::new(
                        toml.byte_offset_to_position(range.start),
                        toml.byte_offset_to_position(range.end),
                    );
                    (a, range, is_missing)
                };

                match key.value.is_empty() || "version".starts_with(&key.value) {
                    true => Some(vec![CompletionItem {
                        label: "version...".to_string(),
                        detail: None,
                        kind: Some(CompletionItemKind::SNIPPET),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                            range,
                            format!(
                                "version = {}{}",
                                match key.value.is_empty() {
                                    true => data.as_str(),
                                    false => data.strip_prefix(&key.value).unwrap_or(data.as_str()),
                                },
                                match is_missing {
                                    true => "}",
                                    false => "",
                                }
                            ),
                        ))),
                        ..Default::default()
                    }]),
                    false => None,
                }
            }
            KeyOrValueOwned::Value(Value::String { value, range }) => {
                let versions = self
                    .crates
                    .read()
                    .await
                    .get_versions(&crate_name.value, value)
                    .await?;
                let range = {
                    let lock = self.toml_store.lock().await;
                    lock.get(uri).map(|v| v.inner_string_range(range))
                };
                Some(
                    versions
                        .into_iter()
                        .rev()
                        .enumerate()
                        .map(|(index, v)| CompletionItem {
                            label: v.to_string(),
                            sort_text: Some(format!("{:04}", index)),
                            detail: None,
                            text_edit: range.clone().map(|range| {
                                CompletionTextEdit::Edit(TextEdit::new(range, v.to_string()))
                            }),
                            ..Default::default()
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    async fn complete_3(
        &self,
        crate_name: Option<&Key>,
        expanded_key: Option<&Key>,
        data: Option<&Value>,
        uri: &str,
    ) -> Option<Vec<CompletionItem>> {
        let crate_name = crate_name?;
        let expanded_key = expanded_key?;
        let data = data?;
        let (value, range) = data.as_str_value()?;
        let lock = self.toml_store.lock().await;
        let others = lock
            .get(uri)?
            .tree
            .by_array_child(data)
            .and_then(|v| v.as_array())
            .and_then(|v| Some(v.into_iter().flat_map(|v| v.as_str()).collect::<Vec<_>>()))
            .unwrap_or_default();
        drop(lock);
        match expanded_key.value.as_str() {
            "features" => {
                let version = self.get_version_for_features(&uri, expanded_key).await?;
                let v = self
                    .crates
                    .read()
                    .await
                    .get_features(&crate_name.value, &version, value)
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|v| !others.contains(v))
                    .map(|v| CompletionItem {
                        label: v.to_string(),
                        detail: None,
                        ..Default::default()
                    })
                    .collect();
                Some(v)
            }
            "version" => {
                let versions = self
                    .crates
                    .read()
                    .await
                    .get_versions(&crate_name.value, value)
                    .await?;

                let range = {
                    let lock = self.toml_store.lock().await;
                    lock.get(uri).map(|v| v.inner_string_range(range))
                };

                Some(
                    versions
                        .into_iter()
                        .rev()
                        .enumerate()
                        .map(|(index, v)| CompletionItem {
                            label: v.to_string(),
                            sort_text: Some(format!("{:04}", index)),
                            detail: None,
                            text_edit: range.clone().map(|range| {
                                CompletionTextEdit::Edit(TextEdit::new(range, v.to_string()))
                            }),
                            ..Default::default()
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }

    async fn analyze(&self, uri: &str) -> Vec<Diagnostic> {
        if let Some(store) = self.toml_store.lock().await.get(uri) {
            let updates = store.needs_update(&self.crates).await;
            let mut items = updates
                .unwrap_or_default()
                .into_iter()
                .map(|(name, range, new)| {
                    let mut start = store.byte_offset_to_position(range.start);
                    start.character += 1;
                    let mut end = store.byte_offset_to_position(range.end);
                    end.character -= 1;
                    Diagnostic::new(
                        Range::new(start, end),
                        Some(DiagnosticSeverity::WARNING),
                        None,
                        None,
                        format!("A newer version is available for crate `{name}`: {new} "),
                        None,
                        None,
                    )
                })
                .collect::<Vec<_>>();
            let unknown_features = store.unknown_features(&self.crates).await;
            items.append(
                &mut unknown_features
                    .into_iter()
                    .map(|(name, range, feature)| {
                        let mut start = store.byte_offset_to_position(range.start);
                        start.character += 1;
                        let mut end = store.byte_offset_to_position(range.end);
                        end.character -= 1;
                        Diagnostic::new(
                            Range::new(start, end),
                            Some(DiagnosticSeverity::ERROR),
                            None,
                            None,
                            format!("Unknown feature `{feature}` for crate `{name}` "),
                            None,
                            None,
                        )
                    })
                    .collect::<Vec<_>>(),
            );

            let invalid_versions = store.invalid_versions(&self.crates).await;
            items.append(
                &mut invalid_versions
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(name, range)| {
                        let mut start = store.byte_offset_to_position(range.start);
                        start.character += 1;
                        let mut end = store.byte_offset_to_position(range.end);
                        end.character -= 1;
                        Diagnostic::new(
                            Range::new(start, end),
                            Some(DiagnosticSeverity::ERROR),
                            None,
                            None,
                            format!("Unknown version for crate `{name}` "),
                            None,
                            None,
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            return items;
        }
        vec![]
    }

    async fn get_version_for_features(&self, uri: &str, expanded_key: &Key) -> Option<String> {
        let lock = self.toml_store.lock().await;
        let data = lock.get(uri)?;
        let tree = data.tree.by_key(expanded_key)?;
        let version = tree.get("version")?.as_str()?;
        Some(version)
    }

    async fn make_sure_open(&self, uris: Vec<Url>) {
        if uris.is_empty() {
            return;
        }
        for uri in uris {
            let mut lock = self.toml_store.lock().await;
            let uri_ = uri.to_string();
            if lock.get(&uri_).is_none() {
                if let Ok(path) = uri.to_file_path() {
                    if let Ok(text) = read_to_string(path) {
                        let store = Store::new(text, self.workspace_root.clone()).await;
                        lock.insert(uri_, store);
                    }
                }
            }
        }
    }
}

pub async fn main(path: PathBuf) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (client, server) = LspService::build(|client| Backend {
        client,
        toml_store: Default::default(),
        crates: shared(Deamon::Starting),
        path,
        workspace_root: shared(None),
    })
    .finish();

    Server::new(stdin, stdout, server).serve(client).await;
}
