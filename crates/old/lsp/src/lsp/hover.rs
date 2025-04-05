use parser::structure::Source;
use tower_lsp::lsp_types::{HoverContents, HoverParams, MarkupContent, MarkupKind, Range};
use tower_lsp::{jsonrpc::Result, lsp_types::Hover};

use crate::context::Context;
use crate::util::get_byte_index_from_position;

impl Context {
    pub async fn hover_(&self, params: HoverParams) -> Option<Result<Option<Hover>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let toml = self.toml_store.read().await;
        let toml = toml.get(&uri)?;

        let byte_offset = get_byte_index_from_position(
            toml.text(),
            params.text_document_position_params.position,
        ) as u32;

        let dep = toml.get_dependency(byte_offset)?;

        if let Some(feature) = dep.data.get_feature(byte_offset) {
            return self.feature_hover(dep, toml, feature).await;
        }

        if let Some(feature) = dep.data.get_version(byte_offset) {
            return self.version_hover(dep, toml, feature).await;
        }

        Some(Ok(None))
    }

    async fn feature_hover(
        &self,
        dep: &parser::structure::Positioned<parser::structure::Dependency>,
        toml: &crate::context::Toml,
        feature: std::borrow::Cow<'_, parser::structure::Positioned<String>>,
    ) -> Option<Result<Option<Hover>>> {
        let v = self.crates.read().await;
        if let Source::Version { value, .. } = &dep.data.source {
            let features = v
                .get_features(&dep.data.name.data, value.value.data.as_str(), "")
                .await?;
            return Some(Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: features
                        .iter()
                        .map(|v| format!("- {}", v))
                        .collect::<Vec<_>>()
                        .join("\n"),
                }),
                range: Some(Range::new(
                    toml.byte_offset_to_position(feature.start),
                    toml.byte_offset_to_position(feature.end),
                )),
            })));
        };
        None
    }

    async fn version_hover(
        &self,
        dep: &parser::structure::Positioned<parser::structure::Dependency>,
        toml: &crate::context::Toml,
        version: std::borrow::Cow<'_, parser::structure::Positioned<String>>,
    ) -> Option<Result<Option<Hover>>> {
        let v = self.crates.read().await;
        let features = v.get_versions(&dep.data.name.data, "").await?;
        Some(Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: features
                    .iter()
                    .map(|v| format!("- {}", v))
                    .collect::<Vec<_>>()
                    .join("\n"),
            }),
            range: Some(Range::new(
                toml.byte_offset_to_position(version.start),
                toml.byte_offset_to_position(version.end),
            )),
        })))
    }
}
