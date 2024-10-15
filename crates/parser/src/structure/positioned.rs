use std::{borrow::Cow, sync::Arc};

use super::RangeExclusive;

/// Data structure used by the language server with positional information
#[derive(Default, Debug)]
pub struct PositionedInfo {
    /// Every dependency in the Cargo.toml file
    pub dependencies: Vec<Positioned<Dependency>>,
    /// Every feature in the Cargo.toml file
    pub features: Vec<Positioned<Feature>>,
}

/// Byte offset of start and end of a value
#[derive(Debug, Clone, Copy)]
pub struct Positioned<T> {
    /// Byte offset of the start of the value
    pub start: u32,
    /// Byte offset of the end of the value
    pub end: u32,
    /// Value
    pub data: T,
}

impl<T> Positioned<T> {
    pub fn contains_inclusive(&self, pos: u32) -> bool {
        self.start <= pos && pos <= self.end
    }

    pub fn is_in_range(&self, min: u32, max: u32) -> bool {
        self.start <= max && min <= self.end
    }
}

/// A dependency in the Cargo.toml file
#[derive(Debug)]
pub struct Dependency {
    /// Name of the dependency
    pub name: Positioned<String>,
    /// Dev dependency or normal dependency
    pub kind: DependencyKind,
    /// Source of the dependency
    pub source: Source,
    /// Enable features for this dependency
    pub(crate) features: Vec<Positioned<String>>,
    pub(crate) features_key_range: Option<RangeExclusive>,
    pub(crate) default_features: Option<Positioned<bool>>,
    /// Keys that are being typed
    pub typing_keys: Vec<Positioned<String>>,
    /// Is optional dependency
    pub(crate) optional: Option<Positioned<bool>>,
    /// Target platforms for this dependency
    /// if empty = all platforms
    pub(crate) target: Arc<Vec<Positioned<Target>>>,
}

