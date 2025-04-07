//TODO: refactor
use std::{cmp::Ordering, fmt::Display, num::ParseIntError};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustVersion {
    major: Option<u32>,
    minor: Option<u32>,
    patch: Option<u32>,
    pre: Option<String>,
    build: Option<String>,
}

impl RustVersion {
    pub fn mahor(&self) -> Option<u32> {
        self.major
    }
    pub fn minor(&self) -> Option<u32> {
        self.minor
    }
    pub fn patch(&self) -> Option<&u32> {
        self.patch.as_ref()
    }

    pub fn is_pre_release(&self) -> bool {
        self.pre.is_some()
    }
}

impl Ord for RustVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.major, &other.major) {
            (Some(self_major), Some(other_major)) => self_major.cmp(other_major),
            (Some(_), None) => Ordering::Less, // Some major < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some major
            (None, None) => Ordering::Equal,
        }
        .then_with(|| match (&self.minor, &other.minor) {
            (Some(self_minor), Some(other_minor)) => self_minor.cmp(other_minor),
            (Some(_), None) => Ordering::Less, // Some minor < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some minor
            (None, None) => Ordering::Equal,
        })
        .then_with(|| match (&self.patch, &other.patch) {
            (Some(self_patch), Some(other_patch)) => self_patch.cmp(other_patch),
            (Some(_), None) => Ordering::Less, // Some patch < None (None is treated as highest)
            (None, Some(_)) => Ordering::Greater, // None > Some patch
            (None, None) => Ordering::Equal,
        })
        .then_with(|| match (&self.pre, &other.pre) {
            (Some(self_pre), Some(other_pre)) => self_pre.cmp(other_pre),
            (Some(_), None) => Ordering::Less, // pre-release < no pre-release
            (None, Some(_)) => Ordering::Greater, // no pre-release > pre-release
            (None, None) => Ordering::Equal,
        })
        .then_with(|| match (&self.build, &other.build) {
            (Some(self_build), Some(other_build)) => self_build.cmp(other_build),
            (Some(_), None) => Ordering::Less, // Arbitrary decision; could also be Equal
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        })
    }
}

impl PartialOrd for RustVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for RustVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(major) = &self.major {
            if let Some(minor) = &self.minor {
                if let Some(path) = &self.patch {
                    write!(f, "{}.{}.{}", major, minor, path)?;
                } else {
                    write!(f, "{}.{}", major, minor)?;
                }
            } else {
                write!(f, "{}", major)?;
            }
        } else {
            write!(f, "*")?;
        }
        if let Some(pre) = &self.pre {
            write!(f, "-{}", pre)?;
        }

        if let Some(build) = &self.build {
            write!(f, "+{}", build)?;
        }
        Ok(())
    }
}

impl TryFrom<&str> for RustVersion {
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (version_part, build) = match value.split_once('+') {
            Some((ver, b)) => (ver, Some(b.to_string())),
            None => (value, None),
        };

        let (core_part, pre) = match version_part.split_once('-') {
            Some((core, p)) => (core, Some(p.to_string())),
            None => (version_part, None),
        };
        let items = core_part.splitn(3, '.').collect::<Vec<_>>();

        let mut se = Self {
            major: None,
            minor: None,
            patch: None,
            pre,
            build,
        };
        match items.len() {
            0 => {}
            1 => {
                se.major = Some(items[0].to_string().parse()?);
            }
            2 => {
                se.major = Some(items[0].to_string().parse()?);
                se.minor = Some(items[1].to_string().parse()?);
            }
            3 => {
                se.major = Some(items[0].to_string().parse()?);
                se.minor = Some(items[1].to_string().parse()?);
                se.patch = Some(items[2].to_string().parse()?);
            }
            _ => {}
        };
        Ok(se)
    }

    type Error = ParseIntError;
}

impl PartialEq for RustVersion {
    fn eq(&self, other: &Self) -> bool {
        fn field_eq<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
            match (a, b) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                (None, Some(_)) => true,
                (Some(_), None) => true,
            }
        }

        field_eq(&self.major, &other.major)
            && field_eq(&self.minor, &other.minor)
            && field_eq(&self.patch, &other.patch)
            && field_eq(&self.pre, &other.pre)
            && field_eq(&self.build, &other.build)
    }
}
impl Eq for RustVersion {}
