//! One public route per document.
//!
//! Each route loads its settings value, projects it into the document, and
//! renders. The path is literal and never namespaced, because these files only
//! work where crawlers look for them.

use std::sync::Arc;

// Never `axum` directly: the framework re-exports the one version a contributed
// route must build against, so two majors cannot link side by side.
use laterite_admin::axum::http::HeaderMap;
use laterite_admin::axum::response::IntoResponse;
use laterite_admin::axum::routing::get;
use laterite_admin::axum::Router;
use laterite_admin::routes::{PublicRoute, PublicRouteReg, RouteCtx};
use laterite_core::Db;

use super::settings::{
    HumansSettings, LlmsSettings, RobotsSettings, SecuritySettings, SitemapSettings,
};
use super::UrlSourceReg;
use crate::sitemap::Sitemap;
use crate::{SiteContext, SiteFile};

/// Which document a route serves. One route type covers all five: they differ
/// only in which settings model they load.
#[derive(Debug, Clone, Copy)]
enum Document {
    Robots,
    Sitemap,
    Llms,
    Humans,
    Security,
}

/// What a route needs after boot, cloned per request.
#[derive(Clone)]
struct Serve {
    document: Document,
    db: Db,
    base_url: Arc<str>,
    admin_path: Arc<str>,
    sources: Arc<Vec<Arc<dyn super::UrlSource>>>,
}

impl Serve {
    fn context(&self) -> SiteContext {
        SiteContext::new(self.base_url.as_ref()).with_admin(self.admin_path.as_ref())
    }

    async fn render(self, headers: HeaderMap) -> impl IntoResponse {
        let cx = self.context();
        // A settings read that fails renders the document's default rather than
        // erroring: a crawler asking for robots.txt must not get a 500, and the
        // default still excludes the panel.
        let (content_type, body) = match self.document {
            Document::Robots => {
                let mut file = load::<RobotsSettings>(&self.db).await.into_document();
                // This plugin publishes the sitemap, so robots names it without
                // being told. Asking an operator to type the path of a file its
                // own plugin serves is asking them to get it wrong.
                let own = Sitemap::default().path().to_string();
                if !file.sitemaps.iter().any(|s| s.ends_with(&own)) {
                    file.sitemaps.insert(0, own);
                }
                (file.content_type(), file.render(&cx))
            }
            Document::Sitemap => {
                let mut file = load::<SitemapSettings>(&self.db).await.into_document();
                for source in self.sources.iter() {
                    file.entries.extend(source.urls(&self.db).await);
                }
                (file.content_type(), file.render(&cx))
            }
            Document::Llms => {
                let file = load::<LlmsSettings>(&self.db).await.into_document();
                (file.content_type(), file.render(&cx))
            }
            Document::Humans => {
                let file = load::<HumansSettings>(&self.db).await.into_document();
                (file.content_type(), file.render(&cx))
            }
            Document::Security => {
                let file = load::<SecuritySettings>(&self.db).await.into_document();
                (file.content_type(), file.render(&cx))
            }
        };
        // Cached, but revalidated: these are fetched by machines on a schedule,
        // and an hour of blind caching would mean an operator saves a change in
        // the admin and watches nothing happen. `no-cache` still allows a cache
        // to keep the copy; it just has to ask first, and the validator makes
        // that answer a bodyless 304 whenever the settings have not moved.
        laterite_admin::http_cache::conditional(
            &headers,
            content_type,
            laterite_admin::http_cache::REVALIDATE,
            body.into_bytes(),
        )
    }
}

async fn load<T: laterite_admin::settings::SettingsModel>(db: &Db) -> T {
    laterite_admin::settings::load::<T>(db)
        .await
        .unwrap_or_default()
}

struct FileRoute(Document);

impl PublicRoute for FileRoute {
    fn mount(&self, ctx: &RouteCtx) -> Router {
        let serve = Serve {
            document: self.0,
            db: ctx.db().clone(),
            base_url: Arc::from(ctx.base_url()),
            admin_path: Arc::from(ctx.admin_path()),
            // Collected once at boot: every module has registered by now, so
            // registration order does not matter.
            sources: Arc::new(
                ctx.contributions::<UrlSourceReg>()
                    .into_iter()
                    .map(|reg| reg.source.clone())
                    .collect(),
            ),
        };
        Router::new().route(
            "/",
            get(move |headers: HeaderMap| {
                let serve = serve.clone();
                async move { serve.render(headers).await }
            }),
        )
    }
}

pub fn public_routes() -> Vec<PublicRouteReg> {
    vec![
        PublicRouteReg::new("/robots.txt", Arc::new(FileRoute(Document::Robots))),
        PublicRouteReg::new("/sitemap.xml", Arc::new(FileRoute(Document::Sitemap))),
        PublicRouteReg::new("/llms.txt", Arc::new(FileRoute(Document::Llms))),
        PublicRouteReg::new("/humans.txt", Arc::new(FileRoute(Document::Humans))),
        PublicRouteReg::new(
            "/.well-known/security.txt",
            Arc::new(FileRoute(Document::Security)),
        ),
    ]
}
