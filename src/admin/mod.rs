//! The admin half: settings screens, public routes, and the seam other modules
//! answer through.
//!
//! Everything here is behind the `admin` feature. The documents themselves know
//! nothing about it: they render from a [`SiteContext`](crate::SiteContext), so
//! the same definitions serve a static build with none of this compiled in.

mod routes;
mod settings;

use std::sync::Arc;

use laterite_core::strata::async_trait;
use laterite_core::{CoreError, CoreResult, Db, MigrationSet, Module, ModuleId, Registry};

pub use settings::{
    HumansSettings, LlmsSettings, RobotsSettings, SecuritySettings, SitemapSettings,
};

use crate::sitemap::Entry;

/// This plugin's identity. Never change it: migrations and permissions are
/// tracked against it.
pub const MODULE_ID: &str = "rainmill.discovery";

/// A module that answers for URLs it owns.
///
/// The sitemap knows nothing about content types. A module that publishes pages
/// implements this and contributes a [`UrlSourceReg`] from its `register`; the
/// sitemap route collects every one of them at render. That inverts the usual
/// direction: the sitemap does not go looking for content, content declares
/// itself.
///
/// Returning [`Entry`] rather than a bare string is deliberate: only the owner
/// knows when its page last changed, how often it changes, and what its
/// translations are.
#[async_trait]
pub trait UrlSource: Send + Sync + 'static {
    /// The URLs this module wants advertised. A path is made absolute at render;
    /// an absolute URL is left alone, so a module may point at another host.
    async fn urls(&self, db: &Db) -> Vec<Entry>;
}

/// A registered source, named for diagnostics rather than lookup: every source
/// contributes to the one sitemap, so nothing resolves them by name.
pub struct UrlSourceReg {
    pub name: String,
    pub source: Arc<dyn UrlSource>,
}

impl UrlSourceReg {
    pub fn new(name: impl Into<String>, source: Arc<dyn UrlSource>) -> Self {
        Self {
            name: name.into(),
            source,
        }
    }
}

/// Every `lat` plugin exposes `module()`, so the generated manifest collects it
/// without naming the type.
pub fn module() -> Box<dyn Module> {
    Box::new(DiscoveryModule)
}

/// The Rainmill Discovery plugin.
pub struct DiscoveryModule;

#[async_trait]
impl Module for DiscoveryModule {
    fn id(&self) -> ModuleId {
        ModuleId::new(MODULE_ID)
    }

    /// The documents render from YAML under `plugins/rainmill.discovery/`
    /// in a static website, with no admin.
    fn supports_static(&self) -> bool {
        true
    }

    /// The five documents for a static build. `robots.yaml` and `sitemap.yaml`
    /// are optional and default to a permissive policy and the pages the build
    /// wrote; `llms.yaml`, `humans.yaml` and `security.yaml` are written only
    /// when their file exists. `robots.yaml` is a [`RobotsDefinition`]; the
    /// others hold the document as the crate's types deserialise.
    async fn site_files(
        &self,
        cx: &laterite_core::SiteFilesCx<'_>,
    ) -> CoreResult<Vec<Box<dyn crate::SiteFile>>> {
        use crate::humans::Humans;
        use crate::llms::Llms;
        use crate::robots::Robots;
        use crate::security::SecurityTxt;
        use crate::sitemap::Sitemap;
        use crate::SiteFile as _;

        let mut out: Vec<Box<dyn crate::SiteFile>> = Vec::new();
        let mut robots: Robots = read_yaml::<RobotsDefinition>(&cx.files.join("robots.yaml"))?
            .map(RobotsDefinition::into_document)
            .unwrap_or_else(Robots::permissive);
        let own = Sitemap::default().path().to_string();
        if !robots.sitemaps.iter().any(|s| s.ends_with(&own)) {
            robots.sitemaps.insert(0, own);
        }
        out.push(Box::new(robots));

        let mut sitemap: Sitemap = read_yaml(&cx.files.join("sitemap.yaml"))?.unwrap_or_default();
        for page in cx.pages.iter().filter(|p| advertised(p)) {
            let loc = if *page == "/" {
                "/".to_string()
            } else {
                format!("{}/", page.trim_end_matches('/'))
            };
            if !sitemap.entries.iter().any(|e| e.loc == loc) {
                sitemap.entries.push(Entry::new(loc));
            }
        }
        for reg in cx.registry.contributions::<UrlSourceReg>() {
            sitemap.entries.extend(reg.item.source.urls(cx.db).await);
        }
        out.push(Box::new(sitemap));

        if let Some(llms) = read_yaml::<Llms>(&cx.files.join("llms.yaml"))? {
            out.push(Box::new(llms));
        }
        if let Some(humans) = read_yaml::<Humans>(&cx.files.join("humans.yaml"))? {
            out.push(Box::new(humans));
        }
        if let Some(security) = read_yaml::<SecurityTxt>(&cx.files.join("security.yaml"))? {
            out.push(Box::new(security));
        }
        Ok(out)
    }

    /// None of its own: every document is a settings value, stored in the
    /// framework's settings table.
    fn migrations(&self) -> MigrationSet {
        MigrationSet::new(MODULE_ID, Vec::new())
    }

    fn register(&self, registry: &mut Registry) {
        use laterite_admin::AdminRegistry;

        registry.add_permission(settings::permission());
        for item in settings::items() {
            registry.add_settings(item);
        }
        for route in routes::public_routes() {
            registry.add_public_route(route);
        }
    }
}

