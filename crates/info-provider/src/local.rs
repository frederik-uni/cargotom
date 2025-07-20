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
        let mut v = search(name, &lock.0, &lock.1);
        v.sort_by(|a, b| b.order.cmp(&a.order));
        v.into_iter().map(|v| v.as_crate(v.name == name)).collect()
    }

    pub async fn get_local(&self, name: &str) -> Option<Arc<OfflineCrate>> {
        let lock = self.data.read().await;
        lock.2.iter().find(|v| v.name == name).cloned()
    }
}

pub async fn init(
    offline: Arc<RwLock<bool>>,
    data: Arc<
        RwLock<(
            Set<Vec<u8>>,
            HashMap<String, Vec<Arc<OfflineCrate>>>,
            Vec<Arc<OfflineCrate>>,
        )>,
    >,
    root: PathBuf,
) {
    {
        if let Some(v) = read_data(&root.join("offline")) {
            *data.write().await = v;
        }
    }
    updater(offline, root.join("offline"), data);
}

use fst::{IntoStreamer, Set, SetBuilder, Streamer as _};
use std::collections::HashSet;

fn tokenize(name: &str) -> Vec<String> {
    name.split(|c| c == '-' || c == '_')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn build_index(
    crate_list: Vec<Arc<OfflineCrate>>,
) -> (Set<Vec<u8>>, HashMap<String, Vec<Arc<OfflineCrate>>>) {
    let mut token_to_crates: HashMap<String, Vec<Arc<OfflineCrate>>> = HashMap::new();
    let mut unique_tokens = HashSet::new();

    for krate_arc in crate_list {
        let tokens = tokenize(&krate_arc.name);
        for token in tokens {
            let token = token.to_lowercase();
            for i in 1..=token.len() {
                let prefix = &token[..i];
                token_to_crates
                    .entry(prefix.to_string())
                    .or_default()
                    .push(Arc::clone(&krate_arc));
                unique_tokens.insert(prefix.to_string());
            }
        }
    }

    let mut token_list: Vec<String> = unique_tokens.into_iter().collect();
    token_list.sort();

    let buffer = {
        let mut builder = SetBuilder::memory();
        for token in &token_list {
            builder.insert(token).unwrap();
        }
        builder.into_inner().unwrap()
    };

    let set = Set::new(buffer).unwrap();
    (set, token_to_crates)
}

fn search(
    query: &str,
    fst_set: &Set<Vec<u8>>,
    token_map: &HashMap<String, Vec<Arc<OfflineCrate>>>,
) -> Vec<Arc<OfflineCrate>> {
    let query_tokens = tokenize(query);
    if query_tokens.is_empty() {
        return vec![];
    }

    let mut matching_sets: Vec<HashSet<Arc<OfflineCrate>>> = Vec::new();

    for token in query_tokens {
        let mut current_set = HashSet::new();
        let mut stream = fst_set.range().ge(&token).into_stream();

        while let Some(token_bytes) = stream.next() {
            let prefix = String::from_utf8_lossy(&token_bytes);
            if !prefix.starts_with(&token) {
                break;
            }

            if let Some(crates) = token_map.get(prefix.as_ref()) {
                for krate in crates {
                    current_set.insert(Arc::clone(krate));
                }
            }
        }

        if current_set.is_empty() {
            return vec![]; // Early exit: no matches for one of the tokens
        }

        matching_sets.push(current_set);
    }

    let mut iter = matching_sets.into_iter();
    let first = iter.next().unwrap();
    let intersection = iter.fold(first, |acc, set| acc.intersection(&set).cloned().collect());

    let mut results: Vec<_> = intersection.into_iter().collect();
    results.sort_by_key(|krate| krate.name.clone());
    results
}

fn read_data(
    root: &Path,
) -> Option<(
    Set<Vec<u8>>,
    HashMap<String, Vec<Arc<OfflineCrate>>>,
    Vec<Arc<OfflineCrate>>,
)> {
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
        let items = res.into_iter().map(Arc::new).collect::<Vec<_>>();
        let indexed = build_index(items.clone());
        return Some((indexed.0, indexed.1, items));
    }
    None
}

pub fn updater(
    offline: Arc<RwLock<bool>>,
    root: PathBuf,
    data: Arc<
        RwLock<(
            Set<Vec<u8>>,
            HashMap<String, Vec<Arc<OfflineCrate>>>,
            Vec<Arc<OfflineCrate>>,
        )>,
    >,
) {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

    pub fn as_crate(&self, exact_match: bool) -> Crate {
        Crate {
            exact_match,
            name: self.name.clone(),
            description: self.description.clone(),
            homepage: self.homepage.clone(),
            documentation: self.documentation.clone(),
            repository: self.repository.clone(),
            max_stable_version: self.latest_stable_version.clone(),
            max_version: self
                .latest_version
                .as_ref()
                .or(self.latest_stable_version.as_ref())
                .cloned(),
        }
    }
}
