use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use reqwest::{header::USER_AGENT, Client};
use rust_version::RustVersion;
use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, RwLock};

use crate::InfoProvider;

pub enum CacheItem<T> {
    Pending(Arc<Notify>),
    Error(String),
    Ready(Vec<T>),
}

pub enum CacheItemOut<T> {
    NotStarted,
    Pending,
    Error(String),
    Ready(Vec<T>),
}

impl InfoProvider {
    pub async fn set_per_page(&self, per_page: usize) {
        *self.per_page.write().await = per_page;
        self.search_cache.lock().await.drain();
    }

    pub async fn get_crate_repository(&self, crate_name: &str) -> Option<String> {
        let url = format!("https://crates.io/api/v1/crates/{}", crate_name);

        let response = self
            .client
            .get(&url)
            .header(USER_AGENT, "zed")
            .send()
            .await
            .ok()?;
        let json: serde_json::Value = response.json().await.ok()?;
        json["crate"]["repository"].as_str().map(|s| s.to_string())
    }

    pub async fn get_info_cache(&self, registry: Option<&str>, name: &str) -> CacheItemOut<Root1> {
        let reg = registry.unwrap_or(self.registry);
        let mut lock = self.info_cache.lock().await;
        let cache = lock.entry(reg.to_owned()).or_default();
        match cache.get(name) {
            None => CacheItemOut::NotStarted,
            Some(CacheItem::Pending(_)) => CacheItemOut::Pending,
            Some(CacheItem::Ready(items)) => CacheItemOut::Ready(items.clone()),
            Some(CacheItem::Error(err)) => CacheItemOut::Error(err.clone()),
        }
    }

    pub async fn search(&self, name: &str) -> Result<Vec<Crate>, anyhow::Error> {
        let fetch = {
            let lock = self.search_cache.lock().await;
            match lock.get(name) {
                Some(v) => match v {
                    CacheItem::Pending(n) => Some(n.clone()),
                    CacheItem::Ready(items) => return Ok(items.clone()),
                    CacheItem::Error(_) => unreachable!(),
                },
                None => None,
            }
        };
        let n = match fetch {
            Some(n) => n,
            None => {
                let n = Arc::new(Notify::new());
                let notify = n.clone();
                {
                    let mut lock = self.search_cache.lock().await;
                    lock.insert(name.to_owned(), CacheItem::Pending(notify.clone()));
                }
                let client = self.client.clone();
                let per_page = *self.per_page.read().await;
                let name = name.to_owned();
                let search_cache = self.search_cache.clone();
                tokio::spawn(async move {
                    let info = search(&client, per_page, &name).await;
                    let mut lock = search_cache.lock().await;
                    match &info {
                        Ok(items) => {
                            lock.insert(name, CacheItem::Ready(items.clone()));
                        }
                        Err(_) => {
                            lock.remove(&name);
                        }
                    }
                    notify.notify_waiters();
                });

                n
            }
        };
        n.notified().await;
        let lock = self.search_cache.lock().await;
        match lock.get(name) {
            Some(v) => match v {
                CacheItem::Pending(_) => return Ok(vec![]),
                CacheItem::Ready(items) => return Ok(items.clone()),
                CacheItem::Error(_) => unreachable!(),
            },
            None => return Ok(vec![]),
        }
    }

    pub async fn get_info(&self, registry: Option<&str>, name: &str) -> Result<Vec<Root1>, String> {
        let reg = registry.unwrap_or(self.registry);
        let fetch = {
            let mut lock = self.info_cache.lock().await;
            let cache = lock.entry(reg.to_owned()).or_default();
            match cache.get(name) {
                Some(v) => match v {
                    CacheItem::Pending(n) => Some(n.clone()),
                    CacheItem::Ready(items) => return Ok(items.clone()),
                    CacheItem::Error(e) => return Err(e.clone()),
                },
                None => None,
            }
        };
        let n = match fetch {
            Some(n) => n,
            None => {
                let n = Arc::new(Notify::new());
                let notify = n.clone();
                let reg = reg.to_owned();
                let name = name.to_owned();
                let info_cache = self.info_cache.clone();
                let client = self.client.clone();
                {
                    let mut lock = self.info_cache.lock().await;
                    let cache = lock.entry(reg.to_owned()).or_default();
                    cache.insert(name.to_owned(), CacheItem::Pending(notify.clone()));
                }
                tokio::spawn(async move {
                    let info = info(&client, &reg, &name).await;

                    let mut lock = info_cache.lock().await;
                    let cache = lock.entry(reg).or_default();
                    match &info {
                        Ok(v) => {
                            cache.insert(name.to_owned(), CacheItem::Ready(v.clone()));
                        }
                        Err(e) => {
                            cache.insert(name.to_owned(), CacheItem::Error(e.to_string()));
                        }
                    };

                    notify.notify_waiters();
                });
                n
            }
        };

        n.notified().await;
        let mut lock = self.info_cache.lock().await;
        let cache = lock.entry(reg.to_owned()).or_default();
        match cache.get(name) {
            Some(v) => match v {
                CacheItem::Pending(_) => return Ok(vec![]),
                CacheItem::Ready(items) => return Ok(items.clone()),
                CacheItem::Error(e) => return Err(e.clone()),
            },
            None => return Ok(vec![]),
        }
    }
}

