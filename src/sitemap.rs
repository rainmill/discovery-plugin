//! `sitemap.xml`: which of a site's URLs it advertises, and how.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Problem, SiteContext, SiteFile};

/// The sitemap protocol's cap on URLs in one file. Beyond this a site needs a
/// sitemap index, which is a later document.
pub const MAX_URLS: usize = 50_000;

/// How often a URL is said to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeFreq {
    Always,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
    Never,
}

impl ChangeFreq {
    fn as_str(self) -> &'static str {
        match self {
            ChangeFreq::Always => "always",
            ChangeFreq::Hourly => "hourly",
            ChangeFreq::Daily => "daily",
            ChangeFreq::Weekly => "weekly",
            ChangeFreq::Monthly => "monthly",
            ChangeFreq::Yearly => "yearly",
            ChangeFreq::Never => "never",
        }
    }
}

/// The same page in another language, for `hreflang`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternate {
    pub hreflang: String,
    pub href: String,
}

/// One advertised URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// A path, made absolute at render, or an absolute URL left as it is.
    pub loc: String,
    #[serde(default)]
    pub lastmod: Option<DateTime<Utc>>,
    #[serde(default)]
    pub changefreq: Option<ChangeFreq>,
    /// 0.0 to 1.0. Omitted when absent rather than defaulted, because the
    /// protocol's own default is 0.5 and saying so adds nothing.
    #[serde(default)]
    pub priority: Option<f32>,
    #[serde(default)]
    pub alternates: Vec<Alternate>,
}

impl Entry {
    pub fn new(loc: impl Into<String>) -> Self {
        Self {
            loc: loc.into(),
            lastmod: None,
            changefreq: None,
            priority: None,
            alternates: Vec::new(),
        }
    }

    pub fn lastmod(mut self, at: DateTime<Utc>) -> Self {
        self.lastmod = Some(at);
        self
    }

    pub fn changefreq(mut self, freq: ChangeFreq) -> Self {
        self.changefreq = Some(freq);
        self
    }

    pub fn priority(mut self, priority: f32) -> Self {
        self.priority = Some(priority);
        self
    }
}

/// The URLs a site advertises.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Sitemap {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl Sitemap {
    /// A sitemap of exactly the pages a static generator wrote.
    ///
    /// The one case where the content genuinely is derived: the generator knows
    /// every page because it just produced them.
    pub fn from_paths<I, S>(paths: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            entries: paths.into_iter().map(|p| Entry::new(p)).collect(),
        }
    }
}

impl SiteFile for Sitemap {
    fn path(&self) -> &str {
        "/sitemap.xml"
    }

    fn content_type(&self) -> &'static str {
        "application/xml; charset=utf-8"
    }

    fn render(&self, cx: &SiteContext) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"");
        let localised = self.entries.iter().any(|e| !e.alternates.is_empty());
        if localised {
            xml.push_str(" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\"");
        }
        xml.push_str(">\n");

        for entry in self.entries.iter().take(MAX_URLS) {
            // A URL under the admin is never advertised, whatever put it here.
            if let Some(admin) = &cx.admin_path {
                if entry.loc.starts_with(admin) {
                    continue;
                }
            }
            xml.push_str("  <url>\n");
            xml.push_str(&format!(
                "    <loc>{}</loc>\n",
                escape(&absolute(cx, &entry.loc))
            ));
            if let Some(lastmod) = entry.lastmod {
                xml.push_str(&format!(
                    "    <lastmod>{}</lastmod>\n",
                    lastmod.format("%Y-%m-%d")
                ));
            }
            if let Some(freq) = entry.changefreq {
                xml.push_str(&format!("    <changefreq>{}</changefreq>\n", freq.as_str()));
            }
            if let Some(priority) = entry.priority {
                xml.push_str(&format!("    <priority>{priority:.1}</priority>\n"));
            }
            for alternate in &entry.alternates {
                xml.push_str(&format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>\n",
                    escape(&alternate.hreflang),
                    escape(&absolute(cx, &alternate.href))
                ));
            }
            xml.push_str("  </url>\n");
        }
        xml.push_str("</urlset>\n");
        xml
    }

    fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.entries.len() > MAX_URLS {
            problems.push(Problem::new(
                "entries",
                format!(
                    "{} URLs is over the {MAX_URLS} a sitemap may hold; the rest are omitted",
                    self.entries.len()
                ),
            ));
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.priority.is_some_and(|p| !(0.0..=1.0).contains(&p)) {
                problems.push(Problem::new(
                    &format!("entries[{index}]"),
                    "priority runs from 0.0 to 1.0",
                ));
            }
        }
        problems
    }
}

