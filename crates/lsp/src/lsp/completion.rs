use crate::context::{Context, Toml};
use crate::util::get_byte_index_from_position;
use parser::structure::{Dependency, Positioned};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionList, CompletionParams, CompletionResponse,
    CompletionTextEdit, MessageType, TextEdit,
};

impl Context {
    pub async fn completion_(
        &self,
        params: CompletionParams,
    ) -> Option<Result<Option<CompletionResponse>>> {
        let uri = params.text_document_position.text_document.uri;
        let toml = self.toml_store.read().await;
        let toml = toml.get(&uri.to_string())?;

        let byte_offset =
            get_byte_index_from_position(toml.text(), params.text_document_position.position)
                as u32;

        let dep = toml.get_dependency(byte_offset)?;
        if dep.data.name.contains_inclusive(byte_offset) {
            self.client
                .log_message(MessageType::INFO, format!("name value {:?}", dep.data.name))
                .await;
            let existing_crates = toml
                .as_cargo()?
                .positioned_info
                .dependencies
                .iter()
                .filter(|v| v.data.kind == dep.data.kind)
                .collect::<Vec<_>>();
            //TODO: add workspace_deps
            let workspace_deps = vec![];
            let v = self
                .name_completion(&dep.data.name, workspace_deps, existing_crates)
                .await?;
            return Some(Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: v,
            }))));
        } else if let Some(v) = dep.data.get_feature(byte_offset) {
            match v {
                std::borrow::Cow::Borrowed(feature) => {
                    let v = self.feature_completion(&dep.data, &feature.data).await?;
                    return Some(Ok(Some(CompletionResponse::List(CompletionList {
                        is_incomplete: false,
                        items: v,
                    }))));
                }
                std::borrow::Cow::Owned(_) => {}
            }
        } else if let Some(v) = dep.data.get_version(byte_offset) {
            match v {
                std::borrow::Cow::Borrowed(_) => {
                    let v = self.version_completion(toml, &dep.data).await?;
                    return Some(Ok(Some(CompletionResponse::List(CompletionList {
                        is_incomplete: false,
                        items: v,
                    }))));
                }
                std::borrow::Cow::Owned(v) => {
                    let v = self
                        .version_key_completion(&dep.data.name.data, toml, &v)
                        .await?;
                    return Some(Ok(Some(CompletionResponse::List(CompletionList {
                        is_incomplete: false,
                        items: v,
                    }))));
                }
            }
        } else if let Some(v) = dep
            .data
            .typing_keys
            .iter()
            .find(|v| v.contains_inclusive(byte_offset))
        {
            let v = self
                .version_key_completion(&dep.data.name.data, toml, v)
                .await?;
            return Some(Ok(Some(CompletionResponse::List(CompletionList {
                is_incomplete: false,
                items: v,
            }))));
        }
        None
    }

    async fn name_completion(
        &self,
        crate_name: &Positioned<String>,
        workspace_deps: Vec<&Dependency>,
        existing_crates: Vec<&Positioned<Dependency>>,
    ) -> Option<Vec<CompletionItem>> {
        let mut result = self.crates.read().await.search(&crate_name.data).await;
        result.sort_by(|(name_a, _, _), (name_b, _, _)| name_a.len().cmp(&name_b.len()));
        Some(
            result
                .into_iter()
                .filter(|(crate_name_, _, _)| {
                    crate_name_ == &crate_name.data
                        || existing_crates
                            .iter()
                            .find(|v| &v.data.name.data == crate_name_)
                            .is_none()
                })
                .enumerate()
                .map(|(index, (name, detail, version))| CompletionItem {
                    label: name.clone(),
                    detail,
                    sort_text: Some(format!("{:04}", index)),
                    insert_text: Some(
                        match workspace_deps
                            .iter()
                            .find(|v| &v.name.data == &name)
                            .is_some()
                        {
                            true => format!("{name} = {} workspace = true {}", '{', '}'),
                            false => format!("{name} = \"{version}\""),
                        },
                    ),
                    kind: Some(CompletionItemKind::SNIPPET),
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        )
    }

    async fn version_completion(
        &self,
        toml: &Toml,
        dep: &Dependency,
    ) -> Option<Vec<CompletionItem>> {
        let version = dep.source.version()?;
        let versions = self
            .crates
            .read()
            .await
            .get_versions(&dep.name.data, &version.data)
            .await?;
        let range = toml.to_range(version);
        Some(
            versions
                .into_iter()
                .enumerate()
                .map(|(index, ver)| CompletionItem {
                    label: ver.to_string(),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                        range,
                        format!("\"{}\"", ver.to_string(),),
                    ))),
                    sort_text: Some(format!("{:04}", index)),
                    detail: None,
                    ..Default::default()
                })
                .collect(),
        )
    }

    async fn version_key_completion(
        &self,
        crate_name: &str,
        toml: &Toml,
        query: &Positioned<String>,
    ) -> Option<Vec<CompletionItem>> {
        let versions = self
            .crates
            .read()
            .await
            .get_versions(crate_name, "")
            .await?;
        let versions = versions.first()?;
        if "version".starts_with(&query.data) {
            //TODO: add is_missing
            let is_missing = false;
            let range = toml.to_range(&query);
            return Some(vec![CompletionItem {
                label: "version".to_string(),
                detail: None,
                kind: Some(CompletionItemKind::SNIPPET),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                    range,
                    format!(
                        "version = \"{}\"{}",
                        versions.to_string(),
                        match is_missing {
                            true => "}",
                            false => "",
                        }
                    ),
                ))),
                ..Default::default()
            }]);
        }
        None
    }

    async fn feature_completion(
        &self,
        dep: &Dependency,
        feature: &str,
    ) -> Option<Vec<CompletionItem>> {
        let version = dep.source.version()?;
        let versions = self
            .crates
            .read()
            .await
            .get_features(&dep.name.data, &version.data, feature)
            .await?;
        Some(
            versions
                .into_iter()
                .map(|v| CompletionItem {
                    label: v,
                    detail: None,
                    ..Default::default()
                })
                .collect(),
        )
    }
}