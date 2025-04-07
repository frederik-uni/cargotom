use std::{collections::HashMap, sync::Arc};

use api::{CacheItem, Crate, Root1};
use local::OfflineCrate;
use tokio::sync::{Mutex, Notify, RwLock};

pub mod api;
mod downloader;
mod local;
pub struct InfoProvider {
    client: Arc<reqwest::Client>,
    registry: &'static str,
    info_cache: Arc<Mutex<HashMap<String, HashMap<String, CacheItem<Root1>>>>>,
    search_cache: Arc<Mutex<HashMap<String, CacheItem<Crate>>>>,
    per_page: RwLock<usize>,
    offline: Arc<RwLock<bool>>,
    data: Arc<RwLock<Vec<OfflineCrate>>>,
}

impl InfoProvider {
    pub fn new(per_page: usize, offline: bool) -> Self {
        let off = Arc::new(RwLock::new(offline));
        let off_data = Arc::new(RwLock::new(Vec::new()));
        if offline {
            local::init(off.clone(), off_data.clone());
        }
        Self {
            data: off_data,
            offline: off,
            client: Arc::new(reqwest::Client::new()),
            registry: "https://index.crates.io/",
            info_cache: Default::default(),
            search_cache: Default::default(),
            per_page: RwLock::new(per_page),
        }
    }
}
