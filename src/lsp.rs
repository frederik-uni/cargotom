use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::crate_lookup::{shared, CratesIoStorage, Shared};
use crate::generate_tree::{get_after_key, parse_toml, Key, KeyOrValueOwned, Tree, Value};

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
}

impl Store {
    pub fn new(s: String) -> Self {
        Self {
            tree: parse_toml(&s),
            content: s,
        }
    }
    pub fn text(&self) -> &str {
        &self.content
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
        self.toml_store.lock().await.insert(uri, Store::new(text));
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
                                        //todo:
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
