use std::{cmp::Ordering, fmt::Display, num::ParseIntError};

#[derive(Debug, Clone)]
pub struct RustVersion {
    major: Option<u32>,
    minor: Option<u32>,
    patch: Option<VersionString>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct VersionString(String);

impl VersionString {
    fn int_or_string(&self) -> Vec<IntOrString> {
        let mut out = vec![];
        let mut builder = vec![];
        let mut number = false;
        let build = |number, builder: &mut Vec<char>, out: &mut Vec<IntOrString>| {
            if !builder.is_empty() {
                let val = builder.drain(..).collect::<String>();
                match number {
                    true => out.push(IntOrString::Int(val.parse().unwrap())),
                    false => out.push(IntOrString::String(val)),
                }
            }
        };
        for char in self.0.chars() {
            if number != char.is_ascii_digit() {
                build(number, &mut builder, &mut out);
                number = !number;
            }
            builder.push(char);
        }
        build(number, &mut builder, &mut out);
        out
    }
}

#[derive(PartialEq, Eq)]
enum IntOrString {
    Int(u64),
    String(String),
}

impl PartialOrd for IntOrString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntOrString {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (IntOrString::Int(a), IntOrString::Int(b)) => a.cmp(b),
            (IntOrString::Int(_), IntOrString::String(_)) => Ordering::Greater,
            (IntOrString::String(_), IntOrString::Int(_)) => Ordering::Less,
            (IntOrString::String(a), IntOrString::String(b)) => a.cmp(b),
        }
    }
}

impl Ord for VersionString {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut iter_a = self.int_or_string().into_iter();
        let mut iter_b = other.int_or_string().into_iter();
        loop {
            match (iter_a.next(), iter_b.next()) {
                (Some(elem_a), Some(elem_b)) => {
                    let cmp_result = elem_a.cmp(&elem_b);
                    if cmp_result != Ordering::Equal {
                        return cmp_result;
                    }
                }
                (Some(_), None) => return Ordering::Greater,
                (None, Some(_)) => return Ordering::Less,
                (None, None) => return Ordering::Equal,
            }
        }
    }
}

impl PartialOrd for VersionString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for VersionString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl From<String> for VersionString {
    fn from(value: String) -> Self {
        Self(value)
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
                    write!(f, "{}.{}.{}", major, minor, path)
                } else {
                    write!(f, "{}.{}", major, minor)
                }
            } else {
                write!(f, "{}", major)
            }
        } else {
            write!(f, "*")
        }
    }
}

impl TryFrom<&str> for RustVersion {
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let items = value.splitn(3, '.').collect::<Vec<_>>();
        let mut se = Self {
            major: None,
            minor: None,
            patch: None,
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
                se.patch = Some(items[2].to_string().into());
            }
            _ => {}
        };
        Ok(se)
    }

    type Error = ParseIntError;
}

impl PartialEq for RustVersion {
    fn eq(&self, other: &Self) -> bool {
        if ((other.major.is_some() && self.major == other.major) || other.major.is_none())
            && ((other.minor.is_some() && self.minor == other.minor) || other.minor.is_none())
            && ((other.patch.is_some() && self.patch == other.patch) || other.patch.is_none())
        {
            return true;
        }
        false
    }
}
impl Eq for RustVersion {}
