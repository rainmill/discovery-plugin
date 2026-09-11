//! `humans.txt`: the people behind a site.
//!
//! The one format where "labelled sections of pairs" is not a flattening:
//! humanstxt.org is literally `/* TEAM */` blocks of `Label: value` lines.

use serde::{Deserialize, Serialize};

use crate::{SiteContext, SiteFile};

/// One `/* HEADING */` block.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    pub title: String,
    /// `Label: value` lines, in order.
    #[serde(default)]
    pub entries: Vec<(String, String)>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            entries: Vec::new(),
        }
    }

    pub fn entry(mut self, label: &str, value: &str) -> Self {
        self.entries.push((label.to_string(), value.to_string()));
        self
    }
}

/// The credits a site publishes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Humans {
    #[serde(default)]
    pub sections: Vec<Section>,
}

impl SiteFile for Humans {
    fn path(&self) -> &str {
        "/humans.txt"
    }

    fn content_type(&self) -> &'static str {
        "text/plain; charset=utf-8"
    }

    fn render(&self, _cx: &SiteContext) -> String {
        let mut out = String::new();
        for section in &self.sections {
            out.push_str(&format!("/* {} */\n", section.title.to_uppercase()));
            for (label, value) in &section.entries {
                out.push_str(&format!("{label}: {value}\n"));
            }
            out.push('\n');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_writes_the_conventional_block_format() {
        let humans = Humans {
            sections: vec![
                Section::new("Team")
                    .entry("Developer", "Ada")
                    .entry("Site", "https://acme.example"),
                Section::new("Thanks").entry("Tools", "Rust"),
            ],
        };
        let out = humans.render(&SiteContext::new("https://acme.example"));
        assert!(out.starts_with("/* TEAM */\nDeveloper: Ada\nSite: https://acme.example\n\n"));
        assert!(out.contains("/* THANKS */\nTools: Rust\n"));
    }

    #[test]
    fn nothing_declared_writes_nothing() {
        assert!(Humans::default()
            .render(&SiteContext::new("https://acme.example"))
            .is_empty());
    }
}
