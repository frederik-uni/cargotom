use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use url::Url;

use super::version::RustVersion;

#[derive(Debug, Deserialize)]
/// parsed information of the Cargo.lock file
pub struct CargoLockRaw {
    package: Vec<PackageRaw>,
}

impl CargoLockRaw {
    /// converts the raw data into a more usable format
    pub fn packages(&self) -> HashMap<String, Vec<Package>> {
        let mut out = HashMap::new();
        for package in &self.package {
            let by_name: &mut Vec<Package> = out.entry(package.name.clone()).or_default();
            by_name.push(Package {
                version: RustVersion::try_from(package.version.as_str()).unwrap(),
                source: package.source.as_ref().map(|v| v.as_str().into()),
                checksum: package.checksum.clone(),
                dependencies: package.dependencies.clone(),
            });
        }
        out
    }
}

#[derive(Debug, Deserialize)]
struct PackageRaw {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    dependencies: Option<Vec<Value>>,
}

#[derive(Debug)]
/// A package parsed from the Cargo.lock file
pub struct Package {
    /// version of the package
    pub version: RustVersion,
    /// source of the package
    source: Option<Source>,
    /// checksum of the package
    checksum: Option<String>,
    /// dependencies of the package
    dependencies: Option<Vec<Value>>,
}

impl Package {
    pub fn version(&self) -> &RustVersion {
        &self.version
    }

    pub fn branch(&self) -> Option<&String> {
        match &self.source {
            Some(Source::Git { branch, .. }) => branch.as_ref(),
            _ => None,
        }
    }

    pub fn rev(&self) -> Option<&String> {
        match &self.source {
            Some(Source::Git { rev, .. }) => Some(rev),
            _ => None,
        }
    }

    pub fn tag(&self) -> Option<&String> {
        match &self.source {
            Some(Source::Git { tag, .. }) => tag.as_ref(),
            _ => None,
        }
    }

    pub fn is_registry(&self) -> bool {
        match self.source {
            Some(Source::Registry(_)) => true,
            _ => false,
        }
    }
    pub fn is_git(&self) -> bool {
        match self.source {
            Some(Source::Git { .. }) => true,
            _ => false,
        }
    }
    pub fn label(&self) -> String {
        match &self.source {
            Some(Source::Git { rev, .. }) => format!("{} #{}", self.version, &rev[..7]),
            _ => self.version.to_string(),
        }
    }
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