/// A path against the site's origin; an absolute URL unchanged.
fn absolute(cx: &SiteContext, loc: &str) -> String {
    if loc.starts_with("http://") || loc.starts_with("https://") {
        loc.to_string()
    } else {
        cx.url(loc)
    }
}

/// XML-escapes a value. A URL can carry `&` from a query string, which would
/// otherwise make the document malformed.
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cx() -> SiteContext {
        SiteContext::new("https://acme.example")
    }

    #[test]
    fn paths_become_absolute_urls() {
        let out = Sitemap::from_paths(["/", "/features/"]).render(&cx());
        assert!(out.contains("<loc>https://acme.example/</loc>"));
        assert!(out.contains("<loc>https://acme.example/features/</loc>"));
    }

    #[test]
    fn an_absolute_url_is_left_alone() {
        let map = Sitemap {
            entries: vec![Entry::new("https://cdn.acme.example/a")],
        };
        assert!(map
            .render(&cx())
            .contains("<loc>https://cdn.acme.example/a</loc>"));
    }

    #[test]
    fn optional_parts_appear_only_when_set() {
        let bare = Sitemap::from_paths(["/"]).render(&cx());
        assert!(!bare.contains("<lastmod>"));
        assert!(!bare.contains("<changefreq>"));
        assert!(!bare.contains("<priority>"));
        // The xhtml namespace is declared only when something uses it.
        assert!(!bare.contains("xmlns:xhtml"));

        let full = Sitemap {
            entries: vec![Entry::new("/")
                .lastmod(Utc.with_ymd_and_hms(2026, 9, 10, 12, 0, 0).unwrap())
                .changefreq(ChangeFreq::Weekly)
                .priority(0.8)],
        }
        .render(&cx());
        assert!(full.contains("<lastmod>2026-09-10</lastmod>"));
        assert!(full.contains("<changefreq>weekly</changefreq>"));
        assert!(full.contains("<priority>0.8</priority>"));
    }

    #[test]
    fn alternates_declare_the_namespace_they_need() {
        let map = Sitemap {
            entries: vec![Entry {
                alternates: vec![Alternate {
                    hreflang: "de".to_string(),
                    href: "/de/".to_string(),
                }],
                ..Entry::new("/")
            }],
        };
        let out = map.render(&cx());
        assert!(out.contains("xmlns:xhtml=\"http://www.w3.org/1999/xhtml\""));
        assert!(out.contains(
            r#"<xhtml:link rel="alternate" hreflang="de" href="https://acme.example/de/"/>"#
        ));
    }

    /// A query string carries `&`, which would make the XML malformed.
    #[test]
    fn urls_are_escaped() {
        let map = Sitemap {
            entries: vec![Entry::new("/search?a=1&b=2")],
        };
        let out = map.render(&cx());
        assert!(out.contains("a=1&amp;b=2"), "{out}");
        assert!(!out.contains("a=1&b=2"));
    }

    /// Whatever put a URL in the list, the panel is never advertised.
    #[test]
    fn admin_urls_are_never_advertised() {
        let map = Sitemap::from_paths(["/", "/backoffice/users"]);
        let out = map.render(&cx().with_admin("/backoffice"));
        assert!(out.contains("<loc>https://acme.example/</loc>"));
        assert!(!out.contains("/backoffice"), "{out}");
    }

    #[test]
    fn the_protocol_cap_is_honoured_and_reported() {
        let map = Sitemap::from_paths((0..MAX_URLS + 10).map(|i| format!("/p/{i}")));
        assert_eq!(map.render(&cx()).matches("<loc>").count(), MAX_URLS);
        assert!(map.problems()[0].message.contains("over the"));
    }

    #[test]
    fn a_priority_outside_the_range_is_reported() {
        let map = Sitemap {
            entries: vec![Entry::new("/").priority(1.5)],
        };
        assert_eq!(map.problems().len(), 1);
        assert!(Sitemap::from_paths(["/"]).problems().is_empty());
    }
}
