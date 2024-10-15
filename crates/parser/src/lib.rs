use std::{path::PathBuf, sync::Arc};

pub use structure::Cargo;
use structure::{CargoRawData, Workspace};

mod dependencies;
mod features;
mod raw_to_tree;
pub mod structure;
mod tree_to_struct;
mod util;
mod workspace;

impl CargoRawData {
    pub fn reload(&mut self, cargo: &mut Cargo) -> Option<PathBuf> {
        self.generate_tree();
        let empty = Arc::new(Vec::new());
        cargo.clean();
        cargo.update_struct(&self.tree, empty);
        if let Some(lock_file) = &cargo.lock_file_path {
            let lock_same_level = cargo.path.parent() == lock_file.parent();
            match &cargo.info.workspace {
                structure::Workspace::WorkspaceModule { .. } => {
                    if lock_same_level {
                        cargo.info.workspace = Workspace::Package;
                    }
                }
                structure::Workspace::Package => {
                    if !lock_same_level {
                        let v = lock_file.parent().map(|v| v.join("Cargo.toml"));
                        cargo.info.workspace = Workspace::WorkspaceModule {
                            path: v.clone().unwrap_or_default(),
                        };
                        return v;
                    }
                }
                _ => {}
            }
        }
        None
    }
}
