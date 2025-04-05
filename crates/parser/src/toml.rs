use std::{fmt::Display, sync::Arc};

use url::Url;

use crate::tree::RangeExclusive;

#[derive(Debug)]
pub struct Toml {
    pub workspace: bool,
    pub children: Vec<String>,
    pub dependencies: Vec<Positioned<Dependency>>,
    pub features: Vec<Positioned<Feature>>,
}

impl Toml {
    pub fn join(self, other: Self) -> Self {
        Self {
            workspace: self.workspace || other.workspace,
            children: self.children.into_iter().chain(other.children).collect(),
            dependencies: self
                .dependencies
                .into_iter()
                .chain(other.dependencies)
                .collect(),
            features: self.features.into_iter().chain(other.features).collect(),
        }
    }
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
    pub fn new(start: u32, end: u32, data: T) -> Self {
        Self { start, end, data }
    }

    pub fn contains(&self, offset: usize) -> bool {
        self.start <= offset as u32 && offset as u32 <= self.end
    }

    pub fn overlap(&self, range: RangeExclusive) -> bool {
        range.start <= self.end && range.end >= self.start
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

#[derive(Debug, Clone)]
pub struct Dependency {
    /// Name of the dependency
    pub name: Positioned<String>,
    /// Dev dependency or normal dependency
    pub kind: DependencyKind,
    /// Source of the dependency
    pub source: DepSource,
    /// Enable features for this dependency
    pub features: Positioned<Vec<Positioned<String>>>,
    pub features_key_range: Option<RangeExclusive>,
    pub default_features: Option<Positioned<bool>>,
    /// Keys that are being typed
    pub typing_keys: Vec<Positioned<String>>,
    /// Is optional dependency
    pub optional: Option<Positioned<bool>>,
    pub expanded: bool,
    /// Target platforms for this dependency
    /// if empty = all platforms
    pub target: Arc<Vec<Positioned<Target>>>,
}

impl Display for DepSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DepSource::Version { value, registry } => match registry {
                Some(r) => write!(
                    f,
                    "version = \"{}\", registry = \"{}\"",
                    value.value.data, r.value.data
                ),
                None => write!(f, "version = \"{}\"", value.value.data),
            },
            DepSource::Git {
                url,
                rev,
                tag,
                branch,
            } => match (rev, tag, branch) {
                (Some(rev), None, None) => write!(
                    f,
                    "git = \"{}\", rev = \"{}\"",
                    url.clone().map(|v| v.value.data).unwrap_or_default(),
                    rev.value.data
                ),
                (None, Some(tag), None) => write!(
                    f,
                    "git = \"{}\", tag = \"{}\"",
                    url.clone().map(|v| v.value.data).unwrap_or_default(),
                    tag.value.data
                ),
                (None, None, Some(branch)) => write!(
                    f,
                    "git = \"{}\", branch = \"{}\"",
                    url.clone().map(|v| v.value.data).unwrap_or_default(),
                    branch.value.data
                ),
                _ => write!(
                    f,
                    "git = \"{}\"",
                    url.clone().map(|v| v.value.data).unwrap_or_default(),
                ),
            },
            DepSource::Path(p) => write!(f, "path = \"{}\"", p.value.data),
            DepSource::None => write!(f, "version = \"\""),
            DepSource::Workspace(_) => write!(f, "workspace = true"),
        }
    }
}

