use semver::Version;

use crate::ignore::parse_version;

pub fn select_candidate_tag(
    current_tag: &str,
    tags: &[String],
    is_ignored: impl Fn(&str) -> bool,
) -> Option<String> {
    let current_semver = parse_version(current_tag);
    if let Some(current) = current_semver.as_ref() {
        let mut best: Option<Version> = None;
        let mut best_tag: Option<String> = None;
        for tag in tags {
            if tag == current_tag || is_ignored(tag) {
                continue;
            }
            let Some(v) = parse_version(tag) else {
                continue;
            };
            if &v <= current {
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

    // If the current tag is unparseable (e.g. floating tags like `latest`), prefer the maximum
    // *parseable* version tag from the registry to avoid lexicographic pitfalls like
    // `v0.2.9` being considered greater than `v0.2.11`.
    if current_semver.is_none() {
        let mut best: Option<(Version, String)> = None;
        for tag in tags {
            if tag == current_tag || is_ignored(tag) {
                continue;
            }
            let Some(v) = parse_version(tag) else {
                continue;
            };

            if best
                .as_ref()
                .is_none_or(|(bv, bt)| &v > bv || (&v == bv && tag.as_str() > bt.as_str()))
            {
                best = Some((v, tag.clone()));
            }
        }

        if let Some((_v, tag)) = best {
            return Some(tag);
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

    #[test]
    fn floating_tag_picks_max_semver_instead_of_lexicographic() {
        let tags = vec!["v0.2.9".to_string(), "v0.2.11".to_string()];
        let picked = select_candidate_tag("latest", &tags, |_| false).unwrap();
        assert_eq!(picked, "v0.2.11");
    }

    #[test]
    fn prefix_numeric_tag_picks_higher() {
        let tags = vec![
            "15-alpine".to_string(),
            "15.6-alpine".to_string(),
            "trixie".to_string(),
        ];
        let picked = select_candidate_tag("15-alpine", &tags, |_| false).unwrap();
        assert_eq!(picked, "15.6-alpine");
    }

    #[test]
    fn prefix_numeric_tag_skips_non_numeric_variant() {
        let tags = vec![
            "7-alpine".to_string(),
            "7.1-alpine".to_string(),
            "windowsservercore".to_string(),
        ];
        let picked = select_candidate_tag("7-alpine", &tags, |_| false).unwrap();
        assert_eq!(picked, "7.1-alpine");
    }
}