async fn search(client: &Client, per_page: usize, name: &str) -> Result<Vec<Crate>, anyhow::Error> {
    let url = format!(
        "https://crates.io/api/v1/crates?page=1&per_page={}&q={}&sort=relevance",
        per_page,
        urlencoding::encode(name)
    );
    let res = client.get(&url).header(USER_AGENT, "zed").send().await;
    let res: Result<SearchResponse, _> = match res {
        Ok(v) => v.json().await,
        Err(e) => Err(e),
    };
    Ok(res?.crates)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Crate {
    pub exact_match: bool,
    pub name: String,
    pub description: Option<String>,
    pub max_stable_version: Option<String>,
    pub max_version: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SearchResponse {
    crates: Vec<Crate>,
}

async fn info(
    client: &reqwest::Client,
    registry: &str,
    name: &str,
) -> Result<Vec<Root1>, anyhow::Error> {
    let mut registry = registry.to_string();
    if !registry.ends_with("/") {
        registry.push('/');
    }
    let url = format!(
        "{}{}",
        registry,
        match name.len() {
            0 => return Ok(vec![]),
            1 => format!("1/{name}"),
            2 => format!("2/{}", name),
            3 => format!("3/{}/{}", &name[0..1], name),
            _ => format!("{}/{}/{}", &name[0..2], &name[2..4], name),
        }
    );
    let data = client
        .get(url)
        .header(USER_AGENT, "zed")
        .send()
        .await?
        .text()
        .await?
        .lines()
        .map(serde_json::from_str)
        .collect::<Result<Vec<Root1>, _>>()?;
    Ok(data.into_iter().filter(|v| !v.yanked).collect())
}

#[derive(Deserialize, Clone)]
pub struct Deps1 {
    pub name: String,
    pub req: String,
    pub optional: bool,
}

#[derive(Deserialize, Clone)]
pub struct Root1 {
    pub name: String,
    pub vers: String,
    yanked: bool,
    pub deps: Vec<Deps1>,
    pub features: HashMap<String, Vec<String>>,
    pub features2: Option<HashMap<String, Vec<String>>>,
}

#[derive(Eq, PartialEq, Serialize, Deserialize, Clone, Copy)]
pub enum ViewMode {
    All,
    UnusedOpt,
    Features,
}

fn name_processor(s: &str) -> String {
    if s.contains("?") {
        let cr = s.split_once("?").map(|v| v.0).unwrap();
        if cr.starts_with("dep:") {
            cr.to_owned()
        } else {
            format!("dep:{cr}")
        }
    } else if s.contains("/") {
        let cr = s.split_once("/").map(|v| v.0).unwrap();
        if cr.starts_with("dep:") {
            cr.to_owned()
        } else {
            format!("dep:{cr}")
        }
    } else {
        s.to_string()
    }
}

impl Root1 {
    pub fn feature_all(&self) -> Vec<String> {
        let f = self.features.keys().cloned();
        let mut opt: Vec<_> = self
            .deps
            .iter()
            .filter(|v| v.optional)
            .map(|v| v.name.clone())
            .collect();
        opt.extend(f);
        if let Some(v) = self.features2.as_ref().map(|v| v.keys()) {
            opt.extend(v.cloned());
        }
        opt
    }
    pub fn features(&self, view_mode: ViewMode) -> Vec<String> {
        let mut values = self
            .features
            .values()
            .flatten()
            .map(|v| format!("dep:{v}"))
            .collect::<HashSet<_>>();
        values.extend(
            self.features2
                .as_ref()
                .map(|v| {
                    v.values()
                        .flatten()
                        .map(|v| name_processor(v))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let mut features = self
            .features
            .iter()
            .map(|v| match v.1.is_empty() {
                true => v.0.to_owned(),
                false => format!("{}: [{}]", v.0, v.1.join(", ")),
            })
            .collect::<BTreeSet<_>>();
        features.extend(
            self.features2
                .as_ref()
                .map(|v| {
                    v.iter()
                        .map(|v| match v.1.is_empty() {
                            true => v.0.to_owned(),
                            false => format!("{}: [{}]", v.0, v.1.join(", ")),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        if view_mode != ViewMode::Features {
            let opt: Vec<_> = self
                .deps
                .iter()
                .filter(|v| v.optional)
                .map(|v| v.name.clone())
                .collect();
            if view_mode == ViewMode::UnusedOpt {
                for opt in opt {
                    if values.get(&format!("dep:{opt}")).is_none() {
                        features.insert(format!(r#"{opt}*"#));
                    }
                }
            } else {
                for opt in opt {
                    features.insert(format!(r#"{opt}*"#));
                }
            }
        }
        features.into_iter().collect()
    }
    pub fn ver(&self) -> Option<RustVersion> {
        RustVersion::try_from(self.vers.as_str()).ok()
    }
}
