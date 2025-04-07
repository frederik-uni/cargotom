use std::{
    collections::HashMap,
    fs::{read_to_string, File},
    io::{self, BufReader, Read as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use byteorder::{LittleEndian, ReadBytesExt};

use tokio::{sync::RwLock, time::sleep};

use crate::{api::Crate, downloader::download_update, InfoProvider};

impl InfoProvider {
    pub async fn search_local(&self, name: &str) -> Vec<Crate> {
        let lock = self.data.read().await;
        let mut v: Vec<_> = lock
            .iter()
            .filter(|v| v.name.starts_with(name))
            .collect::<Vec<_>>();
        v.sort_by(|a, b| b.order.cmp(&a.order));
        v.into_iter()
            .map(|v| Crate {
                exact_match: v.name == name,
                name: v.name.clone(),
                description: v.description.clone(),
                max_stable_version: v.latest_stable_version.clone(),
                max_version: v.latest_version.clone().or(v.latest_stable_version.clone()),
            })
            .collect()
    }
}

pub async fn init(offline: Arc<RwLock<bool>>, data: Arc<RwLock<Vec<OfflineCrate>>>, root: PathBuf) {
    {
        if let Some(v) = read_data(&root.join("offline")) {
            *data.write().await = v;
        }
    }
    updater(offline, root.join("offline"), data);
}

fn read_data(root: &Path) -> Option<Vec<OfflineCrate>> {
    let current = read_to_string(root.join("current")).ok()?;
    let path = root.join(current);
    if path.is_dir() {
        let gen_map = |file: File| {
            let mut reader = BufReader::new(file);

            let mut map = HashMap::new();
            loop {
                let id = match reader.read_u32::<LittleEndian>() {
                    Ok(n) => n,
                    Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                    Err(_) => return None,
                };
                let len = match reader.read_u32::<LittleEndian>() {
                    Ok(n) => n as usize,
                    Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                    Err(_) => return None,
                };

                let mut buffer = vec![0; len];
                reader.read_exact(&mut buffer).ok()?;
                let str = String::from_utf8_lossy(&buffer);
                map.insert(id, Arc::new(str.to_string()));
            }
            Some(map)
        };
        let categories = gen_map(File::open(path.join("categories")).ok()?)?;

        let keywords = gen_map(File::open(path.join("keywords")).ok()?)?;
        let dump = File::open(path.join("dump")).ok()?;
        let mut reader = BufReader::new(dump);

        let mut res = vec![];
        loop {
            let len = match reader.read_u32::<LittleEndian>() {
                Ok(n) => n as usize,
                Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(_) => return None,
            };

            let mut buffer = vec![0; len];
            reader.read_exact(&mut buffer).ok()?;
            res.push(OfflineCrate::from_vec(buffer, &keywords, &categories))
        }
        return Some(res);
    }
    None
}

pub fn updater(offline: Arc<RwLock<bool>>, root: PathBuf, data: Arc<RwLock<Vec<OfflineCrate>>>) {
    tokio::spawn(async move {
        loop {
            if !*offline.read().await {
                break;
            }
            if download_update(&root).await.unwrap_or_default() {
                if let Some(v) = read_data(&root) {
                    *data.write().await = v;
                }
            }
            sleep(Duration::from_secs(3600)).await
        }
    });
}

#[derive(Debug)]
pub struct OfflineCrate {
    pub name: String,
    pub repository: Option<String>,
    pub homepage: Option<String>,
    pub documentation: Option<String>,
    pub description: Option<String>,
    pub latest_stable_version: Option<String>,
    pub latest_version: Option<String>,
    pub categories: Vec<Arc<String>>,
    pub keywords: Vec<Arc<String>>,
    pub num_versions: u32,
    pub order: u32,
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
    pub fn from_vec(
        data: Vec<u8>,
        keywords_map: &HashMap<u32, Arc<String>>,
        categories_map: &HashMap<u32, Arc<String>>,
    ) -> Self {
        let mut cursor = 0;

        let order = read_u32!(data, cursor);
        let num_versions = read_u32!(data, cursor);

        let keywords_len = read_u32!(data, cursor) as usize;
        let mut keywords = Vec::with_capacity(keywords_len);
        for _ in 0..keywords_len {
            let id = read_u32!(data, cursor);
            if let Some(v) = keywords_map.get(&id) {
                keywords.push(v.clone());
            }
        }

        let categories_len = read_u32!(data, cursor) as usize;
        let mut categories = Vec::with_capacity(categories_len);
        for _ in 0..categories_len {
            let id = read_u32!(data, cursor);
            if let Some(v) = categories_map.get(&id) {
                categories.push(v.clone());
            }
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
            description: match description.is_empty() {
                true => None,
                false => Some(description),
            },
            repository,
            homepage,
            documentation,
            latest_stable_version,
            latest_version,
        }
    }
}
