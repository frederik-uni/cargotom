use std::{cmp::Ordering, collections::HashMap, fmt::Display, sync::Arc};

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
    major: Option<VersionString>,
    minor: Option<VersionString>,
    patch: Option<VersionString>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VersionString(String);

impl VersionString {
    fn int_or_string(&self) -> Vec<IntOrString> {
        let mut out = vec![];
        let mut builder = vec![];
        let mut number = false;
        let build = |number, builder: &mut Vec<char>, out: &mut Vec<IntOrString>| {
            if !builder.is_empty() {
                let val = builder.drain(..).collect::<String>();
                match number {
                    true => out.push(IntOrString::Int(val.parse().unwrap())),
                    false => out.push(IntOrString::String(val)),
                }
            }
        };
        for char in self.0.chars() {
            if number != char.is_ascii_digit() {
                build(number, &mut builder, &mut out);
                number = !number;
            }
            builder.push(char);
        }
        build(number, &mut builder, &mut out);
        out
    }
}

#[derive(PartialEq, Eq)]
enum IntOrString {
    Int(u64),
    String(String),
}

impl PartialOrd for IntOrString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntOrString {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (IntOrString::Int(a), IntOrString::Int(b)) => a.cmp(b),
            (IntOrString::Int(_), IntOrString::String(_)) => Ordering::Greater,
            (IntOrString::String(_), IntOrString::Int(_)) => Ordering::Less,
            (IntOrString::String(a), IntOrString::String(b)) => a.cmp(b),
        }
    }
}

impl Ord for VersionString {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut iter_a = self.int_or_string().into_iter();
        let mut iter_b = other.int_or_string().into_iter();
        loop {
            match (iter_a.next(), iter_b.next()) {
                (Some(elem_a), Some(elem_b)) => {
                    let cmp_result = elem_a.cmp(&elem_b);
                    if cmp_result != Ordering::Equal {
                        return cmp_result;
                    }
                }
                (Some(_), None) => return Ordering::Greater,
                (None, Some(_)) => return Ordering::Less,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

impl PartialOrd for VersionString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for VersionString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for VersionString {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl Ord for RustVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.major, &other.major) {
            (Some(self_major), Some(other_major)) => self_major.cmp(other_major),
            (Some(_), None) => Ordering::Less, // Some major < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some major
            (None, None) => Ordering::Equal,
        }
        .then_with(|| match (&self.minor, &other.minor) {
            (Some(self_minor), Some(other_minor)) => self_minor.cmp(other_minor),
            (Some(_), None) => Ordering::Less, // Some minor < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some minor
            (None, None) => Ordering::Equal,
        })
        .then_with(|| match (&self.patch, &other.patch) {
            (Some(self_patch), Some(other_patch)) => self_patch.cmp(other_patch),
            (Some(_), None) => Ordering::Less, // Some patch < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some patch
            (None, None) => Ordering::Equal,
        })
    }
}

impl PartialOrd for RustVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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
                se.major = Some(items[0].to_string().into());
            }
            2 => {
                se.major = Some(items[0].to_string().into());
                se.minor = Some(items[1].to_string().into());
            }
            3 => {
                se.major = Some(items[0].to_string().into());
                se.minor = Some(items[1].to_string().into());
                se.patch = Some(items[2].to_string().into());
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
