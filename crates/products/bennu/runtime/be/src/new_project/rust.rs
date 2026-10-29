//! Rust generators: binary, library, workspace, Bevy game, CLI (clap), web service (axum) and
//! WASM library — all written locally from the data files in `templates/rust/`.
//!
//! Cargo is not needed to create any of them (so none is ever shown disabled for lack of it); it is
//! only needed to build them. That is a choice over `cargo new`: the output is identical on every
//! machine, works offline, and can be previewed file by file before anything is written.

use super::model::{Field, Opt, Plan, PlanCx, Problem};
use super::names;
use super::render::{render_set, TemplateSet, Vars};

const EDITIONS: &[&str] = &["2024", "2021", "2018"];

/// Bevy releases older than this lack the APIs the template uses (`Camera2d`, `Sprite` as a
/// component, `Time::delta_secs`).
pub const BEVY_MIN_MINOR: u32 = 15;

/// What the version field offers when crates.io can't be asked.
pub const BEVY_FALLBACK: &str = "0.16";

fn license_options() -> Vec<Opt> {
    vec![
        Opt::new("", "None"),
        Opt::new("MIT", "MIT"),
        Opt::new("Apache-2.0", "Apache 2.0"),
        Opt::new("MIT OR Apache-2.0", "MIT or Apache 2.0 (the Rust convention)"),
        Opt::new("GPL-3.0-only", "GPL 3.0"),
        Opt::new("BSD-3-Clause", "BSD 3-Clause"),
    ]
}

/// Edition, license and author — on every Rust generator.
fn common_fields() -> Vec<Field> {
    vec![
        Field::select(
            "edition",
            "Edition",
            "2024",
            EDITIONS.iter().map(|e| Opt::new(e, e)).collect(),
        ),
        Field::select("license", "License", "", license_options())
            .hint("Recorded in Cargo.toml; the MIT text is also written as LICENSE-MIT")
            .advanced(),
        Field::text("author", "Author", "")
            .placeholder("Name for the license text")
            .when("license", "MIT")
            .advanced(),
    ]
}

fn with_common(mut head: Vec<Field>) -> Vec<Field> {
    head.extend(common_fields());
    head
}

pub fn bin_fields() -> Vec<Field> {
    with_common(Vec::new())
}
pub fn lib_fields() -> Vec<Field> {
    with_common(Vec::new())
}
pub fn workspace_fields() -> Vec<Field> {
    with_common(Vec::new())
}
pub fn cli_fields() -> Vec<Field> {
    with_common(Vec::new())
}
pub fn wasm_fields() -> Vec<Field> {
    with_common(Vec::new())
}
pub fn axum_fields() -> Vec<Field> {
    with_common(vec![Field::text("port", "Port", "3000").hint("The port the sample server listens on")])
}

/// The Bevy form, with `bevy_version` as free text. The async field loader swaps in a select of
/// the versions crates.io knows ([`bevy_version_field`]).
pub fn bevy_fields() -> Vec<Field> {
    with_common(vec![
        Field::text("bevy_version", "Bevy version", BEVY_FALLBACK)
            .hint("Major.minor; Bevy 0.15 or newer"),
        Field::toggle("dynamic", "Fast rebuilds (dynamic linking)", true)
            .hint("Adds a `dev` feature: cargo run --features dev"),
        Field::toggle("ron", "Settings file (RON)", false)
            .hint("Loads the window title and background from assets/data/settings.ron"),
    ])
}

/// The version picker for Bevy, built from `versions` (newest first, `major.minor` strings).
pub fn bevy_version_field(versions: &[String]) -> Field {
    let default = versions.first().map_or(BEVY_FALLBACK, String::as_str);
    Field::select(
        "bevy_version",
        "Bevy version",
        default,
        versions.iter().map(|v| Opt::new(v, &format!("Bevy {v}"))).collect(),
    )
    .hint("From crates.io; the newest is selected")
}

