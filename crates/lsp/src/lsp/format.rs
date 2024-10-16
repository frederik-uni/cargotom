use crate::context::Context;
use parser::structure::Indent;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{DocumentFormattingParams, TextEdit};

impl Context {
    pub async fn formatting_(
        &self,
        params: DocumentFormattingParams,
    ) -> Option<Result<Option<Vec<TextEdit>>>> {
        let temp = self.toml_store.read().await;
        let toml = temp.get(&params.text_document.uri.to_string())?;
        let raw = toml.as_raw()?;
        let v: Vec<TextEdit> = raw
            .format(
                *self.sort.read().await,
                params.options.insert_final_newline.unwrap_or(true),
                match params.options.insert_spaces {
                    true => Indent::Spaces(params.options.tab_size),
                    false => Indent::Tab,
                },
            )
            .into_iter()
            .map(|(range, text)| TextEdit {
                range: toml.to_range2(&range),
                new_text: text,
            })
            .collect();
        Some(Ok(Some(v)))
    }
}
