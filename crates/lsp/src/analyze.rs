use tower_lsp::lsp_types::{self};

use crate::context::Context;

impl Context {
    pub async fn analyze(&self, uri: &str) -> Vec<lsp_types::Diagnostic> {
        //TODO: analyze
        vec![]
    }
}
