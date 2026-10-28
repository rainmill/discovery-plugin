//! `llms.txt`: a plain-text summary of a site, aimed at language models.
//!
//! The newest and least settled of these conventions. It is a Markdown document
//! with a fixed shape: an H1 name, a blockquote summary, optional prose, then H2
//! sections of annotated links, with `## Optional` last by convention for what a
//! reader may skip.
//!
//! Modelled exactly as the spec describes and no further. The title and summary
//! are the site's own voice, so they are authored; auto-deriving them produces a
//! file that says nothing worth reading.

use serde::{Deserialize, Serialize};

use crate::{Problem, SiteContext, SiteFile};

/// One annotated link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub note: Option<String>,
}

impl Link {
    pub fn new(title: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            url: url.into(),
            note: None,
        }
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// One `## Heading` block of links.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinkSection {
    pub title: String,
    #[serde(default)]
    pub links: Vec<Link>,
}

impl LinkSection {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            links: Vec::new(),
        }
    }

    pub fn link(mut self, link: Link) -> Self {
        self.links.push(link);
        self
    }
}

/// A site's summary for language models.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Llms {
    /// The site's name, the document's H1.
    pub title: String,
    /// One paragraph, rendered as a blockquote.
    #[serde(default)]
    pub summary: Option<String>,
    /// Free prose. Headings are not allowed here: the spec reserves them for
    /// section boundaries, so one would silently start a new section.
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub sections: Vec<LinkSection>,
    /// Rendered last, as `## Optional`: what a reader with a short budget may
    /// skip.
    #[serde(default)]
    pub optional: Option<LinkSection>,
}

impl SiteFile for Llms {
    fn path(&self) -> &str {
        "/llms.txt"
    }

    fn content_type(&self) -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("text/markdown; charset=utf-8")
    }

    fn render(&self, cx: &SiteContext) -> String {
        let mut out = format!("# {}\n", self.title);
        if let Some(summary) = &self.summary {
            out.push_str(&format!("\n> {summary}\n"));
        }
        if let Some(details) = &self.details {
            out.push_str(&format!("\n{details}\n"));
        }
        for section in &self.sections {
            out.push_str(&render_section(cx, section));
        }
        if let Some(optional) = &self.optional {
            out.push_str(&render_section(
                cx,
                &LinkSection {
                    title: "Optional".to_string(),
                    links: optional.links.clone(),
                },
            ));
        }
        out
    }

    fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.title.trim().is_empty() {
            problems.push(Problem::new("title", "a name is required"));
        }
        if let Some(details) = &self.details {
            if details
                .lines()
                .any(|line| line.trim_start().starts_with('#'))
            {
                problems.push(Problem::new(
                    "details",
                    "a heading here would start a new section; use plain paragraphs",
                ));
            }
        }
        problems
    }
}

fn render_section(cx: &SiteContext, section: &LinkSection) -> String {
    let mut out = format!("\n## {}\n\n", section.title);
    for link in &section.links {
        let url = if link.url.starts_with("http://") || link.url.starts_with("https://") {
            link.url.clone()
        } else {
            cx.url(&link.url)
        };
        match &link.note {
            Some(note) => out.push_str(&format!("- [{}]({}): {}\n", link.title, url, note)),
            None => out.push_str(&format!("- [{}]({})\n", link.title, url)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx() -> SiteContext {
        SiteContext::new("https://acme.example")
    }

    #[test]
    fn it_writes_the_shape_the_spec_describes() {
        let llms = Llms {
            title: "Acme".to_string(),
            summary: Some("A widget company.".to_string()),
            details: Some("We have made widgets since 1998.".to_string()),
            sections: vec![LinkSection::new("Docs")
                .link(Link::new("Guide", "/docs/").note("How to start"))
                .link(Link::new("API", "/api/"))],
            optional: Some(LinkSection {
                title: String::new(),
                links: vec![Link::new("Changelog", "/changelog/")],
            }),
        };
        let out = llms.render(&cx());

        assert!(out.starts_with("# Acme\n"));
        assert!(out.contains("\n> A widget company.\n"));
        assert!(out.contains("We have made widgets since 1998."));
        assert!(out.contains("\n## Docs\n\n"));
        // A note becomes the colon-suffixed description; without one the link
        // stands alone.
        assert!(out.contains("- [Guide](https://acme.example/docs/): How to start\n"));
        assert!(out.contains("- [API](https://acme.example/api/)\n"));
        // Optional is always titled Optional and comes last.
        assert!(out
            .trim_end()
            .ends_with("- [Changelog](https://acme.example/changelog/)"));
        assert!(out.contains("## Optional"));
    }

    #[test]
    fn only_the_title_is_required() {
        let llms = Llms {
            title: "Acme".to_string(),
            ..Default::default()
        };
        assert_eq!(llms.render(&cx()), "# Acme\n");
        assert!(llms.problems().is_empty());
    }

    #[test]
    fn a_missing_name_is_reported() {
        assert!(Llms::default()
            .problems()
            .iter()
            .any(|p| p.field == "title"));
    }

    /// A heading inside the prose would silently start a new section, since the
    /// spec reads H2s as section boundaries.
    #[test]
    fn a_heading_in_the_prose_is_reported() {
        let llms = Llms {
            title: "Acme".to_string(),
            details: Some("Intro\n\n## Sneaky\n".to_string()),
            ..Default::default()
        };
        assert!(llms.problems().iter().any(|p| p.field == "details"));
    }
}
