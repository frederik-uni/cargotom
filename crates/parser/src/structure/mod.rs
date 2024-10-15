use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use info::Info;
use lock::{CargoLockRaw, Package};
use positioned::PositionedInfo;

pub use positioned::Dependency;
pub use positioned::DependencyKind;
pub use positioned::Feature;
pub use positioned::FeatureArgKind;
pub use positioned::OptionalKey;
pub use positioned::Positioned;
pub use positioned::Source;
pub use positioned::Target;
pub use positioned::WithKey;
pub use version::RustVersion;

pub use info::Workspace;
pub use raw::CargoRawData;
pub(crate) use raw::Key;
pub(crate) use raw::RangeExclusive;
pub(crate) use raw::Tree;
pub(crate) use raw::TreeValue;
pub(crate) use raw::Value;

mod info;
mod lock;
mod positioned;
mod raw;
mod version;

/// parsed information of the Cargo.toml file
#[derive(Default, Debug)]
pub struct Cargo {
    /// Path to the Cargo.toml file
    pub path: PathBuf,
    pub lock_file_path: Option<PathBuf>,
    /// File info with no positional information
    pub info: Info,
    /// Data used by the language server with positional information
    pub positioned_info: PositionedInfo,
}

impl Cargo {
    pub(crate) fn clean(&mut self) {
        match &mut self.info.workspace {
            info::Workspace::Workspace { members } => {
                members.drain(..);
            }
            _ => {}
        }
        self.positioned_info.dependencies = Default::default();
        self.positioned_info.features = Default::default();
    }

    pub fn new(path: PathBuf, lock_file_path: &mut Option<PathBuf>, workspace: Workspace) -> Self {
        if lock_file_path.is_none() {
            if let Some(lock_file) = find_lock_file(&path) {
                *lock_file_path = Some(lock_file);
            }
        }

        Self {
            path,
            lock_file_path: lock_file_path.clone(),
            info: Info { workspace },
            positioned_info: Default::default(),
        }
    }

    pub fn get_members(&self) -> Vec<PathBuf> {
        match &self.info.workspace {
            info::Workspace::Workspace { members } => {
                members.iter().map(|v| self.path.join(v)).collect()
            }
            _ => vec![],
        }
    }
}

fn find_lock_file(start_path: &Path) -> Option<PathBuf> {
    let mut current_dir = start_path.parent();
    while let Some(dir) = current_dir {
        let lock_file = dir.join("Cargo.lock");
        if lock_file.exists() {
            return Some(lock_file);
        }
        current_dir = dir.parent();
    }
    None
}

/// parsed information of the Cargo.lock file
pub struct Lock {
    raw_data: CargoLockRaw,
    raw_content: String,
}

impl Lock {
    pub fn new(content: String) -> Option<Self> {
        Some(Self {
            raw_data: toml::from_str(&content).ok()?,
            raw_content: content,
        })
    }
    pub fn reload(&mut self) {
        if let Ok(content) = toml::from_str(&self.raw_content) {
            self.raw_data = content;
        }
    }

    pub fn text(&self) -> &str {
        &self.raw_content
    }

    pub fn text_mut(&mut self) -> &mut String {
        &mut self.raw_content
    }
}

impl Lock {
    /// gets the packages from the Cargo.lock file
    pub fn packages(&self) -> HashMap<String, Vec<Package>> {
        self.raw_data.packages()
    }
}
