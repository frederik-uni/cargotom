mod api;
mod git;
pub mod shared;

use std::{
    collections::HashMap,
    fs::{read_dir, read_to_string},
    path::{Path, PathBuf},
    process::exit,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type OfflineCratesData = Option<Trie<u8, Vec<(String, Vec<(String, Vec<Arc<String>>)>)>>>;

use parser::structure::RustVersion;
use reqwest::Client;
use tcp_struct::{register_impl, TCPShare};
use tokio::time::sleep;
use trie_rs::map::{Trie, TrieBuilder};
use util::{config::Config, shared, Shared};

use crate::{
    api::{InfoCacheEntry, SearchCacheEntry},
    git::updated_local_git,
};

impl CratesIoStorage {
    pub fn new(path: &Path, stable: bool, offline: bool, per_page_online: u32) -> Self {
        let data = shared(match offline {
            true => read_data(path),
            false => Default::default(),
        });
        let sel = Self {
            per_page: per_page_online,
            last_checked: shared(Duration::from_micros(0)),
            updating: shared(false),
            data,
            client: Client::new(),
            stable,
            search_cache: Default::default(),
            versions_cache: Default::default(),
        };
        if offline {
            update_thread(sel.clone(), path.to_path_buf());
        }
        sel
    }
}

fn read_data(path: &Path) -> OfflineCratesData {
    if let Ok(dir) = read_dir(path.join("index")) {
        let mut hm: HashMap<String, Vec<(String, Vec<(String, Vec<Arc<String>>)>)>> =
            HashMap::new();
        for file in dir.into_iter().filter_map(|v| v.ok()) {
            let file = file.path();
            let name = file
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default();
            if !name.ends_with(".json") || name.starts_with('.') {
                continue;
            }
            for (key, value) in read_to_string(file)
                .unwrap_or_default()
                .split('\n')
                .filter_map(|line| {
                    serde_json::from_str::<(String, Vec<(String, Vec<String>)>)>(&format!(
                        "[{line}]"
                    ))
                    .ok()
                })
                .map(|(key, value)| (normalize_key(&key), (key, value)))
            {
                let item = hm.entry(key).or_default();
                let (a, b) = value;
                item.push((a, post_process_value(b)));
            }
        }
        if hm.is_empty() {
            return None;
        }
        let mut builder = TrieBuilder::new();
        for (key, value) in hm {
            builder.push(key, value);
        }
        let trie = builder.build();

        Some(trie)
    } else {
        None
    }
}

fn post_process_value(value: Vec<(String, Vec<String>)>) -> Vec<(String, Vec<Arc<String>>)> {
    let mut out = vec![];
    let mut features = HashMap::new();
    for (version, f) in value {
        for feature in f {
            let added = feature.starts_with("+");
            let feature = &feature[1..];
            if added {
                features.insert(feature.to_string(), Arc::new(feature.to_string()));
            } else {
                features.remove(feature);
            }
        }
        out.push((version, features.values().cloned().collect::<Vec<_>>()))
    }
    out
}

#[register_impl]
impl CratesIoStorage {
    pub fn update(&self, config: Config) -> bool {
        //TODO: update daemon
        true
    }
    pub fn stop(&self) {
        exit(0);
    }
    pub async fn search(&self, query: &str) -> Vec<(String, Option<String>, String)> {
        let lock = self.data.read().await;
        if let Some(v) = &*lock {
            let search = v
                .predictive_search(query.to_lowercase().replace("-", "").replace("_", ""))
                .map(|(_, a): (String, &Vec<_>)| a)
                .collect::<Vec<_>>()
                .clone();
            let search = search
                .iter()
                .flat_map(|v| v.iter())
                .map(|(a, b)| {
                    (
                        a.to_string(),
                        None,
                        match self.stable {
                            true => b
                                .iter()
                                .rev()
                                .find(|(version, _)| {
                                    match RustVersion::try_from(version.as_str()) {
                                        Ok(v) => v.is_patch_int(),
                                        Err(_) => false,
                                    }
                                })
                                .map(|(version, _)| version.to_string())
                                .unwrap_or_default(),
                            false => b
                                .last()
                                .map(|(version, _)| version.to_string())
                                .unwrap_or_default(),
                        },
                    )
                })
                .collect::<Vec<_>>();
            search
        } else {
            self.search_online(query)
                .await
                .map(|v| {
                    v.into_iter()
                        .map(|v| {
                            (
                                v.name,
                                v.description,
                                match self.stable {
                                    true => v.max_stable_version.or(v.max_version),
                                    false => v.max_version.or(v.max_stable_version),
                                }
                                .unwrap_or_default(),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
    }

    pub async fn get_version_local(&self, name: &str) -> Option<Vec<RustVersion>> {
        let lock = self.data.read().await;
        if let Some(v) = &*lock {
            let search = v.exact_match(normalize_key(name))?;
            let (_, versions) = search
                .iter()
                .find(|v| v.0.to_lowercase() == name.to_lowercase())?;
            Some(
                versions
                    .iter()
                    .map(|(version, _)| RustVersion::try_from(version.as_str()).unwrap())
                    .filter(|v| !self.stable || v.is_patch_int())
                    .collect::<Vec<_>>(),
            )
        } else {
            let v = self.versions_cache.read().await;
            match v.get(name) {
                Some(v) => match v {
                    InfoCacheEntry::Pending(_) => None,
                    InfoCacheEntry::Ready(v) => Some(
                        v.iter()
                            .map(|v| v.version.clone())
                            .filter(|v| !self.stable || v.is_patch_int())
                            .collect(),
                    ),
                },
                None => {
                    //INFO: thats probably fine, bc CratesIoStorage will exist until the lsp is stopped
                    let cpy = unsafe { (self as *const Self).as_ref() }.unwrap();
                    let name = name.to_string();
                    tokio::spawn(async move { cpy.versions_features(&name).await });
                    None
                }
            }
        }
    }

    pub async fn get_features_local(&self, name: &str, version: &str) -> Option<Vec<String>> {
        let ver = RustVersion::try_from(version).ok()?;
        let lock = self.data.read().await;
        if let Some(v) = &*lock {
            let search = v.exact_match(normalize_key(name))?;
            let (_, versions) = search
                .iter()
                .find(|v| v.0.to_lowercase() == name.to_lowercase())?;
            versions
                .iter()
                .find(|(version, _)| RustVersion::try_from(version.as_str()).unwrap().eq(&ver))
                .map(|v| v.1.iter().map(|v| v.to_string()).collect())
        } else {
            let v = self.versions_cache.read().await;
            match v.get(name) {
                Some(v) => match v {
                    InfoCacheEntry::Pending(_) => None,
                    InfoCacheEntry::Ready(v) => Some(
                        v.iter()
                            .find(|v| &v.version == &ver)
                            .map(|b| b.features.clone())
                            .unwrap_or_default(),
                    ),
                },
                None => {
                    //INFO: thats probably fine, bc CratesIoStorage will exist until the lsp is stopped
                    let cpy = unsafe { (self as *const Self).as_ref() }.unwrap();
                    let name = name.to_string();
                    tokio::spawn(async move { cpy.versions_features(&name).await });
                    None
                }
            }
        }
    }

    pub async fn get_features(
        &self,
        name: &str,
        version: &str,
        search: &str,
    ) -> Option<Vec<String>> {
        let search_query = search.to_lowercase();
        let lock = self.data.read().await;
        if let Some(v) = &*lock {
            let search = v.exact_match(normalize_key(name))?;
            let v = RustVersion::try_from(version).ok()?;
            let (_, versions) = search
                .iter()
                .find(|v| v.0.to_lowercase() == name.to_lowercase())?;
            versions
                .iter()
                .find(|(version, _)| RustVersion::try_from(version.as_str()).unwrap().eq(&v))
                .map(|v| {
                    v.1.iter()
                        .map(|v| v.to_string())
                        .filter(|v| v.starts_with(&search_query))
                        .collect()
                })
        } else {
            let temp = self.versions_features(name).await.ok();
            temp.and_then(|v| {
                v.into_iter()
                    .find(|v| v.matches_version(version))
                    .map(|v| v.features)
                    .map(|v| {
                        v.into_iter()
                            .map(|v| v.to_lowercase())
                            .filter(|v| v.starts_with(&search_query))
                            .collect::<Vec<_>>()
                    })
            })
        }
    }

    pub async fn get_versions(&self, name: &str, version_filter: &str) -> Option<Vec<RustVersion>> {
        let lock = self.data.read().await;
        if let Some(v) = &*lock {
            let search = v.exact_match(normalize_key(name))?;
            let (_, versions) = search
                .iter()
                .find(|v| v.0.to_lowercase() == name.to_lowercase())?;
            Some(
                versions
                    .iter()
                    .filter(|(version, _)| version.starts_with(version_filter))
                    .map(|(version, _)| RustVersion::try_from(version.as_str()).unwrap())
                    .collect::<Vec<_>>(),
            )
        } else {
            self.versions_features(name).await.ok().map(|v| {
                v.into_iter()
                    .map(|v| v.version)
                    .filter(|v| v.to_string().starts_with(version_filter))
                    .collect()
            })
        }
    }
}

fn normalize_key(key: &str) -> String {
    key.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub fn update_thread(data: CratesIoStorage, path: PathBuf) {
    tokio::spawn(async move {
        let need_update = {
            let updating = *data.updating.read().await;
            let last_checked = *data.last_checked.read().await;
            match updating {
                true => false,
                false => match last_checked
                    < SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
                        - Duration::from_secs(300)
                {
                    true => true,
                    false => false,
                },
            }
        };
        if need_update {
            update(data, &path).await;
        }
        sleep(Duration::from_secs(60)).await
    });
}

async fn update(toml_data: CratesIoStorage, path: &Path) {
    *toml_data.updating.write().await = true;

    let update = updated_local_git(path);
    if update {
        let data = read_data(path);
        *toml_data.data.write().await = data;
    }
    *toml_data.updating.write().await = false;
    *toml_data.last_checked.write().await = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
}

#[derive(Clone, TCPShare)]
pub struct CratesIoStorage {
    pub search_cache: Shared<HashMap<String, SearchCacheEntry>>,
    pub versions_cache: Shared<HashMap<String, InfoCacheEntry>>,
    last_checked: Shared<Duration>,
    updating: Shared<bool>,
    pub client: Client,
    pub stable: bool,
    pub per_page: u32,
    pub data: Shared<OfflineCratesData>,
}
