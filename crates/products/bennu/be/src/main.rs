//! `bennu-be` — the headless Java-editor / analysis backend process for Model D.
//!
//! The bennu twin of `tyto-be` / `merula-be`, on the slim path: it serves the bennu
//! domains (open_project / project_tree / read_file / capabilities / completion /
//! diagnostics) over framed-stdio IPC, drives the leaf analysis crates
//! (`bennu-project`, later `bennu-index` / `bennu-classpath` / `bennu-java` / …), and
//! has **no plugin host, no `NsHost`, no credentials/OAuth, and no pushed config** —
//! it resolves its own `bennu_config_dir()` / `bennu_data_dir()` once
//! `init_active_profile()` has run.
//!
//! Each domain's handler functions live in their own module here, auto-advertised via
//! `Hello` and auto-routed by the shell's broker. Phase 0 wires the real project
//! model (capability detection, pom parse, encoding/JDK detection, file tree,
//! decoded reads) plus the completion / diagnostics stubs that route through the
//! native intel provider — so the FE binds the whole contract from day one.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::io::{self, Write};
use std::sync::Arc;

use bennu_core::prelude::BennuState;

// Self-test handlers (be_ping / be_echo) prove the framed-stdio handshake.
mod selftest;

// `JobHandle`: register the background analysis warm-up as a tracked job in the shell registry
// (over the reverse channel) so it appears in the bennu Jobs overlay.
mod jobs;

