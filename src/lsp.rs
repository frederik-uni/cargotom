use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::crate_lookup::{CratesIoStorage, Shared};
use crate::generate_tree::{
    get_after_key, parse_toml, Key, KeyOrValueOwned, RangeExclusive, Tree, TreeValue, Value,
};
use crate::helper::{get_byte_index_from_position, new_workspace_edit, shared};
use crate::rust_version::RustVersion;

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
    pub fn new(s: String) -> Self {
        let mut s = Self {
            tree: parse_toml(&s),
            content: s,
            crates_info: vec![],
        };
        s.crates();
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
        crates: &Shared<CratesIoStorage>,
    ) -> Vec<(String, RangeExclusive, String)> {
        let lock = crates.read().await;
        let mut res = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, _)) = cr.get_version() {
                let features = lock.get_features_local(crate_name, &version).await;
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
        crates: &Shared<CratesIoStorage>,
    ) -> Option<Vec<(String, RangeExclusive, String)>> {
        let lock = crates.read().await;
        let mut updates = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, range)) = cr.get_version() {
                let crate_version = RustVersion::try_from(version.as_str()).ok()?;
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
        Some(updates)
    }

    pub async fn invalid_versions(
        &self,
        crates: &Shared<CratesIoStorage>,
    ) -> Option<Vec<(String, RangeExclusive)>> {
        let lock = crates.read().await;
        let mut updates = vec![];
        for cr in self.crates_info.iter() {
            let crate_name = &cr.key.value;
            if let Some((version, range)) = cr.get_version() {
                let crate_version = RustVersion::try_from(version.as_str()).ok()?;
                if let Some(v) = lock.get_version_local(crate_name).await {
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

    fn crates(&mut self) {
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
        *self.crates.write().await = CratesIoStorage::new(
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
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec!["\"".to_string()]),
                ..Default::default()
            }),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["\"".to_string()]),
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

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri_ = params.text_document.uri.clone();
        let uri = uri_.to_string();
        if !uri.ends_with("/Cargo.toml") {
            return;
        }
        if let Some(v) = self.toml_store.lock().await.get_mut(&uri) {
            v.update(params);
        }
        let diagnostics = self.analyze(&uri).await;
        self.client
            .publish_diagnostics(uri_, diagnostics, None)
            .await;
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
                            let v = self.complete_1(crate_.as_key()).await.unwrap_or_default();
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
                }
            }
        }
        Ok(Some(CompletionResponse::Array(vec![])))
    }
}

impl Backend {
    async fn complete_1(&self, crate_name: Option<&Key>) -> Option<Vec<CompletionItem>> {
        let crate_name = crate_name?;
        let result = self.crates.read().await.search(&crate_name.value).await;
        Some(
            result
                .into_iter()
                .map(|(name, detail, version)| CompletionItem {
                    label: name.clone(),
                    detail,
                    insert_text: Some(format!("{name} = \"{version}\"")),
                    ..Default::default()
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
                        .map(|v| CompletionItem {
                            label: v.to_string(),
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
        let (value, range) = data?.as_str_value()?;
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
                        .map(|v| CompletionItem {
                            label: v.to_string(),
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
