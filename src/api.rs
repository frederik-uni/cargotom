use std::{collections::HashMap, fmt::Display, sync::Arc};

use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};

pub enum SearchCacheEntry {
    Pending(Arc<Notify>),
    Ready(Vec<Crate>),
}

pub enum InfoCacheEntry {
    Pending(Arc<Notify>),
    Ready(Vec<VersionExport>),
}

impl CratesIoStorage {
    pub async fn search_online(&self, query: &str) -> Result<Vec<Crate>, reqwest::Error> {
        let notify = Arc::new(Notify::new());
        let cache_lock = self.search_cache.lock().await;

        if let Some(entry) = cache_lock.get(query) {
            match entry {
                SearchCacheEntry::Pending(existing_notify) => {
                    let cloned_notify = existing_notify.clone();
                    drop(cache_lock);
                    cloned_notify.notified().await;

                    let cache_lock = self.search_cache.lock().await;
                    if let Some(SearchCacheEntry::Ready(result)) = cache_lock.get(query) {
                        return Ok(result.clone());
                    }
                }
                SearchCacheEntry::Ready(result) => {
                    return Ok(result.clone());
                }
            }
        }

        {
            let mut cache_lock = self.search_cache.lock().await;
            cache_lock.insert(query.to_string(), SearchCacheEntry::Pending(notify.clone()));
        }

        let url = format!(
            "https://crates.io/api/v1/crates?page=1&per_page={}&q={}&sort=relevance",
            self.per_page,
            urlencoding::encode(query)
        );

        let res = self.client.get(&url).header(USER_AGENT, "zed").send().await;
        let res: Result<SearchResponse, _> = match res {
            Ok(v) => v.json().await,
            Err(e) => Err(e),
        };

        match res {
            Ok(search_response) => {
                let mut cache_lock = self.search_cache.lock().await;
                cache_lock.insert(
                    query.to_string(),
                    SearchCacheEntry::Ready(search_response.crates.clone()),
                );
                notify.notify_waiters();
                Ok(search_response.crates)
            }
            Err(e) => {
                let mut cache_lock = self.search_cache.lock().await;
                cache_lock.remove(query);
                notify.notify_waiters();
                Err(e)
            }
        }
    }

    pub async fn versions_features(
        &self,
        query: &str,
    ) -> Result<Vec<VersionExport>, reqwest::Error> {
        let notify = Arc::new(Notify::new());
        let cache_lock = self.versions_cache.lock().await;

        if let Some(entry) = cache_lock.get(query) {
            match entry {
                InfoCacheEntry::Pending(existing_notify) => {
                    let cloned_notify = existing_notify.clone();
                    drop(cache_lock);
                    cloned_notify.notified().await;

                    let cache_lock = self.versions_cache.lock().await;
                    if let Some(InfoCacheEntry::Ready(result)) = cache_lock.get(query) {
                        return Ok(result.clone());
                    }
                }
                InfoCacheEntry::Ready(result) => {
                    return Ok(result.clone());
                }
            }
        }

        {
            let mut cache_lock = self.versions_cache.lock().await;
            cache_lock.insert(query.to_string(), InfoCacheEntry::Pending(notify.clone()));
        }

        let url = format!(
            "https://crates.io/api/v1/crates?page=1&per_page={}&q={}&sort=relevance",
            self.per_page,
            urlencoding::encode(query)
        );

        let res = self.client.get(&url).header(USER_AGENT, "zed").send().await;
        let res: Result<VersionResponse, _> = match res {
            Ok(v) => v.json().await,
            Err(e) => Err(e),
        };

        match res {
            Ok(search_response) => {
                let mut cache_lock = self.versions_cache.lock().await;
                let versions = search_response.versions();
                cache_lock.insert(query.to_string(), InfoCacheEntry::Ready(versions.clone()));
                notify.notify_waiters();
                Ok(versions)
            }
            Err(e) => {
                let mut cache_lock = self.search_cache.lock().await;
                cache_lock.remove(query);
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

use crate::crate_lookup::CratesIoStorage;

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
                version: RustVersion::from(v.num.as_str()),
                features: v.features.into_iter().map(|v| v.0).collect(),
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct VersionExport {
    pub version: RustVersion,
    pub features: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RustVersion {
    major: Option<String>,
    minor: Option<String>,
    patch: Option<String>,
}

impl Display for RustVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(major) = &self.major {
            if let Some(minor) = &self.minor {
                if let Some(path) = &self.patch {
                    write!(f, "{}.{}.{}", major, minor, path)
                } else {
                    write!(f, "{}.{}", major, minor)
                }
            } else {
                write!(f, "{}", major)
            }
        } else {
            write!(f, "*")
        }
    }
}

impl From<&str> for RustVersion {
    fn from(s: &str) -> Self {
        let items = s.splitn(3, '.').collect::<Vec<_>>();
        let mut se = Self {
            major: None,
            minor: None,
            patch: None,
        };
        match items.len() {
            0 => {}
            1 => {
                se.major = Some(items[0].to_string());
            }
            2 => {
                se.major = Some(items[0].to_string());
                se.minor = Some(items[1].to_string());
            }
            3 => {
                se.major = Some(items[0].to_string());
                se.minor = Some(items[1].to_string());
                se.patch = Some(items[2].to_string());
            }
            _ => {}
        };
        se
    }
}

impl VersionExport {
    pub fn matches_version(&self, version: &str) -> bool {
        let v = RustVersion::from(version);
        self.version.eq(&v)
    }
}

impl PartialEq for RustVersion {
    fn eq(&self, other: &Self) -> bool {
        if ((other.major.is_some() && self.major == other.major) || other.major.is_none())
            && ((other.minor.is_some() && self.minor == other.minor) || other.minor.is_none())
            && ((other.patch.is_some() && self.patch == other.patch) || other.patch.is_none())
        {
            return true;
        }
        false
    }
}
impl Eq for RustVersion {}

#[derive(Serialize, Deserialize)]
pub struct Version {
    pub features: HashMap<String, Value>,
    pub num: String,
    #[serde(rename = "has_lib")]
    pub has_lib: bool,
    pub yanked: bool,
}