impl Dependency {
    pub fn get_feature<'a>(&'a self, byte_offset: u32) -> Option<Cow<'a, Positioned<String>>> {
        if let Some(features_key_range) = &self.features_key_range {
            if features_key_range.contains_inclusive(byte_offset) {
                return Some(Cow::Owned(Positioned {
                    start: features_key_range.start,
                    end: features_key_range.end,
                    data: String::new(),
                }));
            }
        }
        self.features
            .iter()
            .find(|v| v.contains_inclusive(byte_offset))
            .map(|v| Cow::Borrowed(v))
    }

    pub fn get_version<'a>(&'a self, byte_offset: u32) -> Option<Cow<'a, Positioned<String>>> {
        match &self.source {
            Source::Version { value, .. } => {
                if let Some(range) = value.key {
                    if range.contains_inclusive(byte_offset) {
                        return Some(Cow::Owned(Positioned {
                            start: range.start,
                            end: range.end,
                            data: "version".to_string(),
                        }));
                    }
                }
                if value.value.contains_inclusive(byte_offset) {
                    Some(Cow::Borrowed(&value.value))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum Target {
    Unknown(String),
}

#[derive(Debug)]
pub enum Source {
    /// registry source
    Version {
        value: OptionalKey,
        registry: Option<WithKey>,
    },
    /// Git source
    Git {
        url: Option<WithKey>,
        rev: Option<WithKey>,
        tag: Option<WithKey>,
        branch: Option<WithKey>,
    },
    /// Local source
    Path(WithKey),
    /// Broken file
    /// for suggestions while typing the name
    None,
}

impl Source {
    pub fn end(&self) -> Option<u32> {
        match self {
            Source::Version { value, .. } => Some(value.value.end),
            _ => None,
        }
    }

    pub fn version(&self) -> Option<&Positioned<String>> {
        match self {
            Source::Version { value, .. } => Some(&value.value),
            _ => None,
        }
    }
    pub fn set_version(&mut self, key: OptionalKey) {
        match self {
            Source::Version { value, .. } => {
                *value = key;
            }
            _ => {
                *self = Source::Version {
                    value: key,
                    registry: None,
                }
            }
        }
    }

    pub fn set_git(&mut self, key: WithKey) {
        match self {
            Source::Git { url, .. } => {
                *url = Some(key);
            }
            _ => {
                *self = Source::Git {
                    url: Some(key),
                    rev: None,
                    tag: None,
                    branch: None,
                }
            }
        }
    }

    pub fn set_branch(&mut self, key: WithKey) {
        match self {
            Source::Git { branch, .. } => {
                *branch = Some(key);
            }
            _ => {
                *self = Source::Git {
                    url: None,
                    rev: None,
                    tag: None,
                    branch: Some(key),
                }
            }
        }
    }

    pub fn set_rev(&mut self, key: WithKey) {
        match self {
            Source::Git { rev, .. } => {
                *rev = Some(key);
            }
            _ => {
                *self = Source::Git {
                    url: None,
                    rev: Some(key),
                    tag: None,
                    branch: None,
                }
            }
        }
    }

    pub fn set_tag(&mut self, key: WithKey) {
        match self {
            Source::Git { tag, .. } => {
                *tag = Some(key);
            }
            _ => {
                *self = Source::Git {
                    url: None,
                    rev: None,
                    tag: Some(key),
                    branch: None,
                }
            }
        }
    }

    pub fn set_path(&mut self, key: WithKey) {
        match self {
            Source::Path(value) => {
                *value = key;
            }
            _ => {
                *self = Source::Path(key);
            }
        }
    }

    pub fn set_registry(&mut self, key: WithKey) {
        match self {
            Source::Version { registry, .. } => {
                *registry = Some(key);
            }
            _ => {
                *self = Source::Version {
                    value: OptionalKey {
                        key: None,
                        value: Positioned {
                            start: 0,
                            end: 0,
                            data: String::new(),
                        },
                    },
                    registry: Some(key),
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct WithKey {
    key: RangeExclusive,
    value: Positioned<String>,
}

impl WithKey {
    pub fn new(range: RangeExclusive, value: Positioned<String>) -> Self {
        Self { key: range, value }
    }
}

#[derive(Debug)]
pub struct OptionalKey {
    pub key: Option<RangeExclusive>,
    pub value: Positioned<String>,
}

impl OptionalKey {
    pub fn with_key(range: RangeExclusive, value: Positioned<String>) -> Self {
        Self {
            key: Some(range),
            value,
        }
    }
    pub fn no_key(value: Positioned<String>) -> Self {
        Self { key: None, value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    /// dependencies
    Normal,
    /// dev-dependencies
    Development,
    /// build-dependencies
    Build,
}

#[derive(Debug)]
pub struct Feature {
    /// feature-name =
    pub name: Positioned<String>,
    /// = [...]
    pub args: Vec<FeatureArgKind>,
}

#[derive(Debug)]
pub enum FeatureArgKind {
    /// "feautre-name"
    CrateFeature(Positioned<String>),
    /// "crate/feature-name"
    DependencyFeature {
        dependency: Positioned<String>,
        feature: Positioned<String>,
    },
    /// "dep:crate"
    Dependency(Positioned<String>),
}

impl From<Positioned<String>> for FeatureArgKind {
    fn from(value: Positioned<String>) -> Self {
        if let Some(name) = value.data.strip_prefix("dep:") {
            Self::Dependency(Positioned {
                start: value.start,
                end: value.end,
                data: name.to_string(),
            })
        } else if let Some((crate_name, feature_name)) = value.data.split_once('/') {
            Self::DependencyFeature {
                dependency: Positioned {
                    start: value.start,
                    end: value.start + crate_name.len() as u32,
                    data: crate_name.to_string(),
                },
                feature: Positioned {
                    start: value.start + crate_name.len() as u32,
                    end: value.end,
                    data: feature_name.to_string(),
                },
            }
        } else {
            Self::CrateFeature(value)
        }
    }
}
