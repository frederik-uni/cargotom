use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::api::RustVersion;
use crate::crate_lookup::{shared, CratesIoStorage, Shared};
use crate::generate_tree::{
    get_after_key, parse_toml, Key, KeyOrValueOwned, RangeExclusive, Tree, TreeValue, Value,
};

struct Backend {
    crates: Shared<CratesIoStorage>,
    client: Client,
    path: PathBuf,
    toml_store: Arc<Mutex<HashMap<String, Store>>>,
}

#[derive(Debug, Deserialize, Default)]
struct Config {
    offline: Option<bool>,
    stable: Option<bool>,
    per_page_web: Option<u32>,
}

#[derive(Debug)]
struct Store {
    content: String,
    tree: Tree,
    crates_info: Vec<TreeValue>,
}

impl Store {
    pub fn byte_offset_to_position(&self, byte_offset: u32) -> Position {
        let byte_offset = byte_offset as usize;

        let content_slice = &self.content[..byte_offset];

        let line = content_slice.chars().filter(|&c| c == '\n').count() as u32;

        let line_start = content_slice.rfind('\n').map_or(0, |pos| pos + 1);
        let character = content_slice[line_start..].chars().count() as u32;
        Position::new(line, character)
    }

    pub async fn needs_update(
        &self,
        crates: &Arc<Mutex<CratesIoStorage>>,
    ) -> Vec<(String, RangeExclusive, String)> {
        let lock = crates.lock().await;
        let mut updates = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, range)) = cr.get_version() {
                let crate_version = RustVersion::from(version.as_str());
                let mut versions = lock
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
        updates
    }
    pub fn new(s: String) -> Self {
        let mut s = Self {
            tree: parse_toml(&s),
            content: s,
            crates_info: vec![],
        };
        s.crates();
        s
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    fn crates(&mut self) {
        let mut v = self.tree.find("dependencies");
        v.append(&mut self.tree.find("dev-dependencies"));
        let v = v
            .into_iter()
            .map(|v| match &v.value {
                Value::Tree(tree) => Ok(&tree.0),
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

    pub fn update(&mut self, params: DidChangeTextDocumentParams) {
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
        self.crates();
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
        *self.crates.lock().await = CratesIoStorage::new(
            &self.path,
            config.stable.unwrap_or(true),
            config.offline.unwrap_or(true),
            config.per_page_web.unwrap_or(25),
        );
        let capabilities = ServerCapabilities {
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    ..Default::default()
                },
            )),
            completion_provider: Some(CompletionOptions::default()),
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
                self.client
                    .log_message(MessageType::INFO, format!("{:#?}", v))
                    .await;
                let crate_name = &v.key.value;
                let update = store.needs_update(&self.crates).await;
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
                            edit: Some(WorkspaceEdit {
                                changes: Some(
                                    vec![(uri_.clone(), vec![edit])].into_iter().collect(),
                                ),
                                ..WorkspaceEdit::default()
                            }),
                            ..CodeAction::default()
                        };
                        actions.push(CodeActionOrCommand::CodeAction(action));
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
                        edit: Some(WorkspaceEdit {
                            changes: Some(vec![(uri_, changes)].into_iter().collect()),
                            ..WorkspaceEdit::default()
                        }),
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

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if !uri.ends_with("/Cargo.toml") {
            return;
        }
        if let Some(v) = self.toml_store.lock().await.get_mut(&uri) {
            v.update(params);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.toml_store.lock().await.remove(&uri);
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text = params.text_document.text;
        let uri = params.text_document.uri.to_string();
        let _ = self
            .toml_store
            .lock()
            .await
            .insert(uri.clone(), Store::new(text));
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
        if let Some(v) = lock.get(&uri) {
            let byte_offset =
                get_byte_index_from_position(v.text(), params.text_document_position.position)
                    as u32;

            if let Some(info) = v.tree.get_item_by_pos(byte_offset) {
                if let Some(path) = get_after_key("dependencies", &info)
                    .or(get_after_key("dev-dependencies", &info))
                {
                    let path = path.iter().map(|v| v.owned()).collect::<Vec<_>>();
                    drop(lock);
                    match path.len() {
                        1 => {
                            //todo: use  pub text_edit:
                            let crate_ = &path[0];
                            if let KeyOrValueOwned::Key(key) = crate_ {
                                let result = self.crates.lock().await.search(&key.value).await;
                                let v = result
                                    .into_iter()
                                    .map(|(name, detail, version)| CompletionItem {
                                        label: name.clone(),
                                        detail,
                                        insert_text: Some(format!("{name} = \"{version}\"")),
                                        ..Default::default()
                                    })
                                    .collect::<Vec<_>>();
                                return Ok(Some(CompletionResponse::Array(v)));
                            }
                        }
                        2 => {
                            let crate_ = &path[0];
                            let part2 = &path[1];

                            if let KeyOrValueOwned::Key(key) = crate_ {
                                match part2 {
                                    KeyOrValueOwned::Key(_) => {
                                        //todo: suggest version with the version string
                                        //todo: use additional_text_edits to close }
                                    }
                                    KeyOrValueOwned::Value(Value::String { value, .. }) => {
                                        if let Some(v) = self
                                            .crates
                                            .lock()
                                            .await
                                            .get_versions(&key.value, value)
                                            .await
                                        {
                                            let v = v
                                                .into_iter()
                                                .map(|v| CompletionItem {
                                                    label: v.to_string(),
                                                    detail: None,
                                                    ..Default::default()
                                                })
                                                .collect();
                                            return Ok(Some(CompletionResponse::Array(v)));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        3 => {
                            if let KeyOrValueOwned::Key(crate_) = &path[0] {
                                if let KeyOrValueOwned::Key(expanded_key) = &path[1] {
                                    if let KeyOrValueOwned::Value(Value::String { value, .. }) =
                                        &path[2]
                                    {
                                        match expanded_key.value.as_str() {
                                            "features" => {
                                                if let Some(version) = self
                                                    .get_version_for_features(&uri, expanded_key)
                                                    .await
                                                {
                                                    let v = self
                                                        .crates
                                                        .lock()
                                                        .await
                                                        .get_features(
                                                            &crate_.value,
                                                            &version,
                                                            value,
                                                        )
                                                        .await
                                                        .into_iter()
                                                        .map(|v| CompletionItem {
                                                            label: v.to_string(),
                                                            detail: None,
                                                            ..Default::default()
                                                        })
                                                        .collect();
                                                    return Ok(Some(CompletionResponse::Array(v)));
                                                }
                                            }
                                            "version}" => {
                                                if let Some(v) = self
                                                    .crates
                                                    .lock()
                                                    .await
                                                    .get_versions(&crate_.value, value)
                                                    .await
                                                {
                                                    let v = v
                                                        .into_iter()
                                                        .map(|v| CompletionItem {
                                                            label: v.to_string(),
                                                            detail: None,
                                                            ..Default::default()
                                                        })
                                                        .collect();
                                                    return Ok(Some(CompletionResponse::Array(v)));
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                //search, features, version
                            }
                        }
                        _ => {
                            self.client
                                .log_message(MessageType::INFO, "unexpected ammount of args")
                                .await
                        }
                    };
                }
            }
        }
        Ok(Some(CompletionResponse::Array(vec![])))
    }
}

impl Backend {
    async fn get_version_for_features(&self, uri: &str, expanded_key: &Key) -> Option<String> {
        let lock = self.toml_store.lock().await;
        let data = lock.get(uri)?;
        let tree = data.tree.by_key(expanded_key)?;
        let version = tree.get("version")?.as_str()?;
        Some(version)
    }
}

pub fn get_byte_index_from_position(s: &str, position: Position) -> usize {
    let line_start = index_of_first_char_in_line(s, position.line).unwrap_or(s.len());

    let char_index = line_start + position.character as usize;

    if char_index >= s.len() {
        s.char_indices().nth(s.len() - 1).unwrap().0
    } else {
        s.char_indices().nth(char_index).unwrap().0
    }
}

fn index_of_first_char_in_line(s: &str, line: u32) -> Option<usize> {
    let mut current_line = 0;
    let mut index = 0;

    if line == 0 {
        return Some(0);
    }

    for (i, c) in s.char_indices() {
        if c == '\n' {
            current_line += 1;
            if current_line == line {
                return Some(i + 1);
            }
        }
        index = i;
    }

    if current_line == line - 1 {
        return Some(index + 1);
    }

    None
}

pub async fn main(path: PathBuf) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (client, server) = LspService::build(|client| Backend {
        client,
        toml_store: Default::default(),
        crates: shared(CratesIoStorage::dummy()),
        path,
    })
    .finish();

    Server::new(stdin, stdout, server).serve(client).await;
}
