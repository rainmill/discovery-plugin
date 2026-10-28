//! The files a site publishes about itself.
//!
//! `robots.txt`, `sitemap.xml`, `llms.txt`, `humans.txt`, `security.txt`: small
//! documents at conventional paths that something arriving at the site reads to
//! find out about it. They are policy, not derivation, which is why they belong
//! to a plugin rather than to the framework: what a site allows a crawler to do,
//! and which of its URLs it advertises, are the site's decisions.
//!
//! # One crate, two surfaces
//!
//! The four formats share almost nothing at the content level, so nothing tries
//! to unify them there. What they share is the **envelope**: a path, a media
//! type, and a pure render. That is [`SiteFile`], and it is the only thing the
//! plumbing knows.
//!
//! Rendering is pure and synchronous, which is what lets one definition serve
//! both a database-backed site (a route renders per request) and a static one
//! (the build writes each file). The envelope is the framework's
//! `laterite_core::site`, re-exported here; the `admin` feature adds the
//! settings screens, the routes, and the sources other modules contribute.

#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "admin")]
#[cfg_attr(docsrs, doc(cfg(feature = "admin")))]
pub mod admin;

/// Every plugin exposes `module()` at its crate root, which is what the
/// generated manifest calls. A static-site consumer builds without the `admin`
/// feature and has no module to register, so this goes with it.
#[cfg(feature = "admin")]
pub use admin::module;
pub mod humans;
pub mod llms;
pub mod robots;
pub mod security;
pub mod sitemap;

pub use laterite_core::site::{Problem, SiteContext, SiteFile, Verbatim};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_context_trims_and_joins_urls() {
        let cx = SiteContext::new("https://acme.example/");
        assert_eq!(cx.base_url, "https://acme.example");
        // Joining tolerates a leading slash on either side rather than doubling.
        assert_eq!(cx.url("/about/"), "https://acme.example/about/");
        assert_eq!(cx.url("about/"), "https://acme.example/about/");
    }

    #[test]
    fn a_static_site_has_no_admin_to_exclude() {
        assert!(SiteContext::new("https://acme.example")
            .admin_path
            .is_none());
        assert_eq!(
            SiteContext::new("https://acme.example")
                .with_admin("/backoffice")
                .admin_path
                .as_deref(),
            Some("/backoffice")
        );
    }

    #[test]
    fn a_verbatim_file_is_returned_byte_for_byte() {
        let signed = Verbatim {
            path: "/.well-known/security.txt".to_string(),
            content_type: "text/plain".to_string(),
            body: "-----BEGIN PGP SIGNED MESSAGE-----\nContact: mailto:x@acme.example\n"
                .to_string(),
        };
        // Nothing reformats it: a signature covers the exact bytes.
        assert_eq!(
            signed.render(&SiteContext::new("https://acme.example")),
            signed.body
        );
    }
}
