# Rainmill Discovery

The files a site publishes about itself, for [Laterite](https://laterite.rs):
`robots.txt`, `sitemap.xml`, `llms.txt`, `humans.txt` and `security.txt`.

These are small documents at conventional paths that something arriving at the
site reads to find out about it. They are policy rather than derivation: what a
site allows a crawler to do, and which of its URLs it advertises, are the site's
decisions, not its framework's. That is why they live in a plugin.

## Two surfaces, one definition

Each document renders purely and synchronously, which is what lets one definition
serve both kinds of site:

- **A database-backed site** edits the documents in the admin, and a route
  renders each one per request.
- **A static website** keeps them as YAML under `plugins/rainmill.discovery/`,
  and the build writes them.

The plugin declares `static = true`, so a static website made with
`lat new --static` installs it:

```bash
lat plugin add https://github.com/rainmill/discovery-plugin
```

```yaml
# plugins/rainmill.discovery/robots.yaml
groups:
  - agents: ["*"]
    disallow: ["/drafts/"]
    crawl_delay: 10
sitemaps: []
```

```yaml
# plugins/rainmill.discovery/sitemap.yaml
entries:
  - loc: /
    priority: 1.0
    changefreq: weekly
```

`robots.yaml` and `sitemap.yaml` are optional: without them the policy allows
everything and the sitemap lists every page the build wrote (it always does,
beside the entries given and the URLs other modules contribute).
`llms.yaml`, `humans.yaml` and `security.yaml` hold those documents as the
crate's types deserialise, and are written when present.

The documents render from the framework's site envelope
(`laterite_core::site::{SiteContext, SiteFile}`), re-exported here, so a Rust
generator can also render them itself:

```rust
use rainmill_discovery::{SiteContext, SiteFile, robots::{Group, Robots}};

let cx = SiteContext::new("https://acme.example");
let robots = Robots { groups: vec![Group::all().disallow("/private/")], sitemaps: vec!["/sitemap.xml".into()] };
site.file(robots.path(), &robots.render(&cx))?;
```

An application with an admin takes the default features and gets the settings
screens and routes.

## The admin exclusion

A site with a panel must keep crawlers out of it, using the path the panel
actually mounted at rather than a hardcoded `/admin`. That lives inside the pure
render, so no surface can forget it.

It is repeated in **every** user-agent group, not only `*`. Under RFC 9309 a
crawler obeys only the most specific group matching it, so an exclusion in `*`
does not reach a crawler that names itself.

## Contributing URLs to the sitemap

The sitemap knows nothing about content types. A module that owns pages answers
for them:

```rust
use rainmill_discovery::admin::{UrlSource, UrlSourceReg};
use rainmill_discovery::sitemap::Entry;

#[async_trait]
impl UrlSource for Pages {
    async fn urls(&self, db: &Db) -> Vec<Entry> {
        load_published(db).await.into_iter()
            .map(|page| Entry::new(page.path).lastmod(page.updated_at))
            .collect()
    }
}

// from the module's `register`
registry.add(UrlSourceReg::new("acme.pages", Arc::new(Pages)));
```

Sources are collected once at boot, so registration order does not matter, and
the sitemap never goes looking for content: content declares itself.

## Editing

With the `admin` feature, each document gets a settings screen under Discovery,
gated by `rainmill.discovery.manage`. The screens edit a flat projection of each
document (rows of text rather than nested structures); the projection is the only
place the editable shape and the rendered shape meet.
