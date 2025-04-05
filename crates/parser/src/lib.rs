pub mod structs;
pub mod toml;
pub mod tree;
mod tree_to_struct;

use std::{collections::HashMap, fs::read_to_string, path::PathBuf, sync::Arc};

use ropey::Rope;
use structs::lock::CargoLockRaw;
use toml::{Dependency, Positioned, Toml};
use tree::{RangeExclusive, Tree};
use tree_to_struct::to_struct;
use url::Url;

pub type Uri = url::Url;
#[derive(Default)]
pub struct Db {
    files: HashMap<Uri, Rope>,
    trees: HashMap<Uri, Tree>,
    tomls: HashMap<Uri, Toml>,
    workspaces: HashMap<Uri, Uri>,
    locks: HashMap<Uri, CargoLockRaw>,
}

impl Db {
    pub fn get_line(&self, uri: &Uri, bytes_offset: usize) -> Option<usize> {
        if let Some(v) = self.files.get(uri) {
            let line_index = v.byte_to_line(bytes_offset);
            return Some(line_index);
        }
        None
    }

    pub fn get_workspace(&self, uri: &Uri) -> Option<&Uri> {
        self.workspaces.get(uri)
    }

    pub fn get_toml(&self, uri: &Uri) -> Option<&Toml> {
        self.tomls.get(uri)
    }

    pub fn get_offset(&self, uri: &Uri, byte_offset: usize) -> Option<(usize, usize)> {
        if let Some(v) = self.files.get(uri) {
            let line_index = v.byte_to_line(byte_offset);
            let line_start_char = v.line_to_char(line_index);
            let char_offset = v.byte_to_char(byte_offset);
            let char_offset_in_line = char_offset - line_start_char;
            return Some((line_index, char_offset_in_line));
        }
        None
    }
    pub fn update_lock(&mut self, uri: Uri) {
        if let Ok(path) = uri.to_file_path() {
            if let Ok(str) = read_to_string(path) {
                if let Ok(lock) = ::toml::from_str::<CargoLockRaw>(&str) {
                    self.locks.insert(uri, lock);
                }
            }
        }
    }

    pub fn get_dependency(
        &self,
        uri: &Uri,
        (ls, cs): (usize, usize),
        (le, ce): (usize, usize),
    ) -> Option<&Positioned<Dependency>> {
        let file = self.files.get(uri)?;
        let cs = file.line_to_char(ls) + cs;
        let bs = file.char_to_byte(cs);
        let ce = file.line_to_char(le) + ce;
        let be = file.char_to_byte(ce);
        let toml = self.tomls.get(uri)?;
        let found = toml
            .dependencies
            .iter()
            .find(|v| v.overlap(RangeExclusive::new(bs as u32, be as u32)))?;
        Some(found)
    }
    pub fn reload(&mut self, uri: Uri) {
        let content = self.files.get(&uri);
        if let Some(content) = content {
            self.add_content(uri.clone(), &content.to_string());
            if let Some(tree) = self.trees.get(&uri) {
                let empty = Arc::new(Vec::new());
                let str = to_struct(tree, empty);
                if str.workspace {
                    for ur in &str.children {
                        let file_path = uri.to_file_path().unwrap();
                        let folder_path = file_path.parent();
                        let new_path = folder_path.map(|v| v.join(format!("{}/Cargo.toml", ur)));
                        let ur = Url::from_file_path(
                            &new_path.unwrap_or(PathBuf::from(format!("{}/Cargo.toml", ur))),
                        )
                        .unwrap();
                        self.try_init(&ur);
                        self.workspaces.insert(ur, uri.clone());
                    }
                }
                self.tomls.insert(uri, str);
            }
        }
    }

    pub fn update(
        &mut self,
        uri: &Uri,
        range: Option<((usize, usize), (usize, usize))>,
        content: &str,
    ) {
        if let Some(((sl, sc), (el, ec))) = range {
            let file = self.files.get_mut(&uri).unwrap();
            let start = file.line_to_char(sl) + sc;
            let end = file.line_to_char(el) + ec;
            file.remove(start..end);
            file.insert(start, content);
        } else {
            self.files.insert(uri.clone(), Rope::from_str(content));
        }
    }

    pub fn try_init(&mut self, file: &Uri) {
        if !self.files.contains_key(file) {
            self.add_file(file);
        }

        let file = Url::from_file_path(
            file.to_file_path()
                .unwrap()
                .parent()
                .map(|v| v.join("Cargo.lock"))
                .unwrap_or(PathBuf::from("Cargo.lock")),
        )
        .unwrap();
        if !self.locks.contains_key(&file) {
            self.update_lock(file);
        }
    }

    pub fn add_file(&mut self, file: &Uri) {
        let path = file.to_string();
        if let Some(path) = path.strip_prefix("file://") {
            if let Ok(data) = read_to_string(path) {
                self.update(file, None, &data);
            }
        }
    }

    fn add_content(&mut self, uri: Uri, content: &str) {
        let dom = taplo::parser::parse(content).into_dom();

        let tree = dom.as_table().map(Tree::from);

        if let Some(tree) = tree {
            self.trees.insert(uri, tree);
        }
    }
}

#[cfg(test)]
mod tests {

    use std::path::PathBuf;

    use url::Url;

    use crate::Db;

    #[test]
    fn parse() {
        let mut db = Db::default();
        let uri = Url::from_file_path(&PathBuf::from(
            "/Users/frederik/code/rust/cargotom/Cargo.toml",
        ))
        .unwrap();
        db.add_file(&uri);
        assert_eq!(db.files.len(), 1);
        db.reload(uri.clone());
        assert_eq!(db.files.len(), 3);
        println!("{:#?}", db.tomls)
    }
}
