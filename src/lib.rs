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
//! both a database-backed site (a route renders per request) and a static one (a
//! generator renders at build time). The core half, this module and its
//! documents, has no dependency on Laterite at all; the `admin` feature adds the
//! settings screens, the routes, and the sources other modules contribute.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod humans;
pub mod llms;
pub mod robots;
pub mod security;
pub mod sitemap;

use chrono::{DateTime, Utc};

/// What a document is given when it renders.
///
/// `admin_path` is the reason this exists: a robots file has to exclude the
/// panel using the path it actually mounted at, never a hardcoded `/admin`, and
/// putting that in the pure render means no surface can forget it. It is `None`
/// for a static site, which has no panel to exclude.
#[derive(Debug, Clone)]
pub struct SiteContext {
    /// The site's origin, without a trailing slash.
    pub base_url: String,
    /// Where the admin panel is mounted, when there is one.
    pub admin_path: Option<String>,
    /// The moment the file is being produced, for anything that dates itself.
    pub now: DateTime<Utc>,
}

impl SiteContext {
    /// A context for a site with no admin panel, which is every static site.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            admin_path: None,
            now: Utc::now(),
        }
    }

    /// Names where the admin is mounted, so the documents can exclude it.
    pub fn with_admin(mut self, path: impl Into<String>) -> Self {
        self.admin_path = Some(path.into());
        self
    }

    /// `path` as an absolute URL against this site's origin.
    pub fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

/// Something a site publishes at a fixed path.
///
/// The envelope every document shares. Rendering is infallible because a
/// document is validated when it is authored, never when it is served: a site
/// that cannot render its own robots file has no better answer to give a crawler
/// than the one it can render.
pub trait SiteFile {
    /// Where it is served, absolute from the site root.
    fn path(&self) -> &str;

    /// Its media type, for the `Content-Type` header.
    fn content_type(&self) -> &'static str;

    /// The file's bytes as text. Pure: no IO, no clock beyond `cx.now`.
    fn render(&self, cx: &SiteContext) -> String;

    /// What is wrong with this document, for the editing screen and for a
    /// pre-flight check. Empty means it is fit to publish.
    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }
}

/// Something wrong with a document, named so an editor can point at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The part of the document at fault, for an editor to highlight.
    pub field: String,
    pub message: String,
}

impl Problem {
    pub fn new(field: &str, message: impl Into<String>) -> Self {
        Self {
            field: field.to_string(),
            message: message.into(),
        }
    }
}

/// A file whose contents the site supplies verbatim.
///
/// The escape hatch, and the only honest way to serve a PGP-signed
/// `security.txt`: signing covers the exact bytes, so nothing may reformat them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Verbatim {
    pub path: String,
    pub content_type: String,
    pub body: String,
}

impl SiteFile for Verbatim {
    fn path(&self) -> &str {
        &self.path
    }

    fn content_type(&self) -> &'static str {
        // A verbatim file's type is data, and the trait wants a static string;
        // text/plain covers every root file this is used for, and a signed
        // security.txt is text/plain by RFC 9116.
        "text/plain; charset=utf-8"
    }

    fn render(&self, _cx: &SiteContext) -> String {
        self.body.clone()
    }
}

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
