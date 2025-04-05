use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use parser::structs::version::RustVersion;
use reqwest::{header::USER_AGENT, Client};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Notify};
use tower_lsp::lsp_types::MessageType;

pub enum CacheItem<T> {
    Pending(Arc<Notify>),
    Ready(Vec<T>),
}

pub struct InfoProvider {
    client: Arc<reqwest::Client>,
    registry: &'static str,
    info_cache: Arc<Mutex<HashMap<String, HashMap<String, CacheItem<Root1>>>>>,
    search_cache: Arc<Mutex<HashMap<String, CacheItem<Crate>>>>,
    per_page: usize,
}

impl InfoProvider {
    pub fn new(per_page: usize) -> Self {
        Self {
            client: Arc::new(reqwest::Client::new()),
            registry: "https://index.crates.io/",
            info_cache: Default::default(),
            search_cache: Default::default(),
            per_page,
        }
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

    pub async fn get_info_cache(&self, registry: Option<&str>, name: &str) -> Option<Vec<Root1>> {
        let reg = registry.unwrap_or(self.registry);
        let mut lock = self.info_cache.lock().await;
        let cache = lock.entry(reg.to_owned()).or_default();
        match cache.get(name)? {
            CacheItem::Pending(_) => None,
            CacheItem::Ready(items) => Some(items.clone()),
        }
    }

    pub async fn search(&self, name: &str) -> Result<Vec<Crate>, anyhow::Error> {
        let fetch = {
            let lock = self.search_cache.lock().await;
            match lock.get(name) {
                Some(v) => match v {
                    CacheItem::Pending(n) => Some(n.clone()),
                    CacheItem::Ready(items) => return Ok(items.clone()),
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
                let per_page = self.per_page;
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
            },
            None => return Ok(vec![]),
        }
    }

    pub async fn get_info(
        &self,
        registry: Option<&str>,
        name: &str,
    ) -> Result<Vec<Root1>, anyhow::Error> {
        let reg = registry.unwrap_or(self.registry);
        let fetch = {
            let mut lock = self.info_cache.lock().await;
            let cache = lock.entry(reg.to_owned()).or_default();
            match cache.get(name) {
                Some(v) => match v {
                    CacheItem::Pending(n) => Some(n.clone()),
                    CacheItem::Ready(items) => return Ok(items.clone()),
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
                        Err(_) => {
                            cache.remove(&name);
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

#[derive(Eq, PartialEq)]
pub enum ViewMode {
    All,
    UnusedOpt,
    Features,
}

impl Root1 {
    pub fn features(&self, view_mode: ViewMode) -> Vec<String> {
        let values = self.features.values().flatten().collect::<HashSet<_>>();
        let mut features = self
            .features
            .iter()
            .map(|v| match v.1.is_empty() {
                true => v.0.to_owned(),
                false => format!("{}: [{}]", v.0, v.1.join(", ")),
            })
            .collect::<BTreeSet<_>>();
        if view_mode != ViewMode::Features {
            let opt: Vec<_> = self
                .deps
                .iter()
                .filter(|v| v.optional)
                .map(|v| v.name.clone())
                .collect();
            if view_mode == ViewMode::UnusedOpt {
                for opt in opt {
                    if values.get(&opt).is_none() {
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
        parser::structs::version::RustVersion::try_from(self.vers.as_str()).ok()
    }
}
