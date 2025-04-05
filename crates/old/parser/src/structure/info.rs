use std::path::PathBuf;

/// Data structure used by the language server without positional information
#[derive(Default, Debug)]
pub struct Info {
    /// workspace information
    pub workspace: Workspace,
}

impl Info {
    pub fn add_workspace_members(&mut self, new_members: Vec<String>) {
        match &mut self.workspace {
            Workspace::Workspace { members, .. } => members.extend(new_members),
            _ => {
                self.workspace = Workspace::Workspace {
                    members: new_members,
                }
            }
        }
    }
}

/// workspace information
#[derive(Debug, Clone)]
pub enum Workspace {
    /// Workspace file
    Workspace {
        /// Location of the Workspace modules
        members: Vec<String>,
    },
    WorkspaceModule {
        /// Location of the Workspace file
        path: PathBuf,
    },
    Package,
}

impl Workspace {
    pub fn workspace() -> Self {
        Workspace::Workspace { members: vec![] }
    }
    pub fn module(path: PathBuf) -> Self {
        Workspace::WorkspaceModule { path }
    }
    pub fn package() -> Self {
        Workspace::Package
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace::Package
    }
}
