use std::{fs::read_to_string, path::PathBuf};

use crate_info::{
    shared::{start_daemon, CrateLookUp},
    CratesIoStorage,
};
use parser::{
    structure::{CargoRawData, Lock, Workspace},
    Cargo,
};
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CodeActionProviderCapability, CompletionOptions, HoverProviderCapability, InitializeParams,
        InitializeResult, MessageType, OneOf, ServerCapabilities, SignatureHelpOptions,
        TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, Url,
    },
};
use util::{config::Config, crate_version};

use crate::{
    context::{Context, Toml},
    util::remove_file_prefix,
};

fn load_config(params: &InitializeParams) -> Config {
    params
        .initialization_options
        .clone()
        .map(serde_json::from_value)
        .and_then(|v| v.ok())
        .unwrap_or_default()
}

impl Context {
    async fn deamon_update(&self, config: Config) {
        match config.daemon {
            true => {
                let daemon = CratesIoStorage::read(config.daemon_port, crate_version());
                match daemon.update(config).await {
                    Ok(_) => {}
                    Err(err) => {
                        self.client
                            .log_message(MessageType::INFO, format!("{err:?}"))
                            .await;
                        match err {
                            tcp_struct::Error::StreamError(error) => match error.kind.as_str() {
                                "connection refused" => start_daemon(
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
                        }
                    }
                }
                *self.crates.write().await =
                    CrateLookUp::Deamon((daemon, config, self.path.clone()));
            }
            false => {
                *self.crates.write().await = CrateLookUp::NoDeamon(CratesIoStorage::new(
                    &self.path,
                    config.stable,
                    config.offline,
                    config.per_page_web,
                ));
            }
        }
    }

    async fn init_cache(&self, params: &InitializeParams) {
        if let Some(root_uri) = &params.root_uri {
            let mut path = root_uri.to_file_path().unwrap();
            path = remove_file_prefix(&path).unwrap_or(path);
            let file = path.join("Cargo.toml");
            let lockfile = path.join("Cargo.lock");

            *self.workspace_root.write().await = Some(path);
            let mut lock_file_path = None;
            if let (Ok(text), Ok(uri)) = (read_to_string(&lockfile), Url::from_file_path(&lockfile))
            {
                if let Some(lock) = Lock::new(text) {
                    let lock = Toml::Lock(lock);
                    let _ = self.toml_store.write().await.insert(uri.to_string(), lock);
                    lock_file_path = Some(lockfile)
                }
            }
            if let Some((toml, root, uri)) = self.path_to_toml(file, &mut lock_file_path) {
                let _ = self.toml_store.write().await.insert(uri.to_string(), toml);

                if let Some(root) = root {
                    if let Some((toml, _, uri)) =
                        self.path_to_toml(root.clone(), &mut lock_file_path)
                    {
                        let members = toml.get_members();
                        let _ = self.toml_store.write().await.insert(uri.to_string(), toml);
                        self.open_files(
                            members,
                            lock_file_path,
                            Workspace::WorkspaceModule { path: root },
                        )
                        .await;
                    }
                }
            }
        }
    }

    fn path_to_toml(
        &self,
        file: PathBuf,
        lock_file_path: &mut Option<PathBuf>,
    ) -> Option<(Toml, Option<PathBuf>, Url)> {
        if let (Ok(text), Ok(uri)) = (read_to_string(&file), Url::from_file_path(&file)) {
            let mut toml = Toml::Cargo {
                cargo: Cargo::new(file.clone(), lock_file_path, Workspace::Package),
                raw: CargoRawData::new(text),
            };
            let reload = toml.reload();
            return Some((toml, reload, uri));
        }
        None
    }

    pub async fn open_files(
        &self,
        paths: Vec<PathBuf>,
        mut lock_file: Option<PathBuf>,
        workspace: Workspace,
    ) {
        for path in paths {
            match glob::glob(path.to_str().unwrap_or_default()) {
                Ok(entries) => {
                    for entry in entries.into_iter().filter_map(|e| e.ok()) {
                        let uri = match Url::from_file_path(&entry) {
                            Ok(v) => v,
                            Err(_) => {
                                self.client
                                    .log_message(
                                        MessageType::ERROR,
                                        format!("failed to parse uri: {entry:?}"),
                                    )
                                    .await;
                                continue;
                            }
                        };
                        let uri_ = uri.to_string();

                        let mut lock = self.toml_store.write().await;
                        if lock.get(&uri_).is_none() {
                            if let Ok(path) = uri.to_file_path() {
                                if let Ok(text) = read_to_string(path) {
                                    let mut toml = Toml::Cargo {
                                        cargo: Cargo::new(entry, &mut lock_file, workspace.clone()),
                                        raw: CargoRawData::new(text),
                                    };
                                    toml.reload();
                                    lock.insert(uri_, toml);
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    //TODO: warn user
                }
            }
        }
    }
}

impl Context {
    pub async fn initialize_(&self, params: InitializeParams) -> Result<InitializeResult> {
        let config = load_config(&params);
        *self.hide_docs_info_message.write().await = config.hide_docs_info_message;
        *self.sort.write().await = config.sort;

        self.deamon_update(config).await;
        self.init_cache(&params).await;
        let capabilities = ServerCapabilities {
            inlay_hint_provider: Some(OneOf::Left(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
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
}
