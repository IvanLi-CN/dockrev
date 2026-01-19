use semver::Version;

use crate::ignore::parse_version;

pub fn select_candidate_tag(
    current_tag: &str,
    tags: &[String],
    is_ignored: impl Fn(&str) -> bool,
) -> Option<String> {
    let current_semver = parse_version(current_tag);
    if let Some(current) = current_semver {
        let mut best: Option<Version> = None;
        let mut best_tag: Option<String> = None;
        for tag in tags {
            if tag == current_tag || is_ignored(tag) {
                continue;
            }
            let Some(v) = parse_version(tag) else {
                continue;
            };
            if v <= current {
                continue;
            }
            if best.as_ref().is_none_or(|b| &v > b) {
                best = Some(v);
                best_tag = Some(tag.clone());
            }
        }
        if best_tag.is_some() {
            return best_tag;
        }
    }

    // Fallback: lexicographic maximum (still ignoring current and ignored tags).
    let mut best: Option<&str> = None;
    for tag in tags {
        if tag == current_tag || is_ignored(tag) {
            continue;
        }
        if best.is_none_or(|b| tag.as_str() > b) {
            best = Some(tag);
        }
    }
    best.map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_picks_higher() {
        let tags = vec!["5.2".to_string(), "5.3".to_string(), "5.10".to_string()];
        let picked = select_candidate_tag("5.2", &tags, |_| false).unwrap();
        assert_eq!(picked, "5.10");
    }

    #[test]
    fn semver_respects_ignore() {
        let tags = vec!["5.2".to_string(), "5.3".to_string(), "5.4".to_string()];
        let picked = select_candidate_tag("5.2", &tags, |t| t == "5.4").unwrap();
        assert_eq!(picked, "5.3");
    }

    #[test]
    fn fallback_lexicographic() {
        let tags = vec!["alpha".to_string(), "beta".to_string()];
        let picked = select_candidate_tag("alpha", &tags, |_| false).unwrap();
        assert_eq!(picked, "beta");
    }
}
