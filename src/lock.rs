use std::collections::HashMap;

use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;

use crate::rust_version::RustVersion;

#[derive(Debug, Deserialize)]
struct CargoLock {
    package: Vec<Package>,
}

impl CargoLock {
    pub fn packages(&self) -> HashMap<String, Vec<PackageParsed>> {
        let mut out = HashMap::new();
        for package in &self.package {
            let by_name: &mut Vec<PackageParsed> = out.entry(package.name.clone()).or_default();
            by_name.push(PackageParsed {
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
struct Package {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
    dependencies: Option<Vec<Value>>,
}

#[derive(Debug)]
struct PackageParsed {
    version: RustVersion,
    source: Option<Source>,
    checksum: Option<String>,
    dependencies: Option<Vec<Value>>,
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
