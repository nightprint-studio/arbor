//! `bennu-web` — the web-config graph (docs §2, §8).
//!
//! Models the web-config control flow as a **first-class language** (the XML *is* the
//! source of truth — 880 `<action>`, 0 annotations in the reference project — docs §8):
//!
//! - **Struts2 / XWork** ([`struts`]) — parse `struts.xml` + follow the per-classpath
//!   `<include file="…">` graph across the resource tree; extract
//!   `<package namespace>` + `<action name method class>` + `<result name type>`;
//!   wildcard action names (`*`, `{1}` backrefs) become *candidate* patterns marked
//!   inferred (docs §7).
//! - **Spring bean-XML** ([`spring`]) — `<bean id class parent>` → an id→FQCN map. This
//!   is the load-bearing C1 join: `<action class="beanId">` is a Spring **bean-id**, not
//!   an FQCN (docs §10 C1), so JSP→action resolution goes *through* here.
//! - **Apache Tiles** ([`tiles`]) — `<definition name template>` (+ `extends` +
//!   `<put-attribute name="body">`) → the JSP the definition renders (docs §8 #2).
//! - **[`graph`]** ties them together and exposes the two chains the integration needs:
//!   action → bean-id → FQCN, and action → tiles result → JSP.
//!
//! The emitted [`model`] records are **string-keyed**; the integration ingests them into
//! [`bennu_index`] as `Symbol`s / `Relation` edges, resolving the string keys to ids.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_web::prelude::...`.

pub mod graph;
pub mod model;
pub mod prelude;
pub mod spring;
pub mod struts;
pub mod tiles;
pub mod xml;

/// Tiny filesystem helpers for the module unit tests (write inline XML fixtures to a
/// scratch dir). Compiled only under `cfg(test)`.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch dir under the OS temp dir (per test-binary + counter).
    pub fn tmp_dir(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("bennu-web-test-{}-{}-{}", std::process::id(), tag, n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write `content` to a uniquely-named file and return its path.
    pub fn tmp(name: &str, content: &str) -> PathBuf {
        let dir = tmp_dir("f");
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }
}
