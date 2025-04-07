use std::collections::HashMap;

use async_recursion::async_recursion;
use info_provider::api::CacheItemOut;
use rust_version::RustVersion;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::{toml::DepSource, tree::RangeExclusive, Db, Level, Uri, Warning};

impl Db {
    pub async fn analyze(&self, uri: Option<Uri>) {
        let _ = self.client.inlay_hint_refresh().await;
        if let Some(uri) = &uri {
            self.analyze_single(uri).await;
        } else {
            for item in self.files.keys() {
                self.analyze_single(item).await;
            }
        }
    }
    #[async_recursion]
    async fn analyze_single(&self, uri: &Uri) -> Option<()> {
        let toml = self.tomls.get(uri)?;
        let mut errors = vec![];
        let mut warnings = vec![];
        let mut names: HashMap<String, Vec<_>> = HashMap::new();
        let workspace = self.workspaces.get(uri).and_then(|v| self.tomls.get(v));

        for toml in &toml.dependencies {
            let mut features: HashMap<String, Vec<_>> = HashMap::new();
            for feature in &toml.data.features.data {
                features
                    .entry(feature.data.clone())
                    .or_default()
                    .push(feature.clone());
            }
            for (_, features) in features {
                if features.len() > 1 {
                    for feature in features {
                        errors.push((
                            RangeExclusive::from(&feature),
                            format!("Duplicate feature name"),
                        ));
                    }
                }
            }
            let targets: Vec<_> = toml
                .data
                .target
                .iter()
                .map(|v| v.data.to_string())
                .collect();
            let targets = targets.join(" ");
            names
                .entry(format!(
                    "{}{:?}{}",
                    toml.data.name.data, toml.data.kind, targets
                ))
                .or_default()
                .push(toml)
        }
        for (_, tomls) in &names {
            if tomls.len() > 1 {
                for toml in tomls {
                    errors.push((
                        RangeExclusive::from(&toml.data.name),
                        format!("Duplicate dependency name"),
                    ));
                }
            }
        }
        for toml in &toml.dependencies {
            let src = if let DepSource::Workspace(range) = &toml.data.source {
                workspace.as_ref().and_then(|v| {
                    v.dependencies
                        .iter()
                        .find(|v| v.data.name.data == toml.data.name.data)
                        .map(|v| (&v.data.source, Some(range.clone())))
                })
            } else {
                Some((&toml.data.source, toml.data.source.range()))
            };
            if let Some((DepSource::Version { value, registry }, range)) = src {
                let range = range.unwrap();
                let info = self
                    .info
                    .get_info_cache(
                        registry.as_ref().map(|v| v.value.data.as_str()),
                        &toml.data.name.data,
                    )
                    .await;
                match info {
                    CacheItemOut::Error(e) => {
                        errors.push((RangeExclusive::from(&toml.data.name), e))
                    }
                    CacheItemOut::NotStarted | CacheItemOut::Pending => {
                        let info = self.info.clone();
                        let reg = registry.as_ref().map(|v| v.value.data.to_string());
                        let name = toml.data.name.data.to_owned();
                        let uri = uri.clone();
                        let sel = self.sel.clone().unwrap();
                        tokio::spawn(async move {
                            let _ = info.get_info(reg.as_deref(), &name).await;
                            let lock = sel.read().await;
                            lock.analyze_single(&uri).await;
                        });
                    }
                    CacheItemOut::Ready(items) => {
                        let ver = RustVersion::try_from(value.value.data.as_str());
                        if let Ok(ver) = ver {
                            let versions = items
                                .iter()
                                .filter_map(|v| v.ver().map(|ver| (v, ver)))
                                .collect::<Vec<_>>();
                            if let Some((package, ..)) = versions.iter().rfind(|(_, v)| v == &ver) {
                                let all_features = package.feature_all();
                                for feature in &toml.data.features.data {
                                    if !all_features.contains(&feature.data) {
                                        errors.push((
                                            RangeExclusive::from(feature),
                                            "Unknown Feature".to_string(),
                                        ))
                                    }
                                }
                                if let Some((info, _)) = versions
                                    .iter()
                                    .filter(|v| match self.config.stable_version {
                                        true => !v.1.is_pre_release(),
                                        false => true,
                                    })
                                    .rfind(|(_, v)| v > &ver)
                                {
                                    warnings.push((
                                        range,
                                        format!("Newer version available: {}", info.vers),
                                    ))
                                }
                            } else {
                                errors.push((range, "Invalid version".to_string()))
                            }
                        }
                    }
                }
            } else if let DepSource::Workspace(range) = &toml.data.source {
                match workspace {
                    Some(w) => {
                        if w.dependencies
                            .iter()
                            .find(|v| v.data.name.data == toml.data.name.data)
                            .is_none()
                        {
                            errors.push((range.clone(), format!("coundt find crate in workspace")));
                        }
                    }
                    None => errors.push((range.clone(), format!("isnt part of a workspace"))),
                }
            }
        }
        let mut warn = vec![];
        for (range, msg) in warnings {
            let start = self.get_offset(uri, range.start as usize);
            let end = self.get_offset(uri, range.end as usize);
            if let (Some(start), Some(end)) = (start, end) {
                warn.push(Warning {
                    level: crate::Level::Warn,
                    msg,
                    start,
                    end,
                });
            }
        }
        for (range, msg) in errors {
            let start = self.get_offset(uri, range.start as usize);
            let end = self.get_offset(uri, range.end as usize);
            if let (Some(start), Some(end)) = (start, end) {
                warn.push(Warning {
                    level: crate::Level::Error,
                    msg,
                    start,
                    end,
                });
            }
        }
        let lock = self.warnings.write();
        lock.await.insert(uri.clone(), warn.clone());
        let hide_docs_info_message = self.config.hide_docs_info_message;
        self.client
            .publish_diagnostics(
                uri.clone(),
                to_diagnostics(hide_docs_info_message, warn),
                None,
            )
            .await;
        Some(())
    }
}

fn to_diagnostics(hide_docs_info_message: bool, items: Vec<Warning>) -> Vec<Diagnostic> {
    let mut d = match hide_docs_info_message {
        false => vec![
            Diagnostic {
                range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 1,
                            character: 0,
                        }},
                severity: Some(DiagnosticSeverity::INFORMATION),
                code: None,
                code_description: None,
                source: None,
                message: "\nThe first line of every Cargo.toml has code actions that will open docs/issues for the cargotom lsp.\n\n To hide this message please set hide_docs_info_message. \n\nFor further information please check out the docs".to_owned(),
                related_information: None,
                tags: None,
                data: None,
            }
        ],
        true => vec![],
    };
    for item in items {
        d.push(Diagnostic {
            range: Range {
                start: Position {
                    line: item.start.0 as u32,
                    character: item.start.1 as u32,
                },
                end: Position {
                    line: item.end.0 as u32,
                    character: item.end.1 as u32,
                },
            },
            severity: Some(match item.level {
                Level::Warn => DiagnosticSeverity::WARNING,
                Level::Error => DiagnosticSeverity::ERROR,
            }),
            code: None,
            code_description: None,
            source: None,
            message: item.msg,
            related_information: None,
            tags: None,
            data: None,
        });
    }
    d
}
