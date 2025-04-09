use std::usize;

use parser::{
    lock::LoggedReadGuard,
    structs::lock::Source,
    toml::{Dependency, Positioned},
    tree::RangeExclusive,
    Db,
};
use rust_version::RustVersion;
use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Url};

use crate::lsp::Context;

impl Context {
    async fn hover_version(
        &self,
        dep: &Positioned<Dependency>,
        offset: usize,
        uri: &Url,
        lock: &LoggedReadGuard<'_, Db>,
    ) -> Option<Hover> {
        let range = dep.data.source.range()?;
        if range.contains(offset) {
            let start = lock.get_offset(&uri, range.start as usize)?;
            let end = lock.get_offset(&uri, range.end as usize)?;
            let info = match self
                .info
                .get_info(dep.data.source.registry(), &dep.data.name.data)
                .await
            {
                Ok(v) => format!(
                    "List of all availanle versions: \n{}",
                    v.into_iter()
                        .map(|v| format!("- {}", v.vers))
                        .rev()
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                Err(_) => "Couldnt find version info".to_owned(),
            };
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: Some(Range {
                    start: Position {
                        line: start.0 as u32,
                        character: start.1 as u32,
                    },
                    end: Position {
                        line: end.0 as u32,
                        character: end.1 as u32,
                    },
                }),
            });
        }
        None
    }

    async fn hover_name(
        &self,
        dep: &Positioned<Dependency>,
        offset: usize,
        uri: &Url,
        lock: &LoggedReadGuard<'_, Db>,
    ) -> Option<Hover> {
        let range = RangeExclusive::from(&dep.data.name);
        if range.contains(offset) {
            let start = lock.get_offset(&uri, range.start as usize)?;
            let end = lock.get_offset(&uri, range.end as usize)?;
            let name = &dep.data.name.data;
            let lock = lock.get_lock(uri)?.packages();
            let lock = lock.get(name)?.first()?;
            let mut use_ = false;
            if let Some(Source::Registry(s)) = &lock.source {
                use_ = s == "https://github.com/rust-lang/crates.io-index";
            }
            if !use_ {
                return None;
            }
            let content = self
                .info
                .get_readme_api(&dep.data.name.data, &lock.version.to_string())
                .await?;

            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(Range {
                    start: Position {
                        line: start.0 as u32,
                        character: start.1 as u32,
                    },
                    end: Position {
                        line: end.0 as u32,
                        character: end.1 as u32,
                    },
                }),
            });
        }
        None
    }

    async fn hover_feature(
        &self,
        dep: &Positioned<Dependency>,
        offset: usize,
        uri: &Url,
        lock: &LoggedReadGuard<'_, Db>,
    ) -> Option<Hover> {
        if dep.end == 0 {
            return None;
        }
        let range = RangeExclusive::from(&dep.data.features);
        let vers = RustVersion::try_from(
            match &dep.data.source {
                parser::toml::DepSource::Version { value, .. } => Some(&value.value.data),
                parser::toml::DepSource::Workspace(_) => {
                    let workspace_uri = lock.get_workspace(uri)?;
                    let workspace = lock.get_toml(workspace_uri)?;
                    let w_dep = workspace
                        .dependencies
                        .iter()
                        .find(|v| v.data.name.data == dep.data.name.data)?;
                    Some(&w_dep.data.source.version()?.data)
                }
                _ => None,
            }?
            .as_str(),
        )
        .ok()?;
        if range.contains(offset) {
            let start = lock.get_offset(&uri, range.start as usize)?;
            let end = lock.get_offset(&uri, range.end as usize)?;
            let info = match self
                .info
                .get_info(dep.data.source.registry(), &dep.data.name.data)
                .await
            {
                Ok(v) => format!(
                    "List of all available features: \n{}",
                    v.into_iter()
                        .rfind(|v| match &v.ver() {
                            Some(v) => Some(v),
                            None => None,
                        } == Some(&vers))
                        .map(|v| v.features(lock.config.feature_display_mode))
                        .unwrap_or_default()
                        .into_iter()
                        .map(|v| format!("- {v}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
                Err(_) => "Couldnt find feature info".to_owned(),
            };
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: info,
                }),
                range: Some(Range {
                    start: Position {
                        line: start.0 as u32,
                        character: start.1 as u32,
                    },
                    end: Position {
                        line: end.0 as u32,
                        character: end.1 as u32,
                    },
                }),
            });
        }
        None
    }

    pub async fn hover_dep(
        &self,
        uri: &Url,
        position: Position,
        lock: &LoggedReadGuard<'_, Db>,
    ) -> Option<Hover> {
        let pos = (position.line as usize, position.character as usize);
        let dep = lock.get_dependency(
            &uri,
            pos,
            (position.line as usize, position.character as usize + 1),
        )?;
        let offset = lock.get_byte(&uri, pos.0, pos.1).unwrap_or_default();
        if let Some(v) = self.hover_version(dep, offset, uri, lock).await {
            return Some(v);
        }
        if let Some(v) = self.hover_name(dep, offset, uri, lock).await {
            return Some(v);
        }

        if let Some(v) = self.hover_feature(dep, offset, uri, lock).await {
            return Some(v);
        }
        None
    }
}