/// `major.minor` releases of Bevy worth offering, newest first, from the raw index versions.
/// Pre-releases and yanked versions are never offered; so is anything below [`BEVY_MIN_MINOR`].
pub fn bevy_minors(releases: &[(String, bool)]) -> Vec<String> {
    let mut minors: Vec<(u32, u32)> = Vec::new();
    for (version, yanked) in releases {
        if *yanked {
            continue;
        }
        let mut parts = version.split('.');
        let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else { continue };
        // A pre-release has a suffix on the patch component (`0.17.0-rc.1`).
        let (Ok(a), Ok(b), Ok(_)) = (a.parse::<u32>(), b.parse::<u32>(), c.parse::<u32>()) else { continue };
        if a == 0 && b >= BEVY_MIN_MINOR && !minors.contains(&(a, b)) {
            minors.push((a, b));
        }
    }
    minors.sort_unstable_by(|x, y| y.cmp(x));
    minors.truncate(6);
    minors.into_iter().map(|(a, b)| format!("{a}.{b}")).collect()
}

// ── template sets ────────────────────────────────────────────────────────────────────────

const COMMON: TemplateSet = &[
    ("{{#if git}}.gitignore{{/if}}", include_str!("templates/rust/gitignore.tpl")),
    ("README.md", include_str!("templates/rust/README.md.tpl")),
    ("{{#if mit}}LICENSE-MIT{{/if}}", include_str!("templates/rust/LICENSE-MIT.tpl")),
];

const BIN: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/bin/Cargo.toml.tpl")),
    ("src/main.rs", include_str!("templates/rust/bin/main.rs.tpl")),
];
const LIB: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/lib/Cargo.toml.tpl")),
    ("src/lib.rs", include_str!("templates/rust/lib/lib.rs.tpl")),
];
const WORKSPACE: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/workspace/Cargo.toml.tpl")),
    ("crates/{{name}}-core/Cargo.toml", include_str!("templates/rust/workspace/core.Cargo.toml.tpl")),
    ("crates/{{name}}-core/src/lib.rs", include_str!("templates/rust/workspace/core.lib.rs.tpl")),
    ("crates/{{name}}-app/Cargo.toml", include_str!("templates/rust/workspace/app.Cargo.toml.tpl")),
    ("crates/{{name}}-app/src/main.rs", include_str!("templates/rust/workspace/app.main.rs.tpl")),
];
const CLI: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/cli/Cargo.toml.tpl")),
    ("src/main.rs", include_str!("templates/rust/cli/main.rs.tpl")),
];
const AXUM: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/axum/Cargo.toml.tpl")),
    ("src/main.rs", include_str!("templates/rust/axum/main.rs.tpl")),
];
const WASM: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/wasm/Cargo.toml.tpl")),
    ("src/lib.rs", include_str!("templates/rust/wasm/lib.rs.tpl")),
    ("index.html", include_str!("templates/rust/wasm/index.html.tpl")),
];
const BEVY: TemplateSet = &[
    ("Cargo.toml", include_str!("templates/rust/bevy/Cargo.toml.tpl")),
    ("src/main.rs", include_str!("templates/rust/bevy/main.rs.tpl")),
    ("src/game.rs", include_str!("templates/rust/bevy/game.rs.tpl")),
    ("{{#if ron}}src/settings.rs{{/if}}", include_str!("templates/rust/bevy/settings.rs.tpl")),
    ("{{#if ron}}assets/data/settings.ron{{/if}}", include_str!("templates/rust/bevy/settings.ron.tpl")),
    ("{{#unless ron}}assets/README.md{{/unless}}", include_str!("templates/rust/bevy/assets-README.md.tpl")),
    (".cargo/config.toml", include_str!("templates/rust/bevy/cargo-config.toml.tpl")),
];

// ── variables ────────────────────────────────────────────────────────────────────────────

/// Days since 1970-01-01 → calendar year (civil-from-days, proleptic Gregorian).
fn year_of_days(days: i64) -> i64 {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mp = (5 * (doe - (365 * yoe + yoe / 4 - yoe / 100)) + 2) / 153;
    let y = yoe + era * 400;
    if mp >= 10 { y + 1 } else { y }
}

fn current_year() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    year_of_days((secs / 86_400) as i64)
}

