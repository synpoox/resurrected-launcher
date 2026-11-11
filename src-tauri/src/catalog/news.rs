//! The news file (`news.json`): a launcher-owned, untrusted text feed of
//! announcements and guides for every product (#80). Wire format:
//! `docs/specs/news-feed.md`. Separate from the release feed: nothing here touches
//! schema 3 or the packager.
//!
//! Parsing is plain over bytes. Everything is bounded: the file size (enforced by
//! the download), the item count and every field's length. Fields are text only;
//! links are `https` on [`LINK_HOSTS`] only. Unknown fields are ignored, and items
//! of kinds the feed does not show (release notes live in Patch Notes, and any
//! future kind) are dropped. Anything else invalid rejects the whole file.

use serde::Deserialize;

use crate::contract::ProductId;

/// Schema version of `news.json` this launcher reads.
pub const NEWS_SCHEMA_VERSION: u32 = 1;

/// Largest `news.json` the launcher downloads.
pub const MAX_NEWS_SIZE: u64 = 64 * 1024;

/// Most items one file may hold (all products, all kinds).
pub const MAX_NEWS_ITEMS: usize = 50;

/// Most items the feed shows per product (`docs/specs/launcher-ui.md`).
pub const NEWS_SHOWN: usize = 3;

const MAX_TITLE: usize = 120;
const MAX_SUMMARY: usize = 200;
const MAX_URL: usize = 512;
const MAX_KIND: usize = 32;

/// The only hosts a news link may open (owner decision on #80). Exact match; add
/// `projectd2r.com` once the website is hosted.
pub const LINK_HOSTS: [&str; 3] = ["github.com", "discord.gg", "discord.com"];

/// Why a news file was rejected. The UI shows it as offline either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsError(pub String);

impl std::fmt::Display for NewsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid news.json: {}", self.0)
    }
}

impl std::error::Error for NewsError {}

fn bad(why: impl Into<String>) -> NewsError {
    NewsError(why.into())
}

/// One shown item: an announcement or a guide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsEntry {
    pub product_id: ProductId,
    /// Display tag, from the kind ("Announcement", "Guide").
    pub tag: &'static str,
    pub title: String,
    /// Optional short description under the title (#139), plain text.
    pub summary: Option<String>,
    /// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ`.
    pub published: String,
    /// `https` on [`LINK_HOSTS`].
    pub url: String,
}

/// A validated news file: the shown items of every product.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct News {
    items: Vec<NewsEntry>,
}

// No `deny_unknown_fields`: unknown fields are ignored (forward compatible).
#[derive(Deserialize)]
struct RawNews {
    schema_version: u32,
    items: Vec<RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    product_id: String,
    kind: String,
    title: String,
    #[serde(default)]
    summary: Option<String>,
    published: String,
    url: String,
}

impl News {
    /// Validates `bytes` (already capped at [`MAX_NEWS_SIZE`] by the download).
    pub fn parse(bytes: &[u8]) -> Result<Self, NewsError> {
        if bytes.len() as u64 > MAX_NEWS_SIZE {
            return Err(bad(format!("larger than {MAX_NEWS_SIZE} bytes")));
        }
        let raw: RawNews = serde_json::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
        if raw.schema_version != NEWS_SCHEMA_VERSION {
            return Err(bad(format!(
                "schema_version {} (this launcher reads {NEWS_SCHEMA_VERSION})",
                raw.schema_version
            )));
        }
        if raw.items.len() > MAX_NEWS_ITEMS {
            return Err(bad(format!("more than {MAX_NEWS_ITEMS} items")));
        }
        let mut items = Vec::new();
        for (i, item) in raw.items.into_iter().enumerate() {
            let at = |field: &str| format!("items[{i}].{field}");
            let product_id = product_id(&at("product_id"), item.product_id)?;
            let kind = text(&at("kind"), item.kind, MAX_KIND)?;
            let title = text(&at("title"), item.title, MAX_TITLE)?;
            let summary = item
                .summary
                .map(|s| text(&at("summary"), s, MAX_SUMMARY))
                .transpose()?;
            let published = date(&at("published"), item.published)?;
            let url = link(&at("url"), item.url)?;
            if let Some(tag) = tag_of(&kind) {
                items.push(NewsEntry {
                    product_id,
                    tag,
                    title,
                    summary,
                    published,
                    url,
                });
            }
        }
        Ok(Self { items })
    }

