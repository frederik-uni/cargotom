use std::{collections::HashMap, path::PathBuf};

use crate_info::shared::CrateLookUp;
use parser::{
    structure::{CargoRawData, Dependency, Feature, Lock, Positioned, RustVersion},
    Cargo,
};
use tower_lsp::{
    lsp_types::{DidChangeTextDocumentParams, Position, Range, Url},
    Client,
};
use util::Shared;

use crate::util::get_byte_index_from_position;

pub struct Context {
    pub crates: Shared<CrateLookUp>,
    pub client: Client,
    pub workspace_root: Shared<Option<PathBuf>>,
    pub path: PathBuf,
    pub hide_docs_info_message: Shared<bool>,
    pub toml_store: Shared<HashMap<String, Toml>>,
}

impl Context {
    pub fn shoud_allow_user(&self, uri: &Url) -> bool {
        let uri = uri.to_string();
        uri.ends_with("/Cargo.toml")
    }
}

pub enum Toml {
    Cargo { cargo: Cargo, raw: CargoRawData },
    Lock(Lock),
}

impl Toml {
    pub fn to_range<T>(&self, pos: &Positioned<T>) -> Range {
        let start = self.byte_offset_to_position(pos.start);
        let end = self.byte_offset_to_position(pos.end);
        Range { start, end }
    }

    pub fn as_cargo(&self) -> Option<&Cargo> {
        match self {
            Toml::Cargo { cargo, .. } => Some(cargo),
            _ => None,
        }
    }

    pub fn as_lock(&self) -> Option<&Lock> {
        match self {
            Toml::Lock(lock) => Some(lock),
            _ => None,
        }
    }

    pub fn text_mut(&mut self) -> &mut String {
        match self {
            Toml::Cargo { raw, .. } => raw.text_mut(),
            Toml::Lock(lock) => lock.text_mut(),
        }
    }

    pub async fn needs_update(
        &self,
        lookup: &Shared<CrateLookUp>,
    ) -> Option<Vec<(&Positioned<String>, String)>> {
        let mut updates = vec![];
        let items = self
            .as_cargo()?
            .positioned_info
            .dependencies
            .iter()
            .filter_map(|v| {
                v.data
                    .source
                    .version()
                    .map(|version| (&v.data.name, version))
            })
            .collect::<Vec<_>>();
        for (crate_name, version) in items {
            let crate_version = match RustVersion::try_from(version.data.as_str()) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let mut versions = lookup
                .read()
                .await
                .get_version_local(&crate_name.data)
                .await
                .unwrap_or_default()
                .into_iter()
                .filter(|v| v > &crate_version)
                .collect::<Vec<_>>();
            versions.sort();
            if let Some(v) = versions.pop() {
                updates.push((crate_name, v.to_string()));
            }
        }
        Some(updates)
    }

    pub async fn update(&mut self, params: DidChangeTextDocumentParams) {
        for change in params.content_changes {
            if let Some(range) = change.range {
                let start = get_byte_index_from_position(self.text(), range.start);
                let end = get_byte_index_from_position(self.text(), range.end);

                self.text_mut().replace_range(start..end, &change.text);
            } else {
                self.text_mut().clone_from(&change.text)
            }
        }
        //TODO: dont parse whole toml every time
        self.reload();
    }

    pub fn get_members(&self) -> Vec<PathBuf> {
        match self {
            Toml::Cargo { cargo, .. } => cargo.get_members(),
            Toml::Lock(_) => vec![],
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Toml::Cargo { raw, .. } => raw.text(),
            Toml::Lock(lock) => lock.text(),
        }
    }

    pub fn reload(&mut self) -> Option<PathBuf> {
        match self {
            Toml::Cargo { cargo, raw } => raw.reload(cargo),
            Toml::Lock(lock) => {
                lock.reload();
                None
            }
        }
    }

    pub fn get_dependency(&self, byte_offset: u32) -> Option<&Positioned<Dependency>> {
        match self {
            Toml::Cargo { cargo, .. } => cargo
                .positioned_info
                .dependencies
                .iter()
                .find(|v| v.contains_inclusive(byte_offset)),
            _ => None,
        }
    }

    pub fn get_feature(&self, byte_offset: u32) -> Option<&Positioned<Feature>> {
        match self {
            Toml::Cargo { cargo, .. } => cargo
                .positioned_info
                .features
                .iter()
                .find(|v| v.contains_inclusive(byte_offset)),
            _ => None,
        }
    }

    pub fn get_dependency_by_range(&self, start: u32, end: u32) -> Option<&Positioned<Dependency>> {
        match self {
            Toml::Cargo { cargo, .. } => cargo
                .positioned_info
                .dependencies
                .iter()
                .find(|v| v.is_in_range(start, end)),
            _ => None,
        }
    }

    pub fn byte_offset_to_position(&self, byte_offset: u32) -> Position {
        let byte_offset = byte_offset as usize;

        let content_slice = &self.text()[..byte_offset];

        let line = content_slice.chars().filter(|&c| c == '\n').count() as u32;

        let line_start = content_slice.rfind('\n').map_or(0, |pos| pos + 1);
        let character = content_slice[line_start..].chars().count() as u32;
        Position::new(line, character)
    }
}
