use std::{path::PathBuf, sync::Arc, time::Duration};

use tokio::{sync::RwLock, time::sleep};

use crate::downloader::download_update;

pub fn init(offline: Arc<RwLock<bool>>, data: Arc<RwLock<Vec<OfflineCrate>>>) {
    //init with old
    updater(offline);
}

fn read_data(file: PathBuf) {
    todo!()
}

pub fn updater(offline: Arc<RwLock<bool>>) {
    tokio::spawn(async move {
        loop {
            if !*offline.read().await {
                break;
            }
            if download_update().await.unwrap_or_default() {
                // Handle update success
            }
            sleep(Duration::from_secs(3600)).await
        }
    });
}

#[derive(Debug)]
pub struct OfflineCrate {
    name: String,
    repository: Option<String>,
    homepage: Option<String>,
    documentation: Option<String>,
    description: String,
    latest_stable_version: Option<String>,
    latest_version: Option<String>,
    categories: Vec<u32>,
    keywords: Vec<u32>,
    num_versions: u32,
    order: u32,
}

macro_rules! read_u32 {
    ($data:expr, $cursor:expr) => {{
        let value = u32::from_le_bytes($data[$cursor..$cursor + 4].try_into().unwrap());
        $cursor += 4;
        value
    }};
}

macro_rules! read_string {
    ($data:expr, $cursor:expr) => {{
        let len = read_u32!($data, $cursor);
        let s = String::from_utf8($data[$cursor..$cursor + len as usize].to_vec()).unwrap();
        $cursor += len as usize;
        s
    }};
}

impl OfflineCrate {
    pub fn from_vec(data: Vec<u8>) -> Self {
        let mut cursor = 0;

        let order = read_u32!(data, cursor);
        let num_versions = read_u32!(data, cursor);

        let keywords_len = read_u32!(data, cursor) as usize;
        let mut keywords = Vec::with_capacity(keywords_len);
        for _ in 0..keywords_len {
            keywords.push(read_u32!(data, cursor));
        }

        let categories_len = read_u32!(data, cursor) as usize;
        let mut categories = Vec::with_capacity(categories_len);
        for _ in 0..categories_len {
            categories.push(read_u32!(data, cursor));
        }

        let name = read_string!(data, cursor);
        let description = read_string!(data, cursor);
        let repository = if !data[cursor..].is_empty() {
            let str = read_string!(data, cursor);
            match str.len() == 0 {
                true => None,
                false => Some(str),
            }
        } else {
            unreachable!()
        };
        let homepage = if !data[cursor..].is_empty() {
            let str = read_string!(data, cursor);
            match str.len() == 0 {
                true => None,
                false => Some(str),
            }
        } else {
            unreachable!()
        };
        let documentation = if !data[cursor..].is_empty() {
            let str = read_string!(data, cursor);
            match str.len() == 0 {
                true => None,
                false => Some(str),
            }
        } else {
            unreachable!();
        };
        let latest_stable_version = if !data[cursor..].is_empty() {
            let str = read_string!(data, cursor);
            match str.len() == 0 {
                true => None,
                false => Some(str),
            }
        } else {
            unreachable!()
        };
        let latest_version = if !data[cursor..].is_empty() {
            #[allow(unused_assignments)]
            let str = read_string!(data, cursor);
            match str.len() == 0 {
                true => None,
                false => Some(str),
            }
        } else {
            unreachable!()
        };

        Self {
            order,
            num_versions,
            keywords,
            categories,
            name,
            description,
            repository,
            homepage,
            documentation,
            latest_stable_version,
            latest_version,
        }
    }
}
