use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub struct Config {
    #[serde(default = "true_default")]
    pub offline: bool,
    #[serde(default = "true_default")]
    pub stable: bool,
    #[serde(default = "per_page_web_default")]
    pub per_page_web: u32,
    #[serde(default = "true_default")]
    pub daemon: bool,
    #[serde(default = "daemon_port_default")]
    pub daemon_port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            offline: true,
            stable: true,
            per_page_web: 25,
            daemon: true,
            daemon_port: 54219,
        }
    }
}

fn true_default() -> bool {
    true
}
fn per_page_web_default() -> u32 {
    25
}
fn daemon_port_default() -> u16 {
    54219
}
