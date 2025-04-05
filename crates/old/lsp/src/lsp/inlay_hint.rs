use parser::structure::RustVersion;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{InlayHint, InlayHintKind, InlayHintParams, InlayHintTooltip, Url};

use crate::context::Context;
impl Context {
    pub async fn inlay_hint_(
        &self,
        params: InlayHintParams,
    ) -> Option<Result<Option<Vec<InlayHint>>>> {
        let toml = self.toml_store.read().await;
        let cargo_ = toml.get(&params.text_document.uri)?;
        let cargo = cargo_.as_cargo()?;
        let lock = cargo.lock_file_path.as_ref()?;
        let lock_uri = Url::from_file_path(&lock).ok()?;
        let packages = toml.get(&lock_uri)?.as_lock()?.packages();
        let hints = cargo
            .positioned_info
            .dependencies
            .iter()
            .filter_map(|dep| {
                packages
                    .get(&dep.data.name.data)
                    .and_then(|v| match v.len() > 1 {
                        true => match &dep.data.source {
                            parser::structure::Source::Version { value, .. } => {
                                let items =
                                    v.iter().filter(|v| v.is_registry()).collect::<Vec<_>>();
                                match items.len() > 1 {
                                    true => {
                                        let v = RustVersion::try_from(value.value.data.as_str());
                                        match v {
                                            Ok(ver) => {
                                                let items_t = items
                                                    .iter()
                                                    .filter(|v| v.version.mahor() == ver.mahor())
                                                    .collect::<Vec<_>>();
                                                match items_t.len() {
                                                    0 => items
                                                        .iter()
                                                        .min_by(|a, b| {
                                                            a.version.mahor().unwrap_or(99999).cmp(
                                                                &b.version.mahor().unwrap_or(99999),
                                                            )
                                                        })
                                                        .map(|v| (*v, dep.end)),
                                                    1 => Some((*items_t[0], dep.end)),
                                                    _ => {
                                                        let items = items_t
                                                            .iter()
                                                            .filter(|v| {
                                                                v.version.minor() == ver.minor()
                                                            })
                                                            .collect::<Vec<_>>();
                                                        match items.len() {
                                                            0 => items
                                                                .iter()
                                                                .min_by(|a, b| {
                                                                    a.version
                                                                        .minor()
                                                                        .unwrap_or(99999)
                                                                        .cmp(
                                                                            &b.version
                                                                                .minor()
                                                                                .unwrap_or(99999),
                                                                        )
                                                                })
                                                                .map(|v| (***v, dep.end)),
                                                            1 => Some((**items[0], dep.end)),
                                                            _ => {
                                                                let items_t = items
                                                                    .iter()
                                                                    .filter(|v| {
                                                                        v.version.patch()
                                                                            == ver.patch()
                                                                    })
                                                                    .collect::<Vec<_>>();
                                                                match items_t.len() {
                                                                    0 => items
                                                                        .iter()
                                                                        .min_by(|a, b| {
                                                                            a.version.patch().cmp(
                                                                                &b.version.patch(),
                                                                            )
                                                                        })
                                                                        .map(|v| (***v, dep.end)),
                                                                    1 => Some((
                                                                        ***items_t[0],
                                                                        dep.end,
                                                                    )),
                                                                    _ => None,
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            Err(_) => None,
                                        }
                                    }
                                    false => items.iter().next().map(|v| (*v, dep.end)),
                                }
                            }
                            parser::structure::Source::Git { .. } => {
                                let items = v.iter().filter(|v| v.is_git()).collect::<Vec<_>>();
                                match items.len() > 1 {
                                    true => items
                                        .iter()
                                        .max_by(|a, b| a.version().cmp(b.version()))
                                        .map(|v| (*v, dep.end)),
                                    false => items.iter().next().map(|v| (*v, dep.end)),
                                }
                            }
                            _ => {
                                let items = v
                                    .iter()
                                    .filter(|v| !v.is_git() && !v.is_registry())
                                    .collect::<Vec<_>>();
                                match items.len() > 1 {
                                    true => items
                                        .iter()
                                        .max_by(|a, b| a.version().cmp(b.version()))
                                        .map(|v| (*v, dep.end)),
                                    false => items.iter().next().map(|v| (*v, dep.end)),
                                }
                            }
                        },
                        false => v.iter().next().map(|v| (v, dep.end)),
                    })
            })
            .map(|(v, end)| InlayHint {
                position: cargo_.byte_offset_to_position(end),
                label: tower_lsp::lsp_types::InlayHintLabel::String(v.label()),
                kind: Some(InlayHintKind::TYPE),
                text_edits: None,
                tooltip: Some(InlayHintTooltip::String("this is a tooltip".to_string())),
                padding_left: Some(true),
                padding_right: Some(true),
                data: None,
            })
            .collect::<Vec<_>>();
        Some(Ok(Some(hints)))
    }
}
