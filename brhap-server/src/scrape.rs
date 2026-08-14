//! Reading Required Items off a Workshop page.
//!
//! Fetch one page and parse its title and `RequiredItems` block. One page
//! yields both the item's name and its requirements, which is what lets the UI
//! expand the chain one click at a time.
//!
//! Identifies itself honestly in the user agent rather than posing as a
//! browser. Steam rate limits by address, so this is only ever called in
//! response to a deliberate user action.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::OnceLock;

use regex::Regex;

/// Identify the tool honestly rather than posing as a browser.
pub const USER_AGENT: &str = "brhap/0.1.0 (+arma3 mod launcher)";

const PAGE: &str = "https://steamcommunity.com/sharedfiles/filedetails/?id=";

/// A client whose request looks like the one Node sends.
///
/// This exists because ureq was the only client that ever received Steam's
/// unparseable page variant; curl and Node never did. Matching Node's header
/// set removes that variable rather than writing a parser for a page shape
/// only one client provoked.
pub fn client() -> Result<reqwest::blocking::Client, ScrapeError> {
    use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("*"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("cors"));

    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .build()
        .map_err(|error| ScrapeError::Http(error.to_string()))
}

/// What one Workshop page told us.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageInfo {
    /// Display name of the item, or None when the page has no title block.
    pub name: Option<String>,
    /// Ids this item declares as Required Items, in page order.
    pub requires: Vec<String>,
    /// Names of those required items, free with the same fetch.
    pub required_names: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum ScrapeError {
    /// Steam rate limited us. Retryable after a wait.
    RateLimited(String),
    Http(String),
    /// The response was not a Workshop item page. Recording "no dependencies"
    /// from a page we cannot identify would poison the cache with a fact we
    /// never learned, so this is an error rather than an empty result.
    Unrecognized(String),
}

impl fmt::Display for ScrapeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited(id) => {
                write!(formatter, "Steam rate limited the request for {id}")
            }
            Self::Http(message) => write!(formatter, "{message}"),
            Self::Unrecognized(id) => write!(
                formatter,
                "the page returned for {id} was not a Workshop item page, so nothing was recorded"
            ),
        }
    }
}

impl std::error::Error for ScrapeError {}

fn title_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<div class="workshopItemTitle"[^>]*>\s*(.*?)\s*</div>"#)
            .expect("static pattern")
    })
}

fn required_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<a[^>]+id=(\d+)"[^>]*>\s*<div class="requiredItem">\s*(.*?)\s*</div>"#)
            .expect("static pattern")
    })
}

fn decode(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Pull the item name and its Required Items out of a Workshop page.
pub fn parse_workshop_page(html: &str) -> PageInfo {
    let name = title_regex().captures(html).map(|capture| decode(&capture[1]));

    let mut requires: Vec<String> = Vec::new();
    let mut required_names = BTreeMap::new();
    for capture in required_regex().captures_iter(html) {
        let id = capture[1].to_string();
        if requires.contains(&id) {
            continue;
        }
        required_names.insert(id.clone(), decode(capture[2].trim()));
        requires.push(id);
    }

    PageInfo { name, requires, required_names }
}

/// Fetch and parse one Workshop page. Called only on an explicit user action.
pub fn fetch_workshop_page(id: &str) -> Result<PageInfo, ScrapeError> {
    let url = format!("{PAGE}{id}");
    let response = client()?
        .get(&url)
        .send()
        .map_err(|error| ScrapeError::Http(error.to_string()))?;

    if response.status().as_u16() == 429 {
        return Err(ScrapeError::RateLimited(id.to_string()));
    }
    if !response.status().is_success() {
        return Err(ScrapeError::Http(format!(
            "Workshop page for {id} returned HTTP {}",
            response.status().as_u16()
        )));
    }

    let body = response.text().map_err(|error| ScrapeError::Http(error.to_string()))?;

    let info = parse_workshop_page(&body);
    if !is_item_page(&body) {
        return Err(ScrapeError::Unrecognized(id.to_string()));
    }
    Ok(info)
}

/// Does this look like a Workshop item page at all?
///
/// A real item page always carries the title block. Without it we are looking
/// at an error page, an interstitial, or something else entirely, and an empty
/// requirements list would be a conclusion we did not earn.
pub fn is_item_page(html: &str) -> bool {
    title_regex().is_match(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same trimmed fixture the Node tests use, so both implementations
    /// are checked against identical markup.
    const MISERY: &str = include_str!("../../server/test/fixtures/misery.html");
    const NO_REQUIRED: &str = include_str!("../../server/test/fixtures/no-required-items.html");

    #[test]
    fn reads_the_title_and_every_required_item() {
        let info = parse_workshop_page(MISERY);
        assert_eq!(info.name.as_deref(), Some("Misery"));
        assert_eq!(info.requires, vec!["450814997", "463939057", "2888888564"]);
        assert_eq!(info.required_names.get("450814997").map(String::as_str), Some("CBA_A3"));
        assert_eq!(
            info.required_names.get("2888888564").map(String::as_str),
            Some("Advanced Equipment")
        );
    }

    #[test]
    fn returns_no_requirements_when_the_block_is_absent() {
        let info = parse_workshop_page(NO_REQUIRED);
        assert_eq!(info.name.as_deref(), Some("CBA_A3"));
        assert!(info.requires.is_empty());
    }

    #[test]
    fn decodes_html_entities_in_names() {
        let info = parse_workshop_page(r#"<div class="workshopItemTitle">Guns &amp; Ammo</div>"#);
        assert_eq!(info.name.as_deref(), Some("Guns & Ammo"));
    }

    #[test]
    fn recognizes_a_real_item_page() {
        assert!(is_item_page(MISERY));
        assert!(is_item_page(NO_REQUIRED));
    }

    /// The case that poisoned the cache: something that is not an item page
    /// must never read as "this mod has no dependencies".
    #[test]
    fn does_not_recognize_an_error_or_interstitial_page() {
        assert!(!is_item_page("<html><body>Too Many Requests</body></html>"));
        assert!(!is_item_page("<html><body><div class=\"error_ctn\">Oops</div></body></html>"));
        assert!(!is_item_page(""));
    }

    #[test]
    fn survives_a_page_with_no_title_block() {
        let info = parse_workshop_page("<html><body>nothing useful</body></html>");
        assert_eq!(info.name, None);
        assert!(info.requires.is_empty());
    }
}
