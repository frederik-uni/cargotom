use std::usize;

use info_provider::api::ViewMode;
use parser::{
    structs::version::RustVersion,
    toml::{Dependency, Positioned},
    tree::RangeExclusive,
    Db,
};
use tokio::sync::RwLockReadGuard;
use tower_lsp::lsp_types::{
    Hover, HoverContents, MarkupContent, MarkupKind, MessageType, Position, Range, Url,
};

use crate::lsp::Context;

impl Context {
    async fn hover_version(
        &self,
        dep: &Positioned<Dependency>,
        offset: usize,
        uri: &Url,
        lock: &RwLockReadGuard<'_, Db>,
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
                Ok(v) => v
                    .into_iter()
                    .map(|v| format!("- {}", v.vers))
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
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
        lock: &RwLockReadGuard<'_, Db>,
    ) -> Option<Hover> {
        let range = RangeExclusive::from(&dep.data.name);
        if range.contains(offset) {
            let start = lock.get_offset(&uri, range.start as usize)?;
            let end = lock.get_offset(&uri, range.end as usize)?;

            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: "name".to_owned(),
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
        lock: &RwLockReadGuard<'_, Db>,
    ) -> Option<Hover> {
        if dep.end == 0 {
            return None;
        }
        let range = RangeExclusive::from(&dep.data.features);
        let vers = RustVersion::try_from(
            match &dep.data.source {
                parser::toml::DepSource::Version { value, registry } => Some(&value.value.data),
                parser::toml::DepSource::Workspace => {
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
                Ok(v) => v
                    .into_iter()
                    .rfind(|v|
                       match &v.ver() {
                            Some(v) => Some(v),
                            None => None,
                        } == Some(&vers)
                    )
                    .map(|v| v.features(ViewMode::UnusedOpt))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|v| format!("- {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
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
        lock: &RwLockReadGuard<'_, Db>,
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