    /// `product`'s items, newest first, at most [`NEWS_SHOWN`].
    pub fn for_product(&self, product: &ProductId) -> Vec<&NewsEntry> {
        let mut mine: Vec<_> = self
            .items
            .iter()
            .filter(|e| &e.product_id == product)
            .collect();
        // A date-only item sorts as the start of its day (UTC); equal instants keep
        // file order (the sort is stable).
        mine.sort_by(|a, b| sort_key(&b.published).cmp(&sort_key(&a.published)));
        mine.truncate(NEWS_SHOWN);
        mine
    }
}

/// `published` as `YYYY-MM-DDTHH:MM:SSZ`; a date-only value is `T00:00:00Z`.
fn sort_key(published: &str) -> std::borrow::Cow<'_, str> {
    if published.len() == 10 {
        format!("{published}T00:00:00Z").into()
    } else {
        published.into()
    }
}

/// The shown kinds; `None` drops the item (release notes, unknown kinds).
fn tag_of(kind: &str) -> Option<&'static str> {
    match kind {
        "announcement" => Some("Announcement"),
        "guide" => Some("Guide"),
        _ => None,
    }
}

/// The catalog's product id rule ([`super::check::ident`]).
fn product_id(field: &str, s: String) -> Result<ProductId, NewsError> {
    super::check::ident(field, s)
        .map(ProductId)
        .map_err(|e| bad(e.to_string()))
}

/// Plain text: non-empty after trimming, at most `max` characters, and no
/// control or bidirectional-override characters.
fn text(field: &str, s: String, max: usize) -> Result<String, NewsError> {
    let s = s.trim().to_owned();
    if s.is_empty() || s.chars().count() > max {
        return Err(bad(format!("{field}: must be 1 to {max} characters")));
    }
    let bidi = |c: char| matches!(c, '\u{200E}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    if s.chars().any(|c| c.is_control() || bidi(c)) {
        return Err(bad(format!("{field}: control characters are not allowed")));
    }
    Ok(s)
}

/// `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ`, with real month, day and time ranges.
fn date(field: &str, s: String) -> Result<String, NewsError> {
    let invalid = || bad(format!("{field}: not an ISO date (YYYY-MM-DD)"));
    let b = s.as_bytes();
    let num = |r: std::ops::Range<usize>| -> Option<u32> {
        let part = b.get(r)?;
        if part.iter().all(u8::is_ascii_digit) {
            std::str::from_utf8(part).ok()?.parse().ok()
        } else {
            None
        }
    };
    if b.len() != 10 && b.len() != 20 {
        return Err(invalid());
    }
    let (year, month, day) = (num(0..4), num(5..7), num(8..10));
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return Err(invalid());
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if b[4] != b'-' || b[7] != b'-' || year < 2000 || day == 0 || day > days {
        return Err(invalid());
    }
    if b.len() == 20 {
        let time = (num(11..13), num(14..16), num(17..19));
        let (Some(h), Some(m), Some(sec)) = time else {
            return Err(invalid());
        };
        if b[10] != b'T'
            || b[13] != b':'
            || b[16] != b':'
            || b[19] != b'Z'
            || h > 23
            || m > 59
            || sec > 59
        {
            return Err(invalid());
        }
    }
    Ok(s)
}

/// What a link may hold after its host: an ASCII subset of RFC 3986 with no
/// `,` (explorer.exe splits its command line on commas), quote, backslash,
/// whitespace, `^`, `` ` ``, `|`, `<`, `>`, `{` or `}`.
fn link_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-._~/?#[]@!$&'()*+;=%".contains(&b)
}