fn rust_vars(cx: &PlanCx, description: &str, run_cmd: &str, keep_lock: bool) -> Result<Vars, Problem> {
    let edition = cx.opts.str("edition");
    if !EDITIONS.contains(&edition) {
        return Err(Problem::new("edition", "Pick one of the listed editions"));
    }
    let license = cx.opts.str("license");
    if !license_options().iter().any(|o| o.value == license) {
        return Err(Problem::new("license", "Pick one of the listed licenses"));
    }
    let author = cx.opts.str("author");
    if author.chars().any(char::is_control) || author.contains('"') {
        return Err(Problem::new("author", "Letters, digits and punctuation only"));
    }
    let mut v = Vars::new();
    v.set("name", cx.name)
        .set("ident", names::crate_ident(cx.name))
        .set("title", names::title_case(cx.name))
        .set("edition", edition)
        .set("resolver", if edition == "2024" { "3" } else { "2" })
        .set("license", license)
        .flag("mit", license.contains("MIT"))
        .set("author", if author.is_empty() { "The authors" } else { author })
        .set("year", current_year().to_string())
        .set("description", description)
        .set("run_cmd", run_cmd)
        .set("readme_extra", "")
        .flag("git", cx.git)
        .flag("keep_lock", keep_lock);
    Ok(v)
}

fn build(cx: &PlanCx, vars: Vars, sets: &[TemplateSet], summary: Vec<String>, open: &[&str]) -> Result<Plan, Problem> {
    let mut files = Vec::new();
    for set in sets.iter().chain(std::iter::once(&COMMON)) {
        files.extend(render_set(set, &vars).map_err(|m| Problem::new("name", m))?);
    }
    let mut summary = summary;
    summary.insert(0, format!("Cargo project “{}” (edition {})", cx.name, cx.opts.str("edition")));
    Ok(Plan::files(files, open, summary))
}

pub fn plan_bin(cx: &PlanCx) -> Result<Plan, Problem> {
    let v = rust_vars(cx, "A Rust binary.", "cargo run", true)?;
    build(cx, v, &[BIN], vec!["A binary crate with src/main.rs".into()], &["src/main.rs"])
}

pub fn plan_lib(cx: &PlanCx) -> Result<Plan, Problem> {
    let v = rust_vars(cx, "A Rust library.", "cargo build", false)?;
    build(cx, v, &[LIB], vec!["A library crate with a first unit test".into()], &["src/lib.rs"])
}

pub fn plan_workspace(cx: &PlanCx) -> Result<Plan, Problem> {
    names::crate_name(&format!("{}-core", cx.name)).map_err(|m| Problem::new("name", m))?;
    let v = rust_vars(cx, "A Cargo workspace.", &format!("cargo run -p {}-app", cx.name), true)?;
    let app_main = format!("crates/{}-app/src/main.rs", cx.name);
    build(
        cx,
        v,
        &[WORKSPACE],
        vec![
            format!("A workspace with `{0}-core` (library) and `{0}-app` (binary)", cx.name),
            "Shared version, edition and dependencies in [workspace]".into(),
        ],
        &[app_main.as_str()],
    )
}

pub fn plan_cli(cx: &PlanCx) -> Result<Plan, Problem> {
    let v = rust_vars(cx, "A command-line tool.", "cargo run -- --help", true)?;
    build(cx, v, &[CLI], vec!["A command-line app with clap (derive)".into()], &["src/main.rs"])
}

pub fn plan_axum(cx: &PlanCx) -> Result<Plan, Problem> {
    let port = cx.opts.str("port");
    match port.parse::<u16>() {
        Ok(p) if p >= 1 => {}
        _ => return Err(Problem::new("port", "A port number between 1 and 65535")),
    }
    let mut v = rust_vars(cx, "A web service.", "cargo run", true)?;
    v.set("port", port);
    build(
        cx,
        v,
        &[AXUM],
        vec![format!("An axum + tokio service on port {port}"), "Routes: / and /health".into()],
        &["src/main.rs"],
    )
}

