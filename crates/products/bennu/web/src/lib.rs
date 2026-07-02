//! `bennu-web` — the web-config graph (**Phase-0 skeleton**).
//!
//! Role (docs §2, §8): model the web-config control flow as a **first-class language**
//! (the XML *is* the source of truth — 880 `<action>`, 0 annotations — docs §8):
//! Struts2 / XWork (`struts.xml` + the per-classpath `<include>` graph, wildcard
//! expansion → *candidate* nav, `validation.xml`), Apache Tiles (result→def→JSP), and
//! Spring bean-XML (`id`→impl). **Entando-aware**: it merges the ~69 config fragments
//! pulled by classpath-name (docs §8 #3). This is the config-graph resolution the
//! spike found load-bearing (docs §10 C1) — `<action class="beanId">` is a Spring
//! bean-id, not an FQCN, so JSP→action resolution goes *through* here.
//!
//! **Skeleton only**: role + prelude. The struts/tiles/spring graph builders land in
//! the Phase-2 config-index wave, feeding `bennu-index`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_web::prelude::...`.

pub mod prelude;
