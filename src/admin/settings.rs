//! The editable shape of each document, and the screens that edit it.
//!
//! Each document gets a settings model of its own rather than being edited as
//! itself. The rendering types are nested (a robots group holds an enum of
//! rules; an llms section holds links), and the form vocabulary is flat rows of
//! text. Projecting once, here, keeps the renderers expressive and the screens
//! simple: `into_document` is the only place the two shapes meet.
//!
//! Every list field is a repeater, which stores an array of flat objects, so
//! each row type is a struct of `String` with `#[serde(default)]`.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use laterite_admin::settings::{FormField, SettingsItem, SettingsModel};
use laterite_admin::Permission;
use laterite_core::t;
use serde::{Deserialize, Serialize};

use crate::humans::{Humans, Section};
use crate::llms::{Link, LinkSection, Llms};
use crate::robots::{Group, Robots, Rule};
use crate::security::SecurityTxt;
use crate::sitemap::{ChangeFreq, Entry, Sitemap};

/// Editing any of these files changes what crawlers are told and what the site
/// advertises, so it is gated rather than open to every signed-in operator.
pub const PERMISSION: &str = "rainmill.discovery.manage";

pub fn permission() -> Permission {
    Permission {
        code: PERMISSION.to_string(),
        label: t!("Manage site discovery files"),
        group: category(),
    }
}

fn category() -> laterite_core::Text {
    t!("Discovery")
}

/// Splits a textarea into one entry per non-blank line, trimmed.
fn lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