pub fn plan_wasm(cx: &PlanCx) -> Result<Plan, Problem> {
    let mut v = rust_vars(cx, "A WebAssembly library.", "cargo build --target wasm32-unknown-unknown", false)?;
    v.set(
        "readme_extra",
        "Build the package for the browser with [wasm-pack](https://rustwasm.github.io/wasm-pack/):\n\n```\nwasm-pack build --target web\n```\n\nThen serve this folder and open `index.html`.",
    );
    build(
        cx,
        v,
        &[WASM],
        vec!["A cdylib with wasm-bindgen and a greet() export".into(), "index.html that loads the generated package".into()],
        &["src/lib.rs"],
    )
}

pub fn plan_bevy(cx: &PlanCx) -> Result<Plan, Problem> {
    let version = cx.opts.str("bevy_version");
    let minor = version
        .strip_prefix("0.")
        .and_then(|m| m.split('.').next())
        .and_then(|m| m.parse::<u32>().ok());
    match minor {
        Some(m) if m >= BEVY_MIN_MINOR && version.chars().all(|c| c.is_ascii_digit() || c == '.') => {}
        _ => {
            return Err(Problem::new(
                "bevy_version",
                format!("Use a 0.{BEVY_MIN_MINOR} or newer Bevy release, like {BEVY_FALLBACK}"),
            ))
        }
    }
    let dynamic = cx.opts.flag("dynamic");
    let run = if dynamic { "cargo run --features dev" } else { "cargo run" };
    let mut v = rust_vars(cx, "A game built with Bevy.", run, true)?;
    v.set("bevy_version", version).flag("dynamic", dynamic).flag("ron", cx.opts.flag("ron"));
    let mut summary = vec![
        format!("Bevy {version}: a camera, a sprite, a component, a system and a plugin"),
        "Dev profile tuned for Bevy (own code opt-level 1, dependencies 3)".into(),
        ".cargo/config.toml and an assets/ folder".into(),
    ];
    if dynamic {
        summary.push("`dev` feature for dynamic linking".into());
    }
    if cx.opts.flag("ron") {
        summary.push("assets/data/settings.ron read at startup".into());
    }
    build(cx, v, &[BEVY], summary, &["src/game.rs", "src/main.rs"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::{Opts, Source};
    use serde_json::{json, Map, Value};

    fn plan_with(f: fn(&PlanCx) -> Result<Plan, Problem>, fields: Vec<Field>, name: &str, given: Value) -> Result<Plan, Problem> {
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        f(&PlanCx { name, opts: Opts::resolve(&fields, &map), git: true })
    }

    fn files(p: &Plan) -> Vec<(String, String)> {
        match &p.source {
            Source::Files(f) => f.iter().map(|g| (g.path.clone(), g.content.clone())).collect(),
            _ => panic!(),
        }
    }

    fn paths(p: &Plan) -> Vec<String> {
        files(p).into_iter().map(|(p, _)| p).collect()
    }

    #[test]
    fn year_conversion() {
        assert_eq!(year_of_days(0), 1970);
        assert_eq!(year_of_days(365), 1971);
        assert_eq!(year_of_days(19_723), 2024); // 2024-01-01
        assert_eq!(year_of_days(19_722), 2023); // 2023-12-31
    }

    #[test]
    fn every_generator_renders_with_no_leftover_placeholders() {
        let gens: Vec<(fn(&PlanCx) -> Result<Plan, Problem>, Vec<Field>)> = vec![
            (plan_bin, bin_fields()),
            (plan_lib, lib_fields()),
            (plan_workspace, workspace_fields()),
            (plan_cli, cli_fields()),
            (plan_axum, axum_fields()),
            (plan_wasm, wasm_fields()),
            (plan_bevy, bevy_fields()),
        ];
        for (f, fields) in gens {
            let plan = plan_with(f, fields, "my-thing", json!({"license": "MIT", "ron": true})).unwrap();
            for (path, content) in files(&plan) {
                assert!(!content.contains("{{") && !content.contains("}}"), "{path} has a leftover tag");
            }
            assert!(paths(&plan).contains(&"README.md".to_string()));
            assert!(paths(&plan).contains(&"LICENSE-MIT".to_string()));
        }
    }

    #[test]
    fn bin_has_main_and_lib_ignores_the_lockfile() {
        let b = plan_with(plan_bin, bin_fields(), "tool", json!({})).unwrap();
        assert!(paths(&b).contains(&"src/main.rs".to_string()));
        let ignore = |p: &Plan| files(p).into_iter().find(|(n, _)| n == ".gitignore").unwrap().1;
        assert!(!ignore(&b).contains("Cargo.lock"));
        let l = plan_with(plan_lib, lib_fields(), "lib", json!({})).unwrap();
        assert!(ignore(&l).contains("Cargo.lock"));
    }

    #[test]
    fn workspace_layout_and_resolver() {
        let p = plan_with(plan_workspace, workspace_fields(), "game", json!({})).unwrap();
        let ps = paths(&p);
        assert!(ps.contains(&"crates/game-core/src/lib.rs".to_string()));
        assert!(ps.contains(&"crates/game-app/Cargo.toml".to_string()));
        let root = files(&p).into_iter().find(|(n, _)| n == "Cargo.toml").unwrap().1;
        assert!(root.contains("resolver = \"3\""));
        let app = files(&p).into_iter().find(|(n, _)| n == "crates/game-app/src/main.rs").unwrap().1;
        assert!(app.contains("game_core::greeting"));
        let old = plan_with(plan_workspace, workspace_fields(), "game", json!({"edition": "2021"})).unwrap();
        assert!(files(&old).into_iter().find(|(n, _)| n == "Cargo.toml").unwrap().1.contains("resolver = \"2\""));
    }

    #[test]
    fn bevy_options() {
        let on = plan_with(plan_bevy, bevy_fields(), "sky", json!({"bevy_version": "0.17", "ron": true})).unwrap();
        let ps = paths(&on);
        assert!(ps.contains(&"src/settings.rs".to_string()) && ps.contains(&"assets/data/settings.ron".to_string()));
        assert!(ps.contains(&".cargo/config.toml".to_string()));
        let toml = files(&on).into_iter().find(|(n, _)| n == "Cargo.toml").unwrap().1;
        assert!(toml.contains("bevy = \"0.17\"") && toml.contains("dev = [\"bevy/dynamic_linking\"]") && toml.contains("ron = "));

        let off = plan_with(plan_bevy, bevy_fields(), "sky", json!({"dynamic": false})).unwrap();
        let toml = files(&off).into_iter().find(|(n, _)| n == "Cargo.toml").unwrap().1;
        assert!(!toml.contains("dynamic_linking") && !toml.contains("ron"));
        assert!(paths(&off).contains(&"assets/README.md".to_string()));
    }

    #[test]
    fn bevy_refuses_old_or_odd_versions() {
        for bad in ["0.9", "1.0", "0.16; rm", "x"] {
            let r = plan_with(plan_bevy, bevy_fields(), "sky", json!({"bevy_version": bad}));
            assert_eq!(r.unwrap_err().field, "bevy_version", "{bad}");
        }
    }

    #[test]
    fn bevy_minor_selection() {
        let rel = |v: &str, y: bool| (v.to_string(), y);
        let got = bevy_minors(&[
            rel("0.14.2", false),
            rel("0.15.0", false),
            rel("0.15.3", false),
            rel("0.16.0", false),
            rel("0.17.0-rc.1", false),
            rel("0.17.1", true),
            rel("0.16.1", false),
        ]);
        assert_eq!(got, ["0.16", "0.15"]);
        assert_eq!(bevy_version_field(&got).default, json!("0.16"));
    }

    #[test]
    fn axum_port_is_validated() {
        assert!(plan_with(plan_axum, axum_fields(), "svc", json!({"port": "8080"})).is_ok());
        for bad in ["0", "70000", "abc", ""] {
            assert!(plan_with(plan_axum, axum_fields(), "svc", json!({"port": bad})).is_err(), "{bad}");
        }
    }

    #[test]
    fn license_and_edition_are_allow_listed() {
        assert!(plan_with(plan_bin, bin_fields(), "x", json!({"edition": "2015"})).is_err());
        assert!(plan_with(plan_bin, bin_fields(), "x", json!({"license": "WTFPL"})).is_err());
    }
}
