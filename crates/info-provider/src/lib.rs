use std::{collections::HashMap, path::PathBuf, sync::Arc};

use api::{CacheItem, CacheItemOut, Crate, Root1};
use local::OfflineCrate;
use tokio::sync::RwLock;

pub mod api;
mod downloader;
mod local;
pub struct InfoProvider {
    client: Arc<reqwest::Client>,
    registry: &'static str,
    readme_cache: Arc<RwLock<HashMap<(String, String), CacheItem<String>>>>,
    info_cache: Arc<RwLock<HashMap<String, HashMap<String, CacheItem<Root1>>>>>,
    search_cache: Arc<RwLock<HashMap<String, CacheItem<Crate>>>>,
    per_page: RwLock<usize>,
    offline: Arc<RwLock<bool>>,
    data: Arc<RwLock<Vec<OfflineCrate>>>,
    root: PathBuf,
}

impl InfoProvider {
    pub async fn new(per_page: usize, offline: bool, data_path: PathBuf) -> Self {
        let off = Arc::new(RwLock::new(offline));
        let off_data = Arc::new(RwLock::new(Vec::new()));
        if offline {
            local::init(off.clone(), off_data.clone(), data_path.clone()).await;
        }
        Self {
            root: data_path,
            data: off_data,
            offline: off,
            client: Arc::new(reqwest::Client::new()),
            registry: "https://index.crates.io/",
            info_cache: Default::default(),
            search_cache: Default::default(),
            readme_cache: Default::default(),
            per_page: RwLock::new(per_page),
        }
    }

    pub async fn set_per_page(&self, per_page: usize) {
        *self.per_page.write().await = per_page;
        self.search_cache.write().await.drain();
    }

    pub async fn set_offline(&self, offline: bool) {
        let old_offline = *self.offline.read().await;
        match (old_offline, offline) {
            (true, false) => {
                *self.offline.write().await = false;
                self.data.write().await.clear();
            }
            (false, true) => {
                *self.offline.write().await = true;
                local::init(self.offline.clone(), self.data.clone(), self.root.clone()).await;
            }
            _ => {}
        }
    }

    pub async fn get_info_cache(&self, registry: Option<&str>, name: &str) -> CacheItemOut<Root1> {
        self.get_info_cache_api(registry, name).await
    }

    pub async fn get_info(&self, registry: Option<&str>, name: &str) -> Result<Vec<Root1>, String> {
        self.get_info_api(registry, name).await
    }

    pub async fn search(&self, name: &str) -> Result<Vec<Crate>, anyhow::Error> {
        let offline = *self.offline.read().await;
        if offline {
            let len = self.data.read().await.len();
            if len != 0 {
                return Ok(self.search_local(name).await);
            }
        }
        self.search_api(name).await
    }

    pub async fn get_crate_repository(&self, crate_name: &str) -> Option<String> {
        self.get_crate_repository_api(crate_name).await
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use tokio::time;

    use crate::InfoProvider;

    #[tokio::test]
    async fn offline() {
        let provider = InfoProvider::new(10, true, PathBuf::from("data")).await;
        println!(
            "{:#?}",
            provider
                .data
                .read()
                .await
                .iter()
                .max_by(|a, b| a.order.cmp(&b.order))
        );
        time::sleep(Duration::from_secs(20)).await;
        println!("{}", provider.data.read().await.len());
    }
}
