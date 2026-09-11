//! `robots.txt`: what a site allows a crawler to do.

use serde::{Deserialize, Serialize};

use crate::{Problem, SiteContext, SiteFile};

/// One directive inside a group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum Rule {
    Allow(String),
    Disallow(String),
    /// Seconds a crawler should wait between requests. Not in RFC 9309, but
    /// widely honoured, so it is emitted when asked for.
    CrawlDelay(u32),
}

/// One `User-agent` block: the agents it addresses and the rules for them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Group {
    /// The agents this block addresses. Empty means `*`.
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl Group {
    /// A block addressing every crawler.
    pub fn all() -> Self {
        Self::default()
    }

    /// A block addressing the named agents.
    pub fn for_agents(agents: &[&str]) -> Self {
        Self {
            agents: agents.iter().map(|a| a.to_string()).collect(),
            ..Self::default()
        }
    }

    pub fn allow(mut self, path: &str) -> Self {
        self.rules.push(Rule::Allow(path.to_string()));
        self
    }

    pub fn disallow(mut self, path: &str) -> Self {
        self.rules.push(Rule::Disallow(path.to_string()));
        self
    }

    pub fn crawl_delay(mut self, seconds: u32) -> Self {
        self.rules.push(Rule::CrawlDelay(seconds));
        self
    }
}

/// A site's crawl policy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Robots {
    #[serde(default)]
    pub groups: Vec<Group>,
    /// Sitemaps to advertise, as paths or absolute URLs. RFC 9309 makes this a
    /// robots.txt directive, which is why it lives here rather than in the
    /// sitemap document.
    #[serde(default)]
    pub sitemaps: Vec<String>,
}

impl Robots {
    /// The default policy: everything crawlable, with the admin excluded.
    ///
    /// The exclusion is added at render, so it holds however the groups are
    /// edited.
    pub fn permissive() -> Self {
        Self {
            groups: vec![Group::all()],
            sitemaps: Vec::new(),
        }
    }
}

impl SiteFile for Robots {
    fn path(&self) -> &str {
        "/robots.txt"
    }

    fn content_type(&self) -> &'static str {
        "text/plain; charset=utf-8"
    }

    fn render(&self, cx: &SiteContext) -> String {
        let mut out = String::new();
        // A group is always emitted, so an empty policy still excludes the admin
        // rather than producing a blank file that permits everything.
        let groups: Vec<&Group> = if self.groups.is_empty() {
            vec![&DEFAULT_GROUP]
        } else {
            self.groups.iter().collect()
        };

        for group in groups {
            if group.agents.is_empty() {
                out.push_str("User-agent: *\n");
            } else {
                for agent in &group.agents {
                    out.push_str(&format!("User-agent: {agent}\n"));
                }
            }
            for rule in &group.rules {
                match rule {
                    Rule::Allow(path) => out.push_str(&format!("Allow: {path}\n")),
                    Rule::Disallow(path) => out.push_str(&format!("Disallow: {path}\n")),
                    Rule::CrawlDelay(seconds) => out.push_str(&format!("Crawl-delay: {seconds}\n")),
                }
            }
            // Repeated in EVERY group, not just `*`. Under RFC 9309 a crawler
            // obeys only the most specific group that matches it, so a rule in
            // `*` does not reach a crawler that has a group of its own: putting
            // the exclusion only there would leave the panel crawlable by
            // exactly the crawlers that name themselves.
            if let Some(admin) = &cx.admin_path {
                out.push_str(&format!("Disallow: {}/\n", admin.trim_end_matches('/')));
            }
            out.push('\n');
        }

        for sitemap in &self.sitemaps {
            let url = if sitemap.starts_with("http://") || sitemap.starts_with("https://") {
                sitemap.clone()
            } else {
                cx.url(sitemap)
            };
            out.push_str(&format!("Sitemap: {url}\n"));
        }
        out
    }

    fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for (index, group) in self.groups.iter().enumerate() {
            for rule in &group.rules {
                let path = match rule {
                    Rule::Allow(p) | Rule::Disallow(p) => p,
                    Rule::CrawlDelay(_) => continue,
                };
                // A path rule is matched against the URL path, so one that does
                // not start with a slash silently matches nothing.
                if !path.starts_with('/') && path != "*" && !path.is_empty() {
                    problems.push(Problem::new(
                        &format!("groups[{index}]"),
                        format!("`{path}` should start with a slash"),
                    ));
                }
            }
        }
        problems
    }
}

/// Used when a policy declares no groups at all.
static DEFAULT_GROUP: Group = Group {
    agents: Vec::new(),
    rules: Vec::new(),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> SiteContext {
        SiteContext::new("https://acme.example").with_admin("/backoffice")
    }

    #[test]
    fn it_writes_a_group_per_block() {
        let robots = Robots {
            groups: vec![
                Group::all().disallow("/private/"),
                Group::for_agents(&["Googlebot", "Bingbot"]).allow("/"),
            ],
            sitemaps: Vec::new(),
        };
        let out = robots.render(&SiteContext::new("https://acme.example"));
        assert!(out.starts_with("User-agent: *\nDisallow: /private/\n"));
        assert!(out.contains("User-agent: Googlebot\nUser-agent: Bingbot\nAllow: /\n"));
    }

    /// The rule that is easy to get wrong: RFC 9309 says a crawler obeys only
    /// the most specific group matching it, so an exclusion in `*` does not
    /// reach a crawler that names itself. It has to be in every group.
    #[test]
    fn the_admin_is_excluded_from_every_group_not_just_the_wildcard() {
        let robots = Robots {
            groups: vec![Group::all(), Group::for_agents(&["Googlebot"])],
            sitemaps: Vec::new(),
        };
        let out = robots.render(&cx());
        assert_eq!(
            out.matches("Disallow: /backoffice/").count(),
            2,
            "once per group, or a self-naming crawler still reaches the panel: {out}"
        );
    }

    #[test]
    fn an_empty_policy_still_excludes_the_admin() {
        let out = Robots::default().render(&cx());
        assert!(out.contains("User-agent: *"));
        assert!(out.contains("Disallow: /backoffice/"));
    }

    #[test]
    fn a_static_site_has_nothing_to_exclude() {
        let out = Robots::permissive().render(&SiteContext::new("https://acme.example"));
        assert!(!out.contains("Disallow:"), "{out}");
    }

    #[test]
    fn a_sitemap_is_advertised_absolutely() {
        let robots = Robots {
            groups: vec![Group::all()],
            sitemaps: vec![
                "/sitemap.xml".to_string(),
                "https://cdn.acme.example/sitemap.xml".to_string(),
            ],
        };
        let out = robots.render(&SiteContext::new("https://acme.example"));
        // A path becomes absolute; an absolute URL is left alone.
        assert!(out.contains("Sitemap: https://acme.example/sitemap.xml"));
        assert!(out.contains("Sitemap: https://cdn.acme.example/sitemap.xml"));
    }

    #[test]
    fn a_path_without_a_slash_is_reported() {
        let robots = Robots {
            groups: vec![Group::all().disallow("private/")],
            sitemaps: Vec::new(),
        };
        let problems = robots.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("should start with a slash"));
        assert!(Robots::permissive().problems().is_empty());
    }
}
