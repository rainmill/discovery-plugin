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
- **A static site** builds them in code and writes them at build time.

The core half has no dependency on Laterite at all, so a static generator can use
it without pulling in the admin, a database driver or an async runtime:

```toml
rainmill-discovery = { version = "0.1", default-features = false }
```

```rust
use rainmill_discovery::{SiteContext, SiteFile, robots::{Group, Robots}};

let cx = SiteContext::new("https://acme.example");
let robots = Robots { groups: vec![Group::all().disallow("/private/")], sitemaps: vec!["/sitemap.xml".into()] };
site.file(robots.path(), &robots.render(&cx))?;
```

An application with an admin takes the default features instead, and gets the
settings screens and routes.

## The admin exclusion

A site with a panel must keep crawlers out of it, using the path the panel
actually mounted at rather than a hardcoded `/admin`. That lives inside the pure
render, so no surface can forget it.

It is repeated in **every** user-agent group, not only `*`. Under RFC 9309 a
crawler obeys only the most specific group matching it, so an exclusion in `*`
does not reach a crawler that names itself.
