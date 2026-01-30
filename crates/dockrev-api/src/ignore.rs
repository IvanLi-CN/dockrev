use regex::Regex;
use semver::{Version, VersionReq};

#[derive(Clone, Debug)]
pub enum IgnoreKind {
    Exact,
    Prefix,
    Regex,
    Semver,
}

impl IgnoreKind {
    pub fn parse(input: &str) -> Self {
        match input {
            "prefix" => Self::Prefix,
            "regex" => Self::Regex,
            "semver" => Self::Semver,
            _ => Self::Exact,
        }
    }
}

#[derive(Clone, Debug)]
pub struct IgnoreRuleMatcher {
    pub kind: IgnoreKind,
    pub value: String,
}

impl IgnoreRuleMatcher {
    pub fn matches(&self, tag: &str) -> bool {
        match self.kind {
            IgnoreKind::Exact => tag == self.value,
            IgnoreKind::Prefix => tag.starts_with(&self.value),
            IgnoreKind::Regex => Regex::new(&self.value)
                .ok()
                .is_some_and(|re| re.is_match(tag)),
            IgnoreKind::Semver => {
                let Some(tag_ver) = parse_version(tag) else {
                    return false;
                };
                let Ok(req) = VersionReq::parse(&self.value) else {
                    return false;
                };
                req.matches(&tag_ver)
            }
        }
    }
}

pub fn parse_version(tag: &str) -> Option<Version> {
    let trimmed = tag.trim().strip_prefix('v').unwrap_or(tag.trim());
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(v) = Version::parse(trimmed) {
        return Some(v);
    }

    // Support "major.minor" and "major" tags by coercing to semver.
    let parts = trimmed.split('.').collect::<Vec<_>>();
    let coerced = match parts.len() {
        1 => format!("{}.0.0", parts[0]),
        2 => format!("{}.{}.0", parts[0], parts[1]),
        _ => return None,
    };
    Version::parse(&coerced).ok()
}

pub fn is_strict_semver(tag: &str) -> bool {
    let trimmed = tag.trim().strip_prefix('v').unwrap_or(tag.trim());
    if trimmed.is_empty() {
        return false;
    }
    Version::parse(trimmed).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact_prefix() {
        let m = IgnoreRuleMatcher {
            kind: IgnoreKind::Exact,
            value: "5.3".to_string(),
        };
        assert!(m.matches("5.3"));
        assert!(!m.matches("5.3.1"));

        let m = IgnoreRuleMatcher {
            kind: IgnoreKind::Prefix,
            value: "5.3.".to_string(),
        };
        assert!(m.matches("5.3.1"));
        assert!(!m.matches("5.4.0"));
    }

    #[test]
    fn matches_regex() {
        let m = IgnoreRuleMatcher {
            kind: IgnoreKind::Regex,
            value: "^5\\.3\\..+$".to_string(),
        };
        assert!(m.matches("5.3.1"));
        assert!(!m.matches("5.4.0"));
    }

    #[test]
    fn matches_semver_req() {
        let m = IgnoreRuleMatcher {
            kind: IgnoreKind::Semver,
            value: ">=5.3, <5.4".to_string(),
        };
        assert!(m.matches("5.3.1"));
        assert!(!m.matches("5.4.0"));
    }
}
