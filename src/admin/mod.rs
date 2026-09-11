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
use laterite_core::{Db, MigrationSet, Module, ModuleId, Registry};

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

impl Module for DiscoveryModule {
    fn id(&self) -> ModuleId {
        ModuleId::new(MODULE_ID)
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
