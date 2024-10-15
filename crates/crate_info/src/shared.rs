use std::path::{Path, PathBuf};

use parser::structure::RustVersion;
use util::config::Config;

use crate::{CratesIoStorage, CratesIoStorageReader};

pub enum CrateLookUp {
    Deamon((CratesIoStorageReader, Config, PathBuf)),
    NoDeamon(CratesIoStorage),
    Starting,
}

pub fn start_daemon(port: u16, storage: &Path, stable: bool, offline: bool, per_page_web: u32) {
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
    let _ = std::process::Command::new(current_exe).args(&args).spawn();
}

impl CrateLookUp {
    pub async fn handle_error(&self, e: tcp_struct::Error) {
        match e {
            tcp_struct::Error::StreamError(error) => match error.kind.as_str() {
                "connection refused" => {
                    if let CrateLookUp::Deamon((_, config, path)) = &self {
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
                CrateLookUp::Deamon((daemon, config, path)) => {
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
    pub async fn search(&self, query: &str) -> Vec<(String, Option<String>, String)> {
        match self {
            CrateLookUp::Deamon((v, _, _)) => match v.search(query).await {
                Ok(v) => v,
                Err(e) => {
                    self.handle_error(e).await;
                    Default::default()
                }
            },
            CrateLookUp::NoDeamon(arc) => arc.search(query).await,
            CrateLookUp::Starting => Default::default(),
        }
    }
    pub async fn get_version_local(&self, name: &str) -> Option<Vec<RustVersion>> {
        match self {
            CrateLookUp::Deamon((daemon, _, _)) => match daemon.get_version_local(name).await {
                Ok(v) => v,
                Err(e) => {
                    self.handle_error(e).await;
                    Default::default()
                }
            },
            CrateLookUp::NoDeamon(storage) => storage.get_version_local(name).await,
            CrateLookUp::Starting => Default::default(),
        }
    }
    pub async fn get_versions(&self, name: &str, version_filter: &str) -> Option<Vec<RustVersion>> {
        match self {
            CrateLookUp::Deamon((daemon, _, _)) => {
                match daemon.get_versions(name, version_filter).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.handle_error(e).await;
                        Default::default()
                    }
                }
            }
            CrateLookUp::NoDeamon(storage) => storage.get_versions(name, version_filter).await,
            CrateLookUp::Starting => Default::default(),
        }
    }

    pub async fn get_features_local(&self, name: &str, version: &str) -> Option<Vec<String>> {
        match self {
            CrateLookUp::Deamon((daemon, _, _)) => {
                match daemon.get_features_local(name, version).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.handle_error(e).await;
                        Default::default()
                    }
                }
            }
            CrateLookUp::NoDeamon(storage) => storage.get_features_local(name, version).await,
            CrateLookUp::Starting => Default::default(),
        }
    }

    pub async fn get_features(
        &self,
        name: &str,
        version: &str,
        search: &str,
    ) -> Option<Vec<String>> {
        match self {
            CrateLookUp::Deamon((daemon, _, _)) => {
                match daemon.get_features(name, version, search).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.handle_error(e).await;
                        Default::default()
                    }
                }
            }
            CrateLookUp::NoDeamon(storage) => storage.get_features(name, version, search).await,
            CrateLookUp::Starting => Default::default(),
        }
    }
}
