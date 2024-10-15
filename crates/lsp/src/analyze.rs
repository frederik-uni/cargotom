use tower_lsp::lsp_types::{self, DiagnosticSeverity, DiagnosticTag, Url};

use crate::context::Context;

impl Context {
    pub async fn analyze(&self, uri: &Url) -> Vec<lsp_types::Diagnostic> {
        if !*self.hide_docs_info_message.read().await {
            let v = lsp_types::Diagnostic {
                range: lsp_types::Range {
                    start: lsp_types::Position {
                        line: 0,
                        character: 0,
                    },
                    end: lsp_types::Position {
                        line: 1,
                        character: 0,
                    },
                },
                severity: Some(DiagnosticSeverity::WARNING),
                source: Some("cargotom".to_string()),
                message: "\nThe first line of every Cargo.toml has code actions that will open docs/issues for the cargotom lsp.\n\n To hide this message please set hide_docs_info_message. \n\nFor further information please check out the docs".to_string(),
                ..Default::default()
            };
            self.client
                .publish_diagnostics(uri.to_owned(), vec![v], None)
                .await;
        }

        //TODO: analyze
        vec![]
    }
}
