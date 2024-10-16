use std::collections::{HashMap, HashSet};

use parser::structure::{RangeExclusive, RustVersion};
use tower_lsp::lsp_types::{self, Diagnostic, DiagnosticSeverity, Range, Url};

use crate::context::Context;

enum RangeExclusiveOrRange {
    Range(Range),
    RangeExclusive(RangeExclusive),
}

impl From<RangeExclusive> for RangeExclusiveOrRange {
    fn from(value: RangeExclusive) -> Self {
        Self::RangeExclusive(value)
    }
}

impl Context {
    pub async fn analyze(&self, uri: &Url) {
        let mut issues: HashMap<&Url, Vec<(RangeExclusiveOrRange, String)>> = HashMap::new();

        if !*self.hide_docs_info_message.read().await {
            issues.entry(uri).or_default().push((RangeExclusiveOrRange::Range(Range {
                start: lsp_types::Position {
                    line: 0,
                    character: 0,
                },
                end: lsp_types::Position {
                    line: 1,
                    character: 0,
                },
            }), "\nThe first line of every Cargo.toml has code actions that will open docs/issues for the cargotom lsp.\n\n To hide this message please set hide_docs_info_message. \n\nFor further information please check out the docs".to_string()));
        }

        let items = self.toml_store.read().await;
        let crate_info = self.crates.read().await;
        let crates = items
            .iter()
            .filter_map(|(_, toml)| toml.as_cargo())
            .flat_map(|v| &v.positioned_info.dependencies)
            .filter(|v| v.data.source.version().is_some())
            .map(|v| {
                let name = &v.data.name.data;
                let version = v.data.source.version().unwrap();
                (name, &version.data)
            })
            .collect::<HashSet<_>>();
        let mut max_version_map = HashMap::new();
        let mut crate_features_map = HashMap::new();
        for (name, version) in crates {
            let max_version = crate_info
                .get_versions(name, "")
                .await
                .unwrap_or_default()
                .into_iter()
                .max();
            if let Some(max_version) = max_version {
                max_version_map.insert(name, max_version);
            }
            let features = crate_info
                .get_features(name, version, "")
                .await
                .unwrap_or_default();
            crate_features_map.insert((name, version), features);
        }
        for (uri, toml) in items.iter() {
            let toml = match toml.as_cargo() {
                Some(v) => v,
                None => continue,
            };
            for dependency in &toml.positioned_info.dependencies {
                let name = &dependency.data.name.data;
                if let Some(version) = dependency.data.source.version() {
                    match max_version_map.get(&&dependency.data.name.data) {
                        Some(new) => {
                            let newer_version = RustVersion::try_from(version.data.as_str())
                                .map(|v| new > &v)
                                .unwrap_or(true);
                            if newer_version {
                                issues.entry(uri).or_default().push((
                                    RangeExclusive::new(version.start, version.end).into(),
                                    format!(
                                        "A newer version is available for crate `{name}`: {new} "
                                    ),
                                ));
                            }
                        }
                        None => {
                            issues.entry(uri).or_default().push((
                                RangeExclusive::new(version.start, version.end).into(),
                                format!("couldnt find version for crate `{name}"),
                            ));
                        }
                    };

                    match crate_features_map.get(&(name, &version.data)) {
                        Some(existing_features) => {
                            for feature in dependency.data.features.iter() {
                                if !existing_features.contains(&feature.data) {
                                    issues.entry(uri).or_default().push((
                                        RangeExclusive::new(version.start, version.end).into(),
                                        format!(
                                            "Unknown feature `{}` for crate `{name}`",
                                            feature.data
                                        ),
                                    ));
                                }
                            }
                        }
                        None => {
                            issues.entry(uri).or_default().push((
                                RangeExclusive::new(version.start, version.end).into(),
                                format!("Unknown version for crate `{name}` "),
                            ));
                        }
                    }
                }
            }
        }
        for (uri, issues) in issues {
            let toml = self.toml_store.read().await;
            let toml = match toml.get(uri) {
                Some(v) => v,
                None => continue,
            };
            self.client
                .publish_diagnostics(
                    uri.to_owned(),
                    issues
                        .into_iter()
                        .map(|(range, message)| Diagnostic {
                            range: match range {
                                RangeExclusiveOrRange::Range(range) => range,
                                RangeExclusiveOrRange::RangeExclusive(range_exclusive) => {
                                    toml.to_range2(&range_exclusive)
                                }
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            message,
                            ..Default::default()
                        })
                        .collect(),
                    None,
                )
                .await;
        }
    }
}