// Domain handler modules — each holds the `#[arbor_rpc::handler]`s for one bennu
// domain. They self-register via `inventory`, so `arbor_rpc::registry()` collects
// them and `Hello` advertises them by name.
//
// The typed product config (`get/set_bennu_config`): the editor's per-project JDK /
// encoding overrides + defaults, owned out-of-process here.
mod config_cmds;
// Project model: `bennu_open_project` / `bennu_project_tree` / `bennu_read_file` —
// pom parse + capability detection + JDK/encoding detection + file tree + decoded
// reads, all driven through the leaf `bennu-project` crate.
mod project;
// Capabilities: `bennu_capabilities` — re-detect the Spike-D capability bitset for a
// project without re-opening it.
mod capabilities;
// Code-intel: `bennu_completion` / `bennu_diagnostics` — completion serves from the
// per-project index the `index_service` builds; diagnostics stay a stub for now.
mod intel;
// Refactor rename (docs §5 #10-12): `bennu_rename_plan` (preview) / `bennu_rename_apply`
// (edits) — best-effort, config-aware, off the per-project rename engine.
mod rename;
// Find-usages (docs §5 #7): `bennu_references` — the read-only twin of rename, reporting
// every resolved use site of the symbol under the caret off the same reference index.
mod references;
// Hover (editor hover card): `bennu_hover` — classifies the symbol under the caret off the
// per-project rename engine and returns its signature / kind / owning type.
mod hover;
// Go-to-declaration (Ctrl+Click / Ctrl+B): `bennu_declaration` — resolves the symbol under
// the caret to its declaration site (method / field / local / class) off the same engine.
mod declaration;
// Inherited members (Structure panel's lazy "Inherited" bucket): `bennu_inherited_members` —
// the members inherited from a type's superclass + interfaces (each tagged declaring-type +
// visibility + a project source when resolvable), off the same engine.
mod inherited;
// Validation-file modal context: `bennu_validation_context` — the action class + its
// writable bean properties + already-validated fields for a `<Action>-validation.xml`.
mod validation;
// Form analysis (form → parameters inspector): `bennu_form_analysis` — for a JSP, every
// relevant `<form>` (own, or on a fragment it includes, or on a page that includes it) with its
// complete include-expanded parameter set correlated against the action class (which fields
// bind, which are validated, and where each parameter comes from).
mod forms;
// JSP variable navigation: `bennu_jsp_nav` — go-to-declaration + find-usages for a page-scoped
// JSP variable (`<c:set var>` / `<s:set var>` / … + `${var}` / `%{var}` references), single-file.
mod jsp_nav;
// Action-property navigation + linting: `bennu_action_property_target` (go-to from a JSP form field
// / OGNL root, or a `*-validation.xml` `<field>`, to the action class's `get/set/is` accessor) and
// `bennu_action_property_lint` (a field that matches NO property of the resolved action → warning).
mod action_props;
mod action_props_nav;
// MyBatis mapper-XML navigation: `bennu_mybatis_nav` — go-to from inside a mapper (a statement
// id → the Java interface method, `namespace` → the interface, `<include refid>` → its `<sql>`,
// a `resultMap="…"` → its `<resultMap>`).
mod mybatis_nav;
// Mojibake check: `bennu_mojibake_check` — find UTF-8-decoded-as-Cp1252 corruption (`Ã©` → `é`,
// `â€™` → `'`) in a file, with the corrected character for a one-click fix.
mod mojibake;
// Alt+Enter intentions: `bennu_intentions_at` — every applicable quick-fix at the caret
// (parameterize logging, NP-safe equals, isEmpty()/boolean/negated-comparison simplifications),
// over the pure `bennu-intentions` catalog.
mod intentions;
// New-file scaffolding: `bennu_new_file` — a Java class/interface/enum/record (package inferred
// from the dir) / JSP / XML / plain file's name + initial content for the tree "New…" menu.
mod new_file;
// Index inspector: `bennu_index_stats` — a cheap snapshot of the per-project index (symbol
// + config counts, JDK level, build-ready flag) for an inspector panel.
mod index_stats;
// Encoding report: `bennu_encoding_report` — the source files whose bytes weren't valid in the
// project's declared (Maven `sourceEncoding`) encoding (recovered + indexed, but flagged) for
// a future "non-compliant files" UI.
mod encoding_report;
// JDK status: `bennu_jdk_status` — how the project's JDK resolved (exact / fallback / none),
// for the titlebar warning (no JDK) + Problems entry (wrong-version JDK).
mod jdk_status;
// Index inspector entries: `bennu_index_entries` — the per-kind entry lists (members / jars
// / jdk / beans / actions / relations) behind each headline stat, off the built index.
mod inspect;
// The per-project index lifecycle: build the symbol index off-thread on open, cache
// the native provider, serve completion from it, and patch a single file on edit.
mod index_service;
// Config-graph input discovery: walk the project tree to find struts/spring/tiles files
// (`WebInputs`) for the config-graph build.
mod web_discovery;
// Class index (Go to Class): `bennu_class_index` — a fresh scan of the project's `.java`
// sources, one entry per declared type (fqcn + simple + file + decl line).
mod class_index;
// Manual index invalidation: `bennu_reindex` — drop + rebuild the whole semantic index for
// an open project (fresh generation dir, off-thread), the escape hatch behind the Index
// Inspector's "Rebuild" button. No compilation (that's `bennu_build`), just a re-scan.
mod reindex;
// TODO scan (TODO tool window): `bennu_todos` — a line scan of `.java`/`.xml`/`.jsp`/
// `.properties` for TODO/FIXME/XXX/HACK markers.
mod todos;
// Spell-check (editor niceties): `bennu_spellcheck` (declaration names + comments, split by
// case, checked against en_US/it_IT Hunspell + tech allow-list + custom dicts) /
// `bennu_dict_add` / `bennu_spell_status` / `bennu_download_dictionaries` (LibreOffice dicts).
mod spell;
// Find in files (project-wide text search): `bennu_find_in_files` — a fresh, line-oriented
// scan of the project's text files for a query (plain / whole-word; regex is a
// case-insensitive substring fallback, as the `regex` crate isn't a dependency).
mod find;
// Build/run (docs §4 "il fondo"): `bennu_build` (mvn -q -o compile / javac fallback +
// error parser → structured diagnostics) / `bennu_run` (java <vm> -cp … streaming output,
// cwd + env) / `bennu_cancel_run`. Makes the Run/Debug buttons real + re-indexes
// target/classes.
mod build;
// Dependency-classpath sourcing for the index: resolve a Maven project's `~/.m2` dep jars (cached
// across sessions by pom mtime) into a `ClassSource`, so validation/completion resolve library
// types (Spring, servlet, …), not just the JDK + project. Non-fatal — degrades to JDK-only.
mod dep_classpath;
// The Dependencies tool window (`bennu_dependencies`): every module's effective dependency list read
// out of the poms — inheritance, `${properties}`, `<dependencyManagement>` — matched against the jars
// `dep_classpath` already resolved. Reads files only; never runs Maven.
mod dependencies;
// The Spring beans an **allowlisted** dependency declares (`bennu_library_beans`): its jar's classes
// decoded for their annotations, grouped by artifact. Display only — a bean declared in a jar is a
// declaration Spring may or may not act on, so nothing here feeds resolution or a diagnostic.
mod library_beans;
// That scan, remembered on disk per ARTIFACT (not per project — two projects asking about the same
// jar are asking about the same bytes). Invalidated by the jar's mtime+size and by the extraction's
// own schema, because both the jar and the code that reads it can change the right answer.
mod library_bean_cache;
// One definition of "this classpath, unchanged" — path + mtime + size per jar, and the epoch every
// classpath-derived cache keys off. Central because the trap is shared: a jar's identity is not its
// path, and an in-place `-SNAPSHOT` reinstall is invisible to anything that thinks it is.
mod classpath_stamp;
// Noticing that a dependency was rebuilt WHILE Bennu is open — the one window the on-disk stamping
// cannot cover, because the dependency member tier is in memory by design. Re-stamps on a timer and
// rebuilds + emits `classpath-changed` when the jars move.
mod classpath_watch;
// "Download sources" for a Maven dependency: locate its ~/.m2 jar, derive coordinates, and fetch
// the `-sources.jar` via `mvn dependency:get` — behind the decompiled-tab banner.
mod sources_download;
// Go-to-declaration + hover INSIDE a library/JDK source view (`bennu_library_declaration` /
// `bennu_library_hover`): resolves the caret against the origin project's classpath resolver and
// opens the target's source view member-precise, chaining library → library.
mod library_nav;
// Project-wide "validation without compiling": `bennu_validate_project` — walks every `.java`,
// runs the editor's per-file validation over all of them, and returns timing stats (the compile-time
// proxy) + diagnostics. Shares `build`'s single-run guard so a validation and a Maven build can't run
// concurrently.
mod validate_project;
// Run configurations (per-repo `[bennu.run]` in `<repo>/.arbor/config.toml`):
// `bennu_get_run_config` / `bennu_set_run_config` — the IntelliJ-style named run targets
// the FE's run-configuration editor persists (filesystem, not localStorage).
mod run_config;
// Main-class discovery (run-config editor's picker): `bennu_main_classes` — a fresh
// `.java` scan for types declaring `public static void main(String[])`.
mod main_classes;
// Framework extensions (`bennu-ext` + `bennu-spring` + `bennu-jpa`): `bennu_ext_*` (highlights /
// diagnostics / gutter / navigate / hover / completion / inline hint / catalog / overview) plus the
// few framework-specific settings verbs. Capability-gated and lazy — a project no extension applies
// to never walks a file. This module is the ONLY place the backend names a framework, and adding a
// third is one entry in `registry_for`.
mod frameworks;
// Tomcat JSP hot-swap (per-repo `[bennu.tomcat]`): `bennu_get/set_tomcat_config` (the link) +
// `bennu_detect_tomcat` (validate a Tomcat root + resolve the deployed context) + `bennu_hotswap_jsp`
// (copy one/all JSPs into the exploded webapp so Jasper recompiles them — no redeploy/restart).
mod tomcat;
// Unit tests (`bennu-test`): `bennu_discover_tests` (what in the project IS a test) +
// `bennu_run_tests` / `bennu_cancel_tests` (run a scope of them through Maven, streaming
// per-class results as Surefire writes its reports). Shares the build's single-run lock —
// two Maven processes on one tree fight over `target/`.
mod tests;