/// `https` on a public host (the catalog's rule) that is on [`LINK_HOSTS`],
/// followed only by [`link_char`]s.
pub fn link(field: &str, url: String) -> Result<String, NewsError> {
    if url.len() > MAX_URL {
        return Err(bad(format!("{field}: longer than {MAX_URL} characters")));
    }
    let (url, host) = super::check::https_host(field, url).map_err(|e| bad(e.to_string()))?;
    if !LINK_HOSTS.contains(&host.as_str()) {
        return Err(bad(format!("{field}: {host} is not an allowed link host")));
    }
    // `https_host` accepted the host, so it is ASCII and this index is in bounds.
    let rest = &url.as_bytes()["https://".len() + host.len()..];
    if !rest.iter().copied().all(link_char) {
        return Err(bad(format!(
            "{field}: characters outside the allowed URL set"
        )));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        product: &str,
        kind: &str,
        title: &str,
        published: &str,
        url: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "product_id": product, "kind": kind, "title": title,
            "published": published, "url": url,
        })
    }

    fn file(items: Vec<serde_json::Value>) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "schema_version": 1, "items": items })).unwrap()
    }

    const GH: &str = "https://github.com/synpoox/pd2-resurrected/discussions/1";

    fn one(v: serde_json::Value) -> Result<News, NewsError> {
        News::parse(&file(vec![v]))
    }

    #[test]
    fn shows_announcements_and_guides_newest_first_three_at_most() {
        let news = News::parse(&file(vec![
            item("pd2r", "guide", "Old", "2026-01-01", GH),
            item("pd2r", "announcement", "Newest", "2026-11-05T10:00:00Z", GH),
            item("pd2r", "release_notes", "Notes", "2026-11-06", GH),
            item("pd2r", "poll", "Future kind", "2026-11-07", GH),
            item("pd2r", "guide", "Mid", "2026-06-01", GH),
            item("pd2r", "guide", "Newer", "2026-11-01", GH),
            item(
                "project-forever",
                "guide",
                "Other product",
                "2026-11-08",
                GH,
            ),
        ]))
        .unwrap();
        let shown = news.for_product(&ProductId("pd2r".into()));
        let titles: Vec<_> = shown.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Newest", "Newer", "Mid"]);
        assert_eq!(shown[0].tag, "Announcement");
        assert_eq!(shown[1].tag, "Guide");
        assert!(news.for_product(&ProductId("project-09".into())).is_empty());
    }

    #[test]
    fn same_day_items_sort_by_start_of_day_then_file_order() {
        let news = News::parse(&file(vec![
            item("pd2r", "guide", "Day A", "2026-11-01", GH),
            item("pd2r", "guide", "Morning", "2026-11-01T08:00:00Z", GH),
            item("pd2r", "guide", "Day B", "2026-11-01", GH),
        ]))
        .unwrap();
        let shown = news.for_product(&ProductId("pd2r".into()));
        let titles: Vec<_> = shown.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Morning", "Day A", "Day B"]);
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let mut v = item("pd2r", "guide", "T", "2026-11-01", GH);
        v["image"] = "https://evil.example/x.png".into();
        v["html"] = "<script>".into();
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "items": [v], "extra": { "nested": true },
        }))
        .unwrap();
        assert_eq!(News::parse(&bytes).unwrap().items.len(), 1);
    }

    #[test]
    fn links_must_be_https_on_the_allowlist() {
        for url in [
            "https://github.com/x",
            "https://discord.gg/mGpHT26Xak",
            "https://discord.com/channels/1",
            "https://GitHub.com/x",
            "https://github.com/synpoox/pd2-resurrected/wiki/A_b~c?q=1&r=%20#top",
            "https://discord.com/channels/1/2",
        ] {
            assert!(
                one(item("pd2r", "guide", "T", "2026-11-01", url)).is_ok(),
                "{url}"
            );
        }
        for url in [
            "http://github.com/x",
            "https://evil.example/x",
            "https://github.com.evil.example/x",
            "https://gist.github.com/x",
            "https://github.com@evil.example/x",
            "https://github.com:8443/x",
            "javascript:alert(1)",
            "file:///C:/Windows",
            "https://github.com/../x",
            "github.com/x",
            r"https://github.com/x,C:\Windows\System32\calc.exe",
            "https://github.com/x\"y",
            r"https://github.com/x\y",
            "https://github.com/x^y",
            "https://github.com/x`y",
            "https://github.com/x|y",
            "https://github.com/x<y>",
            "https://github.com/x{y}",
            "https://github.com/caf\u{e9}",
            "https://github.com,evil/x",
        ] {
            assert!(
                one(item("pd2r", "guide", "T", "2026-11-01", url)).is_err(),
                "{url}"
            );
        }
        let long = format!("https://github.com/{}", "a".repeat(MAX_URL));
        assert!(one(item("pd2r", "guide", "T", "2026-11-01", &long)).is_err());
    }

    #[test]
    fn titles_are_bounded_plain_text() {
        assert!(one(item(
            "pd2r",
            "guide",
            &"x".repeat(MAX_TITLE),
            "2026-11-01",
            GH
        ))
        .is_ok());
        for title in [
            "",
            "   ",
            &"x".repeat(MAX_TITLE + 1),
            "line\nbreak",
            "bell\u{7}",
            "rtl\u{202E}gnp.exe",
        ] {
            assert!(
                one(item("pd2r", "guide", title, "2026-11-01", GH)).is_err(),
                "{title:?}"
            );
        }
        let trimmed = one(item("pd2r", "guide", "  Hi  ", "2026-11-01", GH)).unwrap();
        assert_eq!(trimmed.items[0].title, "Hi");
    }

    #[test]
    fn summaries_are_optional_bounded_plain_text() {
        let none = one(item("pd2r", "guide", "T", "2026-11-01", GH)).unwrap();
        assert_eq!(none.items[0].summary, None);
        let mut v = item("pd2r", "guide", "T", "2026-11-01", GH);
        v["summary"] = "  Back up before you patch.  ".into();
        assert_eq!(
            one(v).unwrap().items[0].summary.as_deref(),
            Some("Back up before you patch.")
        );
        let mut max = item("pd2r", "guide", "T", "2026-11-01", GH);
        max["summary"] = "x".repeat(MAX_SUMMARY).into();
        assert!(one(max).is_ok());
        for summary in [
            "".to_owned(),
            "   ".to_owned(),
            "x".repeat(MAX_SUMMARY + 1),
            "line\nbreak".to_owned(),
            "rtl\u{202E}gnp.exe".to_owned(),
        ] {
            let mut v = item("pd2r", "guide", "T", "2026-11-01", GH);
            v["summary"] = summary.clone().into();
            assert!(one(v).is_err(), "{summary:?}");
        }
        let mut wrong = item("pd2r", "guide", "T", "2026-11-01", GH);
        wrong["summary"] = 3.into();
        assert!(one(wrong).is_err());
    }

    #[test]
    fn dates_are_iso() {
        for d in ["2026-11-01", "2024-02-29", "2026-11-01T23:59:59Z"] {
            assert!(one(item("pd2r", "guide", "T", d, GH)).is_ok(), "{d}");
        }
        for d in [
            "2026-13-01",
            "2026-02-29",
            "2026-11-31",
            "2026-11-00",
            "1999-01-01",
            "2026/11/01",
            "2026-11-1",
            "Nov 1 2026",
            "2026-11-01T24:00:00Z",
            "2026-11-01T10:00:00+02:00",
            "+026-11-01",
            "",
        ] {
            assert!(one(item("pd2r", "guide", "T", d, GH)).is_err(), "{d}");
        }
    }

    #[test]
    fn product_ids_and_kinds_are_checked() {
        for p in ["", "PD2R", "pd2r/../x", "-pd2r", &"a".repeat(65)] {
            assert!(
                one(item(p, "guide", "T", "2026-11-01", GH)).is_err(),
                "{p:?}"
            );
        }
        assert!(one(item("pd2r", "", "T", "2026-11-01", GH)).is_err());
    }

    /// `news/fixtures`: the same files `scripts/check-news.py` runs through
    /// `news/news.schema.json`, so the schema and this parser cannot drift.
    #[test]
    fn news_fixtures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../news/fixtures");
        for (dir, valid) in [("valid", true), ("invalid", false)] {
            let mut count = 0;
            for entry in std::fs::read_dir(root.join(dir)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|e| e == "json") {
                    count += 1;
                    let parsed = News::parse(&std::fs::read(&path).unwrap());
                    assert_eq!(parsed.is_ok(), valid, "{}: {parsed:?}", path.display());
                }
            }
            assert!(count > 0, "no fixtures in news/fixtures/{dir}");
        }
    }

    #[test]
    fn malformed_files_are_rejected() {
        let too_many = file(
            (0..=MAX_NEWS_ITEMS)
                .map(|_| item("pd2r", "guide", "T", "2026-11-01", GH))
                .collect(),
        );
        let oversized = vec![b' '; MAX_NEWS_SIZE as usize + 1];
        for bytes in [
            b"not json".to_vec(),
            b"[]".to_vec(),
            br#"{"items": []}"#.to_vec(),
            br#"{"schema_version": 2, "items": []}"#.to_vec(),
            br#"{"schema_version": 1}"#.to_vec(),
            br#"{"schema_version": 1, "items": [{"product_id": "pd2r"}]}"#.to_vec(),
            br#"{"schema_version": 1, "items": [{"product_id": 1, "kind": "guide", "title": "T", "published": "2026-11-01", "url": "https://github.com/x"}]}"#.to_vec(),
            too_many,
            oversized,
        ] {
            assert!(News::parse(&bytes).is_err(), "{}", String::from_utf8_lossy(&bytes[..bytes.len().min(80)]));
        }
        assert_eq!(
            News::parse(br#"{"schema_version": 1, "items": []}"#).unwrap(),
            News::default()
        );
    }
}