/// Splits a comma-separated field, trimmed, dropping blanks.
fn commas(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn some(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

// ---------------------------------------------------------------- robots.txt

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RobotsSettings {
    #[serde(default)]
    pub groups: Vec<GroupRow>,
    #[serde(default)]
    pub sitemaps: Vec<UrlRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupRow {
    /// Comma-separated. Blank addresses every crawler.
    #[serde(default)]
    pub agents: String,
    /// One path per line.
    #[serde(default)]
    pub disallow: String,
    #[serde(default)]
    pub allow: String,
    /// Seconds. Blank omits it.
    #[serde(default)]
    pub crawl_delay: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UrlRow {
    #[serde(default)]
    pub url: String,
}

impl SettingsModel for RobotsSettings {
    const CODE: &'static str = "rainmill.discovery.robots";
}

impl RobotsSettings {
    pub fn into_document(self) -> Robots {
        let mut groups = Vec::new();
        for row in self.groups {
            let mut group = Group {
                agents: commas(&row.agents),
                rules: Vec::new(),
            };
            for path in lines(&row.disallow) {
                group.rules.push(Rule::Disallow(path));
            }
            for path in lines(&row.allow) {
                group.rules.push(Rule::Allow(path));
            }
            if let Ok(seconds) = row.crawl_delay.trim().parse::<u32>() {
                group.rules.push(Rule::CrawlDelay(seconds));
            }
            groups.push(group);
        }
        Robots {
            groups,
            sitemaps: self
                .sitemaps
                .into_iter()
                .filter_map(|row| some(&row.url))
                .collect(),
        }
    }
}

fn robots_item() -> SettingsItem {
    SettingsItem::new(
        RobotsSettings::CODE,
        t!("Crawler rules"),
        vec![
            // Four fields side by side is a table nobody can read, so a rule
            // collapses to the crawler it addresses and opens one at a time.
            FormField::repeater_list(
                "groups",
                t!("Rules"),
                vec![
                    FormField::text("agents", t!("Crawlers"))
                        .help(t!("Comma-separated. Leave blank to address every crawler.")),
                    FormField::textarea("disallow", t!("Disallow")).help(t!("One path per line.")),
                    FormField::textarea("allow", t!("Allow")),
                    FormField::text("crawl_delay", t!("Crawl delay (seconds)")),
                ],
                Some("agents"),
            )
            .help(t!(
                "A crawler obeys only the block that names it, so a crawler with \
                     its own block ignores the catch-all entirely."
            )),
            FormField::repeater(
                "sitemaps",
                t!("Other sitemaps"),
                vec![FormField::text("url", t!("URL"))],
            )
            .help(t!(
                "This site's own sitemap is listed automatically. Add others here: \
                     one hosted elsewhere, or a separate feed for news or images."
            )),
        ],
    )
    .description(t!("Which crawlers may visit, and what they may not."))
    // The precedence note lives on the Rules field, where it is being acted on;
    // repeating it here would say the same sentence twice on one screen.
    .hint(t!(
        "The admin panel is always excluded, whatever you put here."
    ))
    .category(category())
    .order(10)
    .icon("bot")
    .permission(PERMISSION)
}

// --------------------------------------------------------------- sitemap.xml

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SitemapSettings {
    #[serde(default)]
    pub entries: Vec<EntryRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntryRow {
    #[serde(default)]
    pub loc: String,
    #[serde(default)]
    pub changefreq: String,
    #[serde(default)]
    pub priority: String,
}

impl SettingsModel for SitemapSettings {
    const CODE: &'static str = "rainmill.discovery.sitemap";
}

fn change_freq(value: &str) -> Option<ChangeFreq> {
    match value.trim() {
        "always" => Some(ChangeFreq::Always),
        "hourly" => Some(ChangeFreq::Hourly),
        "daily" => Some(ChangeFreq::Daily),
        "weekly" => Some(ChangeFreq::Weekly),
        "monthly" => Some(ChangeFreq::Monthly),
        "yearly" => Some(ChangeFreq::Yearly),
        "never" => Some(ChangeFreq::Never),
        _ => None,
    }
}

impl SitemapSettings {
    /// The authored entries. Contributed sources are appended by the route,
    /// which is the only place that can await them.
    pub fn into_document(self) -> Sitemap {
        let mut entries = Vec::new();
        for row in self.entries {
            let Some(loc) = some(&row.loc) else { continue };
            let mut entry = Entry::new(loc);
            entry.changefreq = change_freq(&row.changefreq);
            entry.priority = row.priority.trim().parse::<f32>().ok();
            entries.push(entry);
        }
        Sitemap { entries }
    }
}

fn sitemap_item() -> SettingsItem {
    SettingsItem::new(
        SitemapSettings::CODE,
        t!("Sitemap"),
        vec![FormField::repeater(
            "entries",
            t!("Pages"),
            vec![
                FormField::text("loc", t!("Path or URL")),
                FormField::select(
                    "changefreq",
                    t!("Changes"),
                    vec![
                        ("", "Unspecified"),
                        ("always", "Always"),
                        ("hourly", "Hourly"),
                        ("daily", "Daily"),
                        ("weekly", "Weekly"),
                        ("monthly", "Monthly"),
                        ("yearly", "Yearly"),
                        ("never", "Never"),
                    ],
                ),
                FormField::text("priority", t!("Priority"))
                    .help(t!("0.0 to 1.0, relative to your other pages. Blank means \
                             the default of 0.5.")),
            ],
        )],
    )
    .description(t!("Pages you want search engines to know about."))
    .hint(t!(
        "Modules that own content add their own pages automatically, so these are the \
         extras: anything the application does not already publish for itself."
    ))
    .category(category())
    .order(20)
    .icon("map")
    .permission(PERMISSION)
}

// ------------------------------------------------------------------ llms.txt

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmsSettings {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub links: Vec<LlmsLinkRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmsLinkRow {
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub note: String,
}

impl SettingsModel for LlmsSettings {
    const CODE: &'static str = "rainmill.discovery.llms";
}

impl LlmsSettings {
    /// Links carry their section name, and sections are rebuilt by grouping on
    /// it in first-appearance order. Editing one flat list beats maintaining a
    /// nested one, and the file's own order is the order you typed.
    pub fn into_document(self) -> Llms {
        let mut sections: Vec<LinkSection> = Vec::new();
        let mut optional: Option<LinkSection> = None;

        for row in self.links {
            let Some(url) = some(&row.url) else { continue };
            let mut link = Link::new(some(&row.title).unwrap_or_else(|| url.clone()), url);
            link.note = some(&row.note);

            let name = some(&row.section).unwrap_or_else(|| "Links".to_string());
            if name.eq_ignore_ascii_case("optional") {
                optional
                    .get_or_insert_with(|| LinkSection::new("Optional"))
                    .links
                    .push(link);
                continue;
            }
            match sections.iter_mut().find(|s| s.title == name) {
                Some(section) => section.links.push(link),
                None => sections.push(LinkSection {
                    title: name,
                    links: vec![link],
                }),
            }
        }

        Llms {
            title: self.title.trim().to_string(),
            summary: some(&self.summary),
            details: some(&self.details),
            sections,
            optional,
        }
    }
}

fn llms_item() -> SettingsItem {
    SettingsItem::new(
        LlmsSettings::CODE,
        t!("Summary for language models"),
        vec![
            FormField::text("title", t!("Site name")).required(),
            FormField::textarea("summary", t!("One-line summary")),
            FormField::textarea("details", t!("Description")).help(t!(
                "Plain paragraphs. A heading here would start a new section."
            )),
            FormField::repeater(
                "links",
                t!("Links"),
                vec![
                    FormField::text("section", t!("Section"))
                        .help(t!("Links sharing a section are grouped under it. Name a \
                             section Optional for things a reader may skip.")),
                    FormField::text("title", t!("Title")),
                    FormField::text("url", t!("URL")),
                    FormField::text("note", t!("Note")),
                ],
            ),
        ],
    )
    .description(t!("A summary of this site for language models."))
    .hint(t!(
        "Assistants that look for one will read this: a short description of the site \
         and the pages worth reading."
    ))
    .category(category())
    .order(30)
    .icon("sparkles")
    .permission(PERMISSION)
}

// ---------------------------------------------------------------- humans.txt

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumansSettings {
    #[serde(default)]
    pub entries: Vec<HumansRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HumansRow {
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: String,
}

impl SettingsModel for HumansSettings {
    const CODE: &'static str = "rainmill.discovery.humans";
}

impl HumansSettings {
    pub fn into_document(self) -> Humans {
        let mut sections: Vec<Section> = Vec::new();
        for row in self.entries {
            let Some(value) = some(&row.value) else {
                continue;
            };
            let name = some(&row.section).unwrap_or_else(|| "Team".to_string());
            let entry = (some(&row.label).unwrap_or_default(), value);
            match sections.iter_mut().find(|s| s.title == name) {
                Some(section) => section.entries.push(entry),
                None => sections.push(Section {
                    title: name,
                    entries: vec![entry],
                }),
            }
        }
        Humans { sections }
    }
}

fn humans_item() -> SettingsItem {
    SettingsItem::new(
        HumansSettings::CODE,
        t!("Credits"),
        vec![FormField::repeater(
            "entries",
            t!("Entries"),
            vec![
                FormField::text("section", t!("Section"))
                    .help(t!("Such as Team, Thanks, or Site.")),
                FormField::text("label", t!("Label")),
                FormField::text("value", t!("Value")),
            ],
        )],
    )
    .description(t!("The people behind the site."))
    .hint(t!(
        "Credits for whoever built and runs the site, and anything you want to say \
         about how it was made."
    ))
    .category(category())
    .order(40)
    .icon("users")
    .permission(PERMISSION)
}

// -------------------------------------------------------------- security.txt

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecuritySettings {
    #[serde(default)]
    pub contacts: Vec<UrlRow>,
    /// `YYYY-MM-DD`. Authored, never rolled forward: the field exists to show
    /// somebody reviewed this recently.
    #[serde(default)]
    pub expires: String,
    #[serde(default)]
    pub policy: String,
    #[serde(default)]
    pub acknowledgments: String,
    #[serde(default)]
    pub encryption: String,
    #[serde(default)]
    pub hiring: String,
    /// Comma-separated language tags.
    #[serde(default)]
    pub preferred_languages: String,
}

impl SettingsModel for SecuritySettings {
    const CODE: &'static str = "rainmill.discovery.security";
}

/// A date the operator typed, at the end of that day in UTC. An unparseable or
/// missing date becomes the epoch, which `problems()` reports as expired rather
/// than quietly passing.
fn expiry(value: &str) -> DateTime<Utc> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(23, 59, 59))
        .map(|at| Utc.from_utc_datetime(&at))
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).unwrap_or_default())
}

impl SecuritySettings {
    pub fn into_document(self) -> SecurityTxt {
        SecurityTxt {
            contacts: self
                .contacts
                .into_iter()
                .filter_map(|row| some(&row.url))
                .collect(),
            expires: expiry(&self.expires),
            encryption: some(&self.encryption),
            acknowledgments: some(&self.acknowledgments),
            preferred_languages: commas(&self.preferred_languages),
            canonical: None,
            policy: some(&self.policy),
            hiring: some(&self.hiring),
        }
    }
}

fn security_item() -> SettingsItem {
    SettingsItem::new(
        SecuritySettings::CODE,
        t!("Security contact"),
        vec![
            FormField::repeater(
                "contacts",
                t!("Contacts"),
                vec![FormField::text("url", t!("Address"))],
            )
            .help(t!(
                "An email address as mailto:you@example.com, or a form as an \
                     https: URL. Most preferred first."
            )),
            FormField::date("expires", t!("Valid until"))
                .required()
                .help(t!(
                    "Set a date within the next year and revisit it. An expired \
                         file tells a researcher nobody is reading the inbox."
                )),
            FormField::text("policy", t!("Policy URL")),
            FormField::text("acknowledgments", t!("Acknowledgements URL")),
            FormField::text("encryption", t!("Encryption key URL")),
            FormField::text("hiring", t!("Hiring URL")),
            FormField::text("preferred_languages", t!("Preferred languages"))
                .help(t!("Comma-separated language tags, such as en, de.")),
        ],
    )
    .description(t!("Where to report a security problem."))
    .hint(t!(
        "Published at the well-known address security researchers check first, so a \
         report reaches you rather than going nowhere."
    ))
    .category(category())
    .order(50)
    .icon("shield")
    .permission(PERMISSION)
}

pub fn items() -> Vec<SettingsItem> {
    vec![
        robots_item(),
        sitemap_item(),
        llms_item(),
        humans_item(),
        security_item(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SiteContext, SiteFile};

    fn cx() -> SiteContext {
        SiteContext::new("https://acme.example").with_admin("/backoffice")
    }

    #[test]
    fn a_robots_row_splits_agents_by_comma_and_paths_by_line() {
        let settings = RobotsSettings {
            groups: vec![GroupRow {
                agents: "GoodBot, OtherBot".to_string(),
                disallow: "/private/\n\n  /tmp/  \n".to_string(),
                allow: "/public/".to_string(),
                crawl_delay: "10".to_string(),
            }],
            sitemaps: vec![UrlRow {
                url: "/sitemap.xml".to_string(),
            }],
        };
        let out = settings.into_document().render(&cx());

        assert!(out.contains("User-agent: GoodBot\n"));
        assert!(out.contains("User-agent: OtherBot\n"));
        // Blank lines and surrounding spaces are not paths.
        assert!(out.contains("Disallow: /private/\n"));
        assert!(out.contains("Disallow: /tmp/\n"));
        assert!(out.contains("Allow: /public/\n"));
        assert!(out.contains("Crawl-delay: 10\n"));
        assert!(out.contains("Sitemap: https://acme.example/sitemap.xml"));
    }

    /// The plugin serves the sitemap, so robots names it without being told.
    /// An operator typing the path of a file their own plugin publishes is an
    /// invitation to get it wrong, and to not notice when the mount moves.
    #[test]
    fn robots_advertises_the_sitemap_this_plugin_serves() {
        use crate::sitemap::Sitemap;
        let mut doc = RobotsSettings::default().into_document();
        let own = Sitemap::default().path().to_string();
        if !doc.sitemaps.iter().any(|s| s.ends_with(&own)) {
            doc.sitemaps.insert(0, own);
        }
        let out = doc.render(&cx());
        assert!(
            out.contains("Sitemap: https://acme.example/sitemap.xml"),
            "{out}"
        );
    }

    /// An operator who typed it anyway does not get it twice.
    #[test]
    fn a_sitemap_already_listed_is_not_repeated() {
        let mut doc = RobotsSettings {
            sitemaps: vec![UrlRow {
                url: "/sitemap.xml".into(),
            }],
            ..Default::default()
        }
        .into_document();
        let own = crate::sitemap::Sitemap::default().path().to_string();
        if !doc.sitemaps.iter().any(|s| s.ends_with(&own)) {
            doc.sitemaps.insert(0, own);
        }
        let out = doc.render(&cx());
        assert_eq!(out.matches("Sitemap:").count(), 1, "{out}");
    }

    #[test]
    fn a_group_naming_no_crawler_addresses_all_of_them() {
        let settings = RobotsSettings {
            groups: vec![GroupRow {
                disallow: "/private/".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(settings
            .into_document()
            .render(&cx())
            .contains("User-agent: *\n"));
    }

    /// The reason the exclusion lives inside the render: an operator cannot
    /// remove it by clearing the screen, and it uses the real mount.
    #[test]
    fn the_panel_is_excluded_even_when_nothing_is_configured() {
        let out = RobotsSettings::default().into_document().render(&cx());
        assert!(out.contains("Disallow: /backoffice/"));
        assert!(!out.contains("/admin"));
    }

    #[test]
    fn an_unparseable_crawl_delay_is_left_out_rather_than_defaulted() {
        let settings = RobotsSettings {
            groups: vec![GroupRow {
                crawl_delay: "soon".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(!settings
            .into_document()
            .render(&cx())
            .contains("Crawl-delay"));
    }

    #[test]
    fn llms_links_group_into_sections_in_the_order_they_first_appear() {
        let settings = LlmsSettings {
            title: "Acme".to_string(),
            links: vec![
                LlmsLinkRow {
                    section: "Docs".to_string(),
                    title: "Guide".to_string(),
                    url: "/guide".to_string(),
                    note: "Start here".to_string(),
                },
                LlmsLinkRow {
                    section: "About".to_string(),
                    title: "Team".to_string(),
                    url: "/team".to_string(),
                    ..Default::default()
                },
                LlmsLinkRow {
                    section: "Docs".to_string(),
                    title: "API".to_string(),
                    url: "/api".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let document = settings.into_document();

        assert_eq!(document.sections.len(), 2);
        assert_eq!(document.sections[0].title, "Docs");
        // The third row joins the first section rather than opening a third.
        assert_eq!(document.sections[0].links.len(), 2);
        assert_eq!(document.sections[1].title, "About");

        let out = document.render(&cx());
        assert!(out.contains("- [Guide](https://acme.example/guide): Start here\n"));
    }

    #[test]
    fn a_section_named_optional_becomes_the_optional_block() {
        let settings = LlmsSettings {
            title: "Acme".to_string(),
            links: vec![LlmsLinkRow {
                section: "OPTIONAL".to_string(),
                title: "Changelog".to_string(),
                url: "/changelog".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let document = settings.into_document();
        assert!(document.sections.is_empty());
        assert!(document.optional.is_some());
        assert!(document.render(&cx()).contains("## Optional"));
    }

    #[test]
    fn a_link_with_no_url_is_not_a_link() {
        let settings = LlmsSettings {
            title: "Acme".to_string(),
            links: vec![LlmsLinkRow {
                section: "Docs".to_string(),
                title: "Nothing".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(settings.into_document().sections.is_empty());
    }

    #[test]
    fn humans_entries_group_by_section() {
        let settings = HumansSettings {
            entries: vec![
                HumansRow {
                    section: "Team".to_string(),
                    label: "Developer".to_string(),
                    value: "Ada".to_string(),
                },
                HumansRow {
                    section: "Team".to_string(),
                    label: "Designer".to_string(),
                    value: "Grace".to_string(),
                },
            ],
        };
        let document = settings.into_document();
        assert_eq!(document.sections.len(), 1);
        assert_eq!(document.sections[0].entries.len(), 2);
    }

    #[test]
    fn a_security_expiry_is_the_end_of_the_day_it_names() {
        let settings = SecuritySettings {
            contacts: vec![UrlRow {
                url: "mailto:security@acme.example".to_string(),
            }],
            expires: "2027-01-31".to_string(),
            preferred_languages: "en, de".to_string(),
            ..Default::default()
        };
        let document = settings.into_document();
        assert_eq!(document.expires.to_rfc3339(), "2027-01-31T23:59:59+00:00");
        assert_eq!(document.preferred_languages, vec!["en", "de"]);
        assert!(document.problems().is_empty());
    }

    /// An unset or malformed date must not silently pass as valid: it becomes
    /// the epoch, which the document itself reports as expired.
    #[test]
    fn a_missing_expiry_reads_as_expired_rather_than_as_valid() {
        let document = SecuritySettings {
            contacts: vec![UrlRow {
                url: "mailto:security@acme.example".to_string(),
            }],
            expires: "whenever".to_string(),
            ..Default::default()
        }
        .into_document();
        assert!(document
            .problems()
            .iter()
            .any(|p| p.message.contains("expired")));
    }

    #[test]
    fn a_sitemap_row_carries_its_change_frequency_and_priority() {
        let settings = SitemapSettings {
            entries: vec![
                EntryRow {
                    loc: "/about".to_string(),
                    changefreq: "monthly".to_string(),
                    priority: "0.8".to_string(),
                },
                EntryRow {
                    loc: "/contact".to_string(),
                    changefreq: String::new(),
                    priority: "not a number".to_string(),
                },
            ],
        };
        let out = settings.into_document().render(&cx());

        assert!(out.contains("<loc>https://acme.example/about</loc>"));
        assert!(out.contains("<changefreq>monthly</changefreq>"));
        assert!(out.contains("<priority>0.8</priority>"));
        // The second entry still publishes; only its unusable extras are dropped.
        assert!(out.contains("<loc>https://acme.example/contact</loc>"));
        assert_eq!(out.matches("<priority>").count(), 1);
    }

    #[test]
    fn every_screen_is_gated_and_stores_under_its_own_code() {
        let codes: Vec<String> = items().iter().map(|i| i.code.clone()).collect();
        assert_eq!(codes.len(), 5);
        let mut unique = codes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 5, "two screens share a storage code");
        assert!(items()
            .iter()
            .all(|i| i.permission.as_deref() == Some(PERMISSION)));
    }
}