/// `robots.yaml` as a static website writes it: each group's agents and its
/// allow and disallow paths as lists, a crawl delay in seconds, and the
/// sitemaps to advertise beyond this plugin's own.
///
/// ```yaml
/// groups:
///   - agents: ["*"]
///     disallow: ["/drafts/"]
///     allow: ["/drafts/public/"]
///     crawl_delay: 10
/// sitemaps: []
/// ```
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct RobotsDefinition {
    pub groups: Vec<GroupDefinition>,
    pub sitemaps: Vec<String>,
}

/// One `User-agent` block of [`RobotsDefinition`].
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct GroupDefinition {
    pub agents: Vec<String>,
    pub allow: Vec<String>,
    pub disallow: Vec<String>,
    pub crawl_delay: Option<u32>,
}

impl RobotsDefinition {
    /// The document the definition describes.
    pub fn into_document(self) -> crate::robots::Robots {
        use crate::robots::{Group, Rule};
        crate::robots::Robots {
            groups: self
                .groups
                .into_iter()
                .map(|g| {
                    let mut rules: Vec<Rule> = g.allow.into_iter().map(Rule::Allow).collect();
                    rules.extend(g.disallow.into_iter().map(Rule::Disallow));
                    if let Some(seconds) = g.crawl_delay {
                        rules.push(Rule::CrawlDelay(seconds));
                    }
                    Group {
                        agents: g.agents,
                        rules,
                    }
                })
                .collect(),
            sitemaps: self.sitemaps,
        }
    }
}

/// A YAML definition file, or `None` when there is none.
fn read_yaml<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> CoreResult<Option<T>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| CoreError::Config(format!("could not read {}: {e}", path.display())))?;
    serde_saphyr::from_str(&text)
        .map(Some)
        .map_err(|e| CoreError::Config(format!("{} does not parse: {e}", path.display())))
}

/// Whether a page the build wrote belongs in the sitemap: a clean page, never
/// the error page or a file-named URL (`/robots.txt`).
fn advertised(page: &str) -> bool {
    page != "/404"
        && !page
            .rsplit('/')
            .next()
            .is_some_and(|last| last.contains('.'))
}

#[cfg(test)]
mod static_tests {
    use super::*;
    use crate::SiteContext;

    #[tokio::test]
    async fn a_static_build_gets_robots_and_a_sitemap_of_its_pages_plus_the_yaml_files() {
        let (db, _guard) = laterite_core::testing::connect_test(&[]).await;
        let tmp = tempfile::tempdir().unwrap();
        let files = tmp.path().join("plugins/rainmill.discovery");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(
            files.join("robots.yaml"),
            "groups:\n  - agents: [\"*\"]\n    disallow: [\"/drafts/\"]\n    crawl_delay: 10\n",
        )
        .unwrap();
        std::fs::write(
            files.join("sitemap.yaml"),
            "entries:\n  - loc: /\n    priority: 1.0\n",
        )
        .unwrap();
        std::fs::write(
            files.join("humans.yaml"),
            "sections:\n  - title: Team\n    entries:\n      - [Name, Ada]\n",
        )
        .unwrap();
        let registry = Registry::new();
        let site = SiteContext::new("https://acme.example");
        let pages = vec![
            "/".to_string(),
            "/about".to_string(),
            "/404".to_string(),
            "/robots.txt".to_string(),
        ];
        let cx = laterite_core::SiteFilesCx {
            db: &db,
            registry: &registry,
            site: &site,
            root: tmp.path(),
            files,
            pages: &pages,
        };
        let out = DiscoveryModule.site_files(&cx).await.unwrap();
        let paths: Vec<&str> = out.iter().map(|f| f.path()).collect();
        assert_eq!(paths, vec!["/robots.txt", "/sitemap.xml", "/humans.txt"]);
        let robots = out[0].render(&site);
        assert!(robots.contains("Disallow: /drafts/"), "{robots}");
        assert!(robots.contains("Crawl-delay: 10"), "{robots}");
        assert!(
            robots.contains("Sitemap: https://acme.example/sitemap.xml"),
            "{robots}"
        );
        let sitemap = out[1].render(&site);
        assert!(
            sitemap.contains("<loc>https://acme.example/</loc>"),
            "{sitemap}"
        );
        assert!(
            sitemap.contains("<loc>https://acme.example/about/</loc>"),
            "{sitemap}"
        );
        assert!(!sitemap.contains("404"), "{sitemap}");
        assert!(!sitemap.contains("robots.txt"), "{sitemap}");
        assert_eq!(
            sitemap.matches("<loc>https://acme.example/</loc>").count(),
            1
        );
        assert!(out[2].render(&site).contains("Ada"));
    }

    #[tokio::test]
    async fn without_definition_files_the_policy_is_permissive() {
        let (db, _guard) = laterite_core::testing::connect_test(&[]).await;
        let tmp = tempfile::tempdir().unwrap();
        let registry = Registry::new();
        let site = SiteContext::new("https://acme.example");
        let pages = vec!["/".to_string()];
        let cx = laterite_core::SiteFilesCx {
            db: &db,
            registry: &registry,
            site: &site,
            root: tmp.path(),
            files: tmp.path().join("plugins/rainmill.discovery"),
            pages: &pages,
        };
        let out = DiscoveryModule.site_files(&cx).await.unwrap();
        assert_eq!(out.len(), 2);
        assert!(out[0].render(&site).contains("User-agent: *"));
    }
}
