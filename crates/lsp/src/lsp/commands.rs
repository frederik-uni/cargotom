use std::process::Command;

use tower_lsp::lsp_types::{ExecuteCommandParams, MessageType};

use crate::context::Context;

impl Context {
    pub async fn execute_command_(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<(), tower_lsp::jsonrpc::Error> {
        if params.command == "cargo-update" {
            let _ = Command::new("cargo").arg("update").spawn();
        } else if params.command == "open_url" {
            let mut args = params.arguments.iter();
            if let Some(url) = args.next().and_then(|arg| arg.as_str()) {
                if let Err(e) = webbrowser::open(url) {
                    self.client
                        .show_message(
                            MessageType::WARNING,
                            format!("failed to open browser {}", e.to_string()),
                        )
                        .await;
                    return Err(tower_lsp::jsonrpc::Error::invalid_params(
                        "failed to open browser",
                    ));
                }
            } else {
                return Err(tower_lsp::jsonrpc::Error::invalid_params(
                    "URL argument missing",
                ));
            }
        } else {
            return Err(tower_lsp::jsonrpc::Error::method_not_found());
        }
        Ok(())
    }
}
