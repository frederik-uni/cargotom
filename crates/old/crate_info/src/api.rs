use std::{collections::HashMap, sync::Arc};

use parser::structure::RustVersion;
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub enum SearchCacheEntry {
    Pending(Arc<Notify>),
    Ready(Vec<Crate>),
}

#[derive(Clone)]
pub enum InfoCacheEntry {
    Pending(Arc<Notify>),
    Ready(Vec<VersionExport>),
}

impl CratesIoStorage {
    pub async fn search_online(&self, query: &str) -> Result<Vec<Crate>, reqwest::Error> {
        let notify = Arc::new(Notify::new());
        {
            let entry = self.search_cache.read().await.get(query).cloned();
            if let Some(entry) = entry {
                match entry {
                    SearchCacheEntry::Pending(existing_notify) => {
                        existing_notify.notified().await;
                        let entry = self.search_cache.read().await.get(query).cloned();
                        if let Some(SearchCacheEntry::Ready(result)) = entry {
                            return Ok(result);
                        }
                    }
                    SearchCacheEntry::Ready(result) => {
                        return Ok(result);
                    }
                }
            }
        }

        let url = format!(
            "https://crates.io/api/v1/crates?page=1&per_page={}&q={}&sort=relevance",
            self.per_page,
            urlencoding::encode(query)
        );

        self.search_cache
            .write()
            .await
            .insert(query.to_string(), SearchCacheEntry::Pending(notify.clone()));

        let res = self.client.get(&url).header(USER_AGENT, "zed").send().await;
        let res: Result<SearchResponse, _> = match res {
            Ok(v) => v.json().await,
            Err(e) => Err(e),
        };

        match res {
            Ok(search_response) => {
                {
                    let mut cache_lock = self.search_cache.write().await;
                    cache_lock.insert(
                        query.to_string(),
                        SearchCacheEntry::Ready(search_response.crates.clone()),
                    );
                }
                notify.notify_waiters();
                Ok(search_response.crates)
            }
            Err(e) => {
                {
                    let mut cache_lock = self.search_cache.write().await;
                    cache_lock.remove(query);
                }
                notify.notify_waiters();
                Err(e)
            }
        }
    }

    pub async fn versions_features(
        &self,
        name: &str,
    ) -> Result<Vec<VersionExport>, reqwest::Error> {
        let notify = Arc::new(Notify::new());
        {
            let entry = self.versions_cache.read().await.get(name).cloned();
            if let Some(entry) = entry {
                match entry {
                    InfoCacheEntry::Pending(existing_notify) => {
                        existing_notify.notified().await;
                        let entry = self.versions_cache.read().await.get(name).cloned();
                        if let Some(InfoCacheEntry::Ready(result)) = entry {
                            return Ok(result);
                        }
                    }
                    InfoCacheEntry::Ready(result) => {
                        return Ok(result);
                    }
                }
            }
        }

        let url = format!("https://crates.io/api/v1/crates/{}/versions", name);

        self.versions_cache
            .write()
            .await
            .insert(name.to_string(), InfoCacheEntry::Pending(notify.clone()));

        let res = self.client.get(&url).header(USER_AGENT, "zed").send().await;
        let res: Result<VersionResponse, _> = match res {
            Ok(v) => v.json().await,
            Err(e) => Err(e),
        };

        match res {
            Ok(version_response) => {
                let versions = version_response.versions();
                {
                    let mut cache_lock = self.versions_cache.write().await;
                    cache_lock.insert(name.to_string(), InfoCacheEntry::Ready(versions.clone()));
                }
                notify.notify_waiters();
                Ok(versions)
            }
            Err(e) => {
                {
                    let mut cache_lock = self.versions_cache.write().await;
                    cache_lock.remove(name);
                }
                notify.notify_waiters();
                Err(e)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Crate {
    exact_match: bool,
    pub name: String,
    pub description: Option<String>,
    pub max_stable_version: Option<String>,
    pub max_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SearchResponse {
    crates: Vec<Crate>,
}

use serde_json::Value;
use tokio::sync::Notify;

use crate::CratesIoStorage;

#[derive(Debug, Clone)]
pub struct VersionExport {
    pub version: RustVersion,
    pub features: Vec<String>,
}

impl VersionExport {
    pub fn matches_version(&self, version: &str) -> bool {
        let v = RustVersion::try_from(version);
        match v {
            Ok(v) => self.version.eq(&v),
            Err(_) => false,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct VersionResponse {
    pub versions: Vec<Version>,
}

impl VersionResponse {
    pub fn versions(self) -> Vec<VersionExport> {
        self.versions
            .into_iter()
            .filter(|v| !v.yanked)
            .filter(|v| v.has_lib)
            .map(|v| VersionExport {
                version: RustVersion::try_from(v.num.as_str()).unwrap(),
                features: v.features.into_iter().map(|v| v.0).collect(),
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Version {
    pub features: HashMap<String, Value>,
    pub num: String,
    #[serde(rename = "has_lib")]
    pub has_lib: bool,
    pub yanked: bool,
}
