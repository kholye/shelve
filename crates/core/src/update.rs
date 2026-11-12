//! Reading GitHub's answer about the latest release, and telling whether it is newer.

use serde::Deserialize;

/// GitHub's API; `latest` leaves out drafts and pre-releases.
pub const RELEASES_HOST: &str = "api.github.com";
pub const LATEST_RELEASE_PATH: &str = "/repos/kholye/shelve/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Without the leading "v" of the tag, like "0.3.0".
    pub version: String,
    /// The release page, where the download is.
    pub url: String,
}

#[derive(Deserialize)]
struct Answer {
    tag_name: String,
    html_url: String,
}

/// The release in GitHub's JSON answer, if it has one.
pub fn parse_latest(json: &str) -> Option<Release> {
    let a: Answer = serde_json::from_str(json).ok()?;
    let version = a.tag_name.trim().trim_start_matches(['v', 'V']).to_owned();
    if numbers(&version).is_none() || !a.html_url.starts_with("https://github.com/") {
        return None;
    }
    Some(Release {
        version,
        url: a.html_url,
    })
}

/// True if `latest` is a later version than `current`. Unreadable versions are never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (numbers(latest), numbers(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// "1.2.3" as [1, 2, 3]; missing parts count as 0, so "1.2" equals "1.2.0".
fn numbers(v: &str) -> Option<[u64; 3]> {
    let mut out = [0; 3];
    let mut parts = v.split('.');
    for slot in &mut out {
        match parts.next() {
            Some(p) => *slot = p.parse().ok()?,
            None => break,
        }
    }
    (parts.next().is_none() && !v.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_compares_numbers_not_text() {
        assert!(is_newer("0.3.0", "0.2.0"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("1.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
    }

    #[test]
    fn odd_versions_are_never_newer() {
        assert!(!is_newer("0.3.0-beta", "0.2.0"));
        assert!(!is_newer("", "0.2.0"));
        assert!(!is_newer("1.2.3.4", "0.2.0"));
        assert!(!is_newer("0.3.0", "dev"));
    }

    #[test]
    fn reads_tag_and_page_from_github_answer() {
        let json = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/kholye/shelve/releases/tag/v0.3.0","draft":false,"assets":[]}"#;
        assert_eq!(
            parse_latest(json),
            Some(Release {
                version: "0.3.0".into(),
                url: "https://github.com/kholye/shelve/releases/tag/v0.3.0".into(),
            })
        );
    }

    #[test]
    fn rejects_answers_without_a_release() {
        assert_eq!(parse_latest(r#"{"message":"Not Found"}"#), None);
        assert_eq!(parse_latest("not json"), None);
        assert_eq!(
            parse_latest(r#"{"tag_name":"nightly","html_url":"https://github.com/x"}"#),
            None
        );
        // The link is opened in the browser, so only GitHub pages are taken.
        assert_eq!(
            parse_latest(r#"{"tag_name":"v9.0.0","html_url":"https://example.com/"}"#),
            None
        );
    }
}
