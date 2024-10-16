use std::{fs::read_to_string, path::PathBuf};

use crate_info::shared::CrateLookUp;
use parser::{
    structure::{CargoRawData, Workspace},
    Cargo,
};
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CodeActionParams, CodeActionResponse, CompletionParams, CompletionResponse,
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentFormattingParams, ExecuteCommandParams, Hover, HoverParams, InitializeParams,
        InitializeResult, InitializedParams, InlayHint, InlayHintParams, MessageType, TextEdit,
        Url,
    },
    LanguageServer, LspService, Server,
};
use util::shared;

use crate::{
    context::{Context, Toml},
    util::remove_file_prefix,
};

#[tower_lsp::async_trait]
impl LanguageServer for Context {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.initialize_(params).await
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "LSP Initialized")
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        if !self.shoud_allow_user(&params.text_document_position_params.text_document.uri) {
            return Ok(None);
        }
        match self.hover_(params).await {
            Some(v) => v,
            None => Ok(None),
        }
    }
    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        if !self.shoud_allow_user(&params.text_document.uri) {
            return Ok(None);
        }
        match self.code_action_(params).await {
            Some(v) => v,
            None => Ok(None),
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        match self.formatting_(params).await {
            Some(v) => v,
            None => Ok(None),
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        if !self.shoud_allow_user(&params.text_document_position.text_document.uri) {
            return Ok(None);
        }
        match self.completion_(params).await {
            Some(v) => v,
            None => Ok(None),
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> tower_lsp::jsonrpc::Result<Option<serde_json::Value>> {
        self.execute_command_(params).await?;
        Ok(None)
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if !uri.to_string().ends_with("/Cargo.toml") || uri.to_string().ends_with("Cargo.lock") {
            return;
        }
        let (members, lock_file) = if let Some(v) = self.toml_store.write().await.get_mut(&uri) {
            v.update(params).await;
            (
                v.get_members(),
                v.as_cargo().and_then(|v| v.lock_file_path.clone()),
            )
        } else {
            (vec![], None)
        };
        if let Ok(v) = uri
            .to_file_path()
            .map(|v| remove_file_prefix(&v).unwrap_or(v))
        {
            self.open_files(members, lock_file, Workspace::module(v))
                .await;
        }
        let diagnostics = self.analyze(&uri).await;
        let _ = self.client.inlay_hint_refresh().await;
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        match self.inlay_hint_(params).await {
            Some(v) => v,
            None => Ok(None),
        }
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let text = params.text_document.text;
        let uri = params.text_document.uri;
        if !uri.to_string().ends_with("/Cargo.toml") || uri.to_string().ends_with("Cargo.lock") {
            return;
        }
        if !self.shoud_allow_user(&uri) {
            return;
        }
        if let Ok(mut path) = uri.to_file_path() {
            path = remove_file_prefix(&path).unwrap_or(path);
            let (mut lock_file, workspace) = self
                .toml_store
                .write()
                .await
                .remove(&uri)
                .map(|v| match v {
                    Toml::Cargo { cargo, .. } => (cargo.lock_file_path, Some(cargo.info.workspace)),
                    _ => (None, None),
                })
                .unwrap_or_default();
            let mut store = Toml::Cargo {
                cargo: Cargo::new(
                    path.clone(),
                    &mut lock_file,
                    workspace.unwrap_or(Workspace::Package),
                ),
                raw: CargoRawData::new(text),
            };
            let root = store.reload();
            if let Some(root) = root {
                let _ = self.toml_store.write().await.insert(uri.clone(), store);
                let uri = match Url::from_file_path(&root) {
                    Ok(v) => v,
                    Err(_) => return,
                };

                let (mut lock_file, workspace) = self
                    .toml_store
                    .write()
                    .await
                    .remove(&uri)
                    .map(|v| match v {
                        Toml::Cargo { cargo, .. } => {
                            (cargo.lock_file_path, Some(cargo.info.workspace))
                        }
                        _ => (None, None),
                    })
                    .unwrap_or_default();
                let mut store = Toml::Cargo {
                    cargo: Cargo::new(
                        path.clone(),
                        &mut lock_file,
                        workspace.unwrap_or(Workspace::Package),
                    ),
                    raw: CargoRawData::new(read_to_string(root).unwrap_or_default()),
                };
                let _ = store.reload();
                let lock_file = store.as_cargo().and_then(|v| v.lock_file_path.clone());
                let members = store.get_members();
                let _ = self.toml_store.write().await.insert(uri, store);
                self.open_files(members, lock_file, Workspace::module(path))
                    .await;
            } else {
                let members = store.get_members();
                let lock_file = store.as_cargo().and_then(|v| v.lock_file_path.clone());
                let _ = self.toml_store.write().await.insert(uri.clone(), store);
                self.open_files(members, lock_file, Workspace::module(path))
                    .await;
            }
            let diagnostics = self.analyze(&uri).await;
        }
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        // let uri = params.text_document.uri.to_string();
        // self.toml_store.lock().await.remove(&uri);
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
pub async fn main(path: PathBuf) {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (client, server) = LspService::build(|client| Context {
        client,
        toml_store: Default::default(),
        crates: shared(CrateLookUp::Starting),
        path,
        workspace_root: shared(None),
        hide_docs_info_message: shared(false),
        sort: shared(false),
    })
    .finish();

    Server::new(stdin, stdout, server).serve(client).await;
}
