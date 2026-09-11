//! `/.well-known/security.txt`: how to report a vulnerability (RFC 9116).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Problem, SiteContext, SiteFile};

/// A site's security contact details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityTxt {
    /// At least one, in order of preference. A `mailto:` or `https:` URI.
    #[serde(default)]
    pub contacts: Vec<String>,
    /// When this file stops being valid. Authored, never rolled forward from
    /// now: the field exists to show a human reviewed it recently, and a rolling
    /// date defeats exactly that.
    pub expires: DateTime<Utc>,
    #[serde(default)]
    pub encryption: Option<String>,
    #[serde(default)]
    pub acknowledgments: Option<String>,
    #[serde(default)]
    pub preferred_languages: Vec<String>,
    /// Where this file canonically lives. Defaults to its own URL at render.
    #[serde(default)]
    pub canonical: Option<String>,
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub hiring: Option<String>,
}

impl SecurityTxt {
    /// A file with one contact, expiring on the given date.
    pub fn new(contact: impl Into<String>, expires: DateTime<Utc>) -> Self {
        Self {
            contacts: vec![contact.into()],
            expires,
            encryption: None,
            acknowledgments: None,
            preferred_languages: Vec::new(),
            canonical: None,
            policy: None,
            hiring: None,
        }
    }
}

impl SiteFile for SecurityTxt {
    fn path(&self) -> &str {
        "/.well-known/security.txt"
    }

    fn content_type(&self) -> &'static str {
        "text/plain; charset=utf-8"
    }

    fn render(&self, cx: &SiteContext) -> String {
        let mut out = String::new();
        for contact in &self.contacts {
            out.push_str(&format!("Contact: {contact}\n"));
        }
        // RFC 9116 wants an absolute instant, not a date.
        out.push_str(&format!(
            "Expires: {}\n",
            self.expires.format("%Y-%m-%dT%H:%M:%SZ")
        ));
        let canonical = self
            .canonical
            .clone()
            .unwrap_or_else(|| cx.url(self.path()));
        out.push_str(&format!("Canonical: {canonical}\n"));
        if !self.preferred_languages.is_empty() {
            out.push_str(&format!(
                "Preferred-Languages: {}\n",
                self.preferred_languages.join(", ")
            ));
        }
        for (field, value) in [
            ("Encryption", &self.encryption),
            ("Acknowledgments", &self.acknowledgments),
            ("Policy", &self.policy),
            ("Hiring", &self.hiring),
        ] {
            if let Some(value) = value {
                out.push_str(&format!("{field}: {value}\n"));
            }
        }
        out
    }

    fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        if self.contacts.is_empty() {
            problems.push(Problem::new("contacts", "at least one contact is required"));
        }
        if self.expires <= Utc::now() {
            problems.push(Problem::new(
                "expires",
                "this file has expired and will be ignored",
            ));
        } else if self.expires < Utc::now() + chrono::Duration::days(30) {
            problems.push(Problem::new("expires", "this file expires within 30 days"));
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn later() -> DateTime<Utc> {
        Utc::now() + chrono::Duration::days(365)
    }

    #[test]
    fn it_writes_the_required_fields() {
        let file = SecurityTxt::new("mailto:security@acme.example", later());
        let out = file.render(&SiteContext::new("https://acme.example"));
        assert!(out.contains("Contact: mailto:security@acme.example\n"));
        assert!(out.contains("Expires: "));
        // Canonical defaults to where the file actually lives.
        assert!(out.contains("Canonical: https://acme.example/.well-known/security.txt\n"));
    }

    #[test]
    fn optional_fields_appear_only_when_set() {
        let mut file = SecurityTxt::new("mailto:x@acme.example", later());
        assert!(!file
            .render(&SiteContext::new("https://acme.example"))
            .contains("Policy:"));
        file.policy = Some("https://acme.example/security".to_string());
        file.preferred_languages = vec!["en".to_string(), "de".to_string()];
        let out = file.render(&SiteContext::new("https://acme.example"));
        assert!(out.contains("Policy: https://acme.example/security\n"));
        assert!(out.contains("Preferred-Languages: en, de\n"));
    }

    /// The whole point of `Expires` is to show someone reviewed the file
    /// recently, so an expired or nearly-expired one is worth saying out loud.
    #[test]
    fn an_expired_file_is_reported() {
        let stale = SecurityTxt::new(
            "mailto:x@acme.example",
            Utc::now() - chrono::Duration::days(1),
        );
        assert!(stale
            .problems()
            .iter()
            .any(|p| p.message.contains("expired")));

        let soon = SecurityTxt::new(
            "mailto:x@acme.example",
            Utc::now() + chrono::Duration::days(5),
        );
        assert!(soon
            .problems()
            .iter()
            .any(|p| p.message.contains("within 30 days")));

        assert!(SecurityTxt::new("mailto:x@acme.example", later())
            .problems()
            .is_empty());
    }

    #[test]
    fn a_file_with_no_contact_is_reported() {
        let mut file = SecurityTxt::new("mailto:x@acme.example", later());
        file.contacts.clear();
        assert!(file.problems().iter().any(|p| p.field == "contacts"));
    }
}