impl Display for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.optional.map(|v| v.data).unwrap_or_default() || self.expanded {
            true => {
                let mut items = vec![self.source.to_string()];
                if let Some(v) = self.optional {
                    if v.data {
                        items.push("optional = true".to_string());
                    }
                }
                if let Some(v) = self
                    .default_features
                    .map(|v| format!("default-features = {}", v.data))
                {
                    items.push(v);
                }
                if !self.features.data.is_empty() {
                    items.push(format!(
                        "features = [ {} ]",
                        self.features
                            .data
                            .iter()
                            .map(|v| v.data.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }

                write!(f, "{} = {{ {} }}", self.name.data, items.join(", "))
            }
            false => match &self.source {
                DepSource::Version { value, .. } => {
                    write!(f, "{} = \"{}\"", &self.name.data, &value.value.data)
                }
                DepSource::Git { url, .. } => write!(
                    f,
                    "{}.git = \"{}\"",
                    &self.name.data,
                    url.clone().map(|v| v.value.data).unwrap_or_default(),
                ),
                DepSource::Path(path) => {
                    write!(f, "{}.path = \"{}\"", &self.name.data, &path.value.data,)
                }
                DepSource::None => write!(f, "{}", &self.name.data,),
                DepSource::Workspace(_) => write!(f, "{}.workspace = true", &self.name.data,),
            },
        }
    }
}

#[derive(Debug)]
pub enum Target {
    Unknown(String),
}

#[derive(Debug)]
enum Source {
    Registry(String),
    Git {
        url: String,
        branch: Option<String>,
        tag: Option<String>,
        rev: String,
    },
    Other(String),
}

impl From<&str> for Source {
    fn from(value: &str) -> Self {
        if value.starts_with("registry+") {
            Source::Registry(value.replace("registry+", ""))
        } else if let Some(url_str) = value.strip_prefix("git+") {
            match Url::parse(url_str) {
                Ok(url) => {
                    let rev = url.fragment().unwrap_or("").to_string();

                    let branch = url
                        .query_pairs()
                        .find(|(key, _)| key == "branch")
                        .map(|(_, value)| value.to_string());

                    let tag = url
                        .query_pairs()
                        .find(|(key, _)| key == "tag")
                        .map(|(_, value)| value.to_string());

                    Source::Git {
                        url: url_str.to_string(),
                        branch,
                        tag,
                        rev,
                    }
                }
                Err(_) => Source::Other(value.to_string()),
            }
        } else {
            Source::Other(value.to_string())
        }
    }
}

#[derive(Debug, Clone)]
pub struct WithKey {
    key: RangeExclusive,
    pub value: Positioned<String>,
}

impl WithKey {
    pub fn new(range: RangeExclusive, value: Positioned<String>) -> Self {
        Self { key: range, value }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub enum DepSource {
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
    Workspace(RangeExclusive),
}

impl DepSource {
    pub fn registry(&self) -> Option<&str> {
        match self {
            DepSource::Version { registry, .. } => registry.as_ref().map(|v| v.value.data.as_str()),
            _ => None,
        }
    }
    pub fn range(&self) -> Option<RangeExclusive> {
        match self {
            DepSource::Version { value, .. } => {
                let mut r = RangeExclusive::from(&value.value);
                if let Some(v) = value.key {
                    r = r.join(&v)
                }
                Some(r)
            }
            Self::Workspace(r) => Some(r.clone()),
            _ => None,
        }
    }
    pub fn contains(&self, offset: usize) -> bool {
        match self {
            DepSource::Version { value, registry } => {
                value.value.contains(offset)
                    || value.key.map(|v| v.contains(offset)).unwrap_or_default()
            }
            _ => false,
        }
    }
    pub fn end(&self) -> Option<u32> {
        match self {
            DepSource::Version { value, .. } => Some(value.value.end),
            _ => None,
        }
    }

    pub fn version(&self) -> Option<&Positioned<String>> {
        match self {
            DepSource::Version { value, .. } => Some(&value.value),
            _ => None,
        }
    }

    pub fn set_workspace(&mut self, range: RangeExclusive) {
        *self = DepSource::Workspace(range)
    }
    pub fn set_version(&mut self, key: OptionalKey) {
        match self {
            DepSource::Version { value, .. } => {
                *value = key;
            }
            _ => {
                *self = DepSource::Version {
                    value: key,
                    registry: None,
                }
            }
        }
    }

    pub fn set_git(&mut self, key: WithKey) {
        match self {
            DepSource::Git { url, .. } => {
                *url = Some(key);
            }
            _ => {
                *self = DepSource::Git {
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
            DepSource::Git { branch, .. } => {
                *branch = Some(key);
            }
            _ => {
                *self = DepSource::Git {
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
            DepSource::Git { rev, .. } => {
                *rev = Some(key);
            }
            _ => {
                *self = DepSource::Git {
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
            DepSource::Git { tag, .. } => {
                *tag = Some(key);
            }
            _ => {
                *self = DepSource::Git {
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
            DepSource::Path(value) => {
                *value = key;
            }
            _ => {
                *self = DepSource::Path(key);
            }
        }
    }

    pub fn set_registry(&mut self, key: WithKey) {
        match self {
            DepSource::Version { registry, .. } => {
                *registry = Some(key);
            }
            _ => {
                *self = DepSource::Version {
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

impl FeatureArgKind {
    pub fn range(&self) -> RangeExclusive {
        match self {
            FeatureArgKind::CrateFeature(v) => RangeExclusive {
                start: v.start,
                end: v.end,
            },
            FeatureArgKind::DependencyFeature {
                dependency,
                feature,
            } => RangeExclusive {
                start: dependency.start,
                end: feature.end,
            },
            FeatureArgKind::Dependency(v) => RangeExclusive {
                start: v.start,
                end: v.end,
            },
        }
    }
}
