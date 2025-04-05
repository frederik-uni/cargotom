pub mod config;

use std::sync::Arc;

use tokio::sync::RwLock;

pub type Shared<T> = Arc<RwLock<T>>;

pub fn shared<T>(t: T) -> Shared<T> {
    Arc::new(RwLock::new(t))
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