fn main() {
    // Seed the active profile FIRST — CRITICAL. Without this, `bennu_config_dir()` /
    // `bennu_data_dir()` (which the domain modules read directly) silently resolve the
    // `default` profile instead of the one the launcher spawned us on, so a dev
    // launcher would read config/data from the wrong (or empty) profile.
    arbor_core::prelude::init_active_profile();

    // Seed the classpath's extra JDK search directories from the settings (`jdk_paths`), so a
    // JDK installed somewhere non-standard is found. Re-seeded on config save (`config_cmds`).
    bennu_classpath::prelude::set_extra_jdk_homes(
        bennu_core::config::load().jdk_paths.iter().map(std::path::PathBuf::from).collect(),
    );

    // The framed-stdio plumbing (writer / sink / reverse channel / runtime), in one
    // call. No `plugin_host` / `api_installer` — bennu-be loads no plugins in Phase 0
    // (like merula-be).
    let app = arbor_be::App::new(arbor_be::BackendIo::new());

    // The state every handler gets: event egress + the reverse channel (for host
    // round-trips like reveal-in-explorer). `Arc`-shared across the dispatcher + any
    // background workers (the future indexing thread).
    let state = Arc::new(BennuState::new(app.sink()).with_host_caller(app.host_caller()));

    // The method routing, declared as the inventory of `#[handler]`s this binary
    // links. `inventory("")` covers them all — bennu-be links only its own handlers.
    let dispatcher =
        arbor_be::Dispatcher::new(Arc::clone(&state), app.runtime_handle()).inventory("");

    // Serve over framed stdio until the shell disconnects. No plugin host means the
    // `App`'s default post-`Hello` hook is a clean no-op (nothing to reload).
    if let Err(e) = app.run(dispatcher) {
        eprintln!("bennu-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
