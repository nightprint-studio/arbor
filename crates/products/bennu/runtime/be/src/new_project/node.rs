//! Node generators: the official scaffolding CLIs (Vite, Angular, SvelteKit, Vue, Next.js) run as
//! child processes, and a plain package written locally.
//!
//! ## Why the CLIs and not our own templates
//!
//! A framework's scaffolder is the only thing that is always current with the framework. Copying
//! its output into Bennu would be a snapshot that goes stale the day the framework changes its
//! defaults. What Bennu owns is the **invocation**: a closed set of options, each mapped to a flag
//! from a fixed table, and an argv built from them — no value the user typed reaches a command
//! line except the project name, which must already match `names::npm_name` (lowercase letters,
//! digits, `.`, `-`, `_`).
//!
//! The flags were taken from each tool's documentation. They change between major versions; a run
//! that fails on an unknown flag shows the tool's own message in the progress log.

use super::model::{Field, GenFile, Opt, Plan, PlanCx, Problem, Source, Step};
use super::names;
use super::render::{render, Vars};
use super::tools;

const PMS: &[(&str, &str)] = &[("npm", "npm"), ("pnpm", "pnpm"), ("yarn", "Yarn")];

fn pm_field() -> Field {
    let options = PMS
        .iter()
        .map(|(id, label)| {
            let missing = *id != "npm" && tools::locate(id).is_none();
            Opt::new(id, &if missing { format!("{label} (not found)") } else { (*label).to_string() })
        })
        .collect();
    Field::select("pm", "Package manager", "npm", options)
}

fn install_field() -> Field {
    Field::toggle("install", "Install dependencies", true).hint("Run the package manager after scaffolding")
}

/// The chosen package manager, checked against the allow-list and the machine.
fn package_manager(cx: &PlanCx) -> Result<&'static str, Problem> {
    let pm = cx.opts.str("pm");
    let (id, _) = PMS
        .iter()
        .find(|(id, _)| *id == pm)
        .ok_or_else(|| Problem::new("pm", "Pick npm, pnpm or Yarn"))?;
    if tools::locate(id).is_none() {
        return Err(Problem::new("pm", format!("`{id}` was not found on your PATH")));
    }
    Ok(id)
}

fn command_plan(argv: Vec<String>, name: &str, summary: Vec<String>, open: &[&str]) -> Plan {
    Plan {
        source: Source::Command { argv, produces: name.to_string() },
        post: Vec::new(),
        open: open.iter().map(|s| s.to_string()).collect(),
        summary,
    }
}

/// `argv` pieces from literals.
fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_string()).collect()
}

/// The invocation prefix for `create <tool>`: how each package manager spells it.
fn create_prefix(pm: &str, tool: &str) -> Vec<String> {
    match pm {
        "pnpm" => args(&["pnpm", "create", &format!("{tool}@latest")]),
        "yarn" => args(&["yarn", "create", tool]),
        _ => args(&["npm", "create", "--yes", &format!("{tool}@latest")]),
    }
}

/// Where tool-specific arguments go: npm needs a `--` separator, the others do not.
fn passthrough(pm: &str) -> Vec<String> {
    if pm == "npm" { args(&["--"]) } else { Vec::new() }
}

// ── Vite ─────────────────────────────────────────────────────────────────────────────────

const VITE_FRAMEWORKS: &[(&str, &str)] = &[
    ("vanilla", "Vanilla"),
    ("react", "React"),
    ("vue", "Vue"),
    ("svelte", "Svelte"),
    ("solid", "Solid"),
    ("preact", "Preact"),
    ("lit", "Lit"),
];

pub fn vite_fields() -> Vec<Field> {
    vec![
        Field::select(
            "framework",
            "Framework",
            "react",
            VITE_FRAMEWORKS.iter().map(|(id, l)| Opt::new(id, l)).collect(),
        ),
        Field::toggle("ts", "TypeScript", true),
        pm_field(),
        install_field(),
    ]
}

pub fn plan_vite(cx: &PlanCx) -> Result<Plan, Problem> {
    let pm = package_manager(cx)?;
    let fw = cx.opts.str("framework");
    if !VITE_FRAMEWORKS.iter().any(|(id, _)| *id == fw) {
        return Err(Problem::new("framework", "Pick one of the listed frameworks"));
    }
    let template = if cx.opts.flag("ts") { format!("{fw}-ts") } else { fw.to_string() };
    let mut argv = create_prefix(pm, "vite");
    argv.push(cx.name.to_string());
    argv.extend(passthrough(pm));
    argv.extend(args(&["--template", &template, "--no-interactive", "--no-immediate"]));
    let plan = command_plan(
        argv,
        cx.name,
        vec![format!("Vite project “{}” (template {template})", cx.name), format!("Scaffolded by create-vite with {pm}")],
        &["README.md", "package.json"],
    );
    Ok(with_install(plan, cx, pm))
}

/// Vite and Vue do not install on their own: a run that ends without `node_modules` is not what the
/// user expects from "create project", so the install is a post step they can switch off.
fn with_install(mut plan: Plan, cx: &PlanCx, pm: &str) -> Plan {
    if cx.opts.flag("install") {
        plan.post.push(Step { label: format!("Installing dependencies ({pm})"), argv: args(&[pm, "install"]) });
    }
    plan
}

// ── Angular ──────────────────────────────────────────────────────────────────────────────

pub fn angular_fields() -> Vec<Field> {
    vec![
        Field::select("style", "Stylesheet format", "css", vec![Opt::new("css", "CSS"), Opt::new("scss", "SCSS"), Opt::new("sass", "Sass"), Opt::new("less", "Less")]),
        Field::toggle("routing", "Routing", true),
        Field::toggle("ssr", "Server-side rendering", false),
        pm_field(),
        Field::toggle("install", "Install dependencies", true).hint("Done by the Angular CLI itself"),
    ]
}

pub fn plan_angular(cx: &PlanCx) -> Result<Plan, Problem> {
    let pm = package_manager(cx)?;
    // The CLI refuses names that are not identifiers; saying so here is kinder than its error.
    if !cx.name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) || cx.name.contains('.') {
        return Err(Problem::new("name", "Angular names start with a letter and can't contain dots"));
    }
    let style = cx.opts.str("style");
    if !["css", "scss", "sass", "less"].contains(&style) {
        return Err(Problem::new("style", "Pick one of the listed formats"));
    }
    let mut argv = args(&["npx", "--yes", "@angular/cli@latest", "new", cx.name, "--defaults", "--interactive=false", "--skip-git", "--package-manager", pm, "--style", style]);
    argv.push(format!("--routing={}", cx.opts.flag("routing")));
    argv.push(format!("--ssr={}", cx.opts.flag("ssr")));
    if !cx.opts.flag("install") {
        argv.push("--skip-install".into());
    }
    Ok(command_plan(
        argv,
        cx.name,
        vec![format!("Angular workspace “{}” ({style})", cx.name), "Scaffolded by the Angular CLI".into()],
        &["README.md", "package.json"],
    ))
}

// ── SvelteKit ────────────────────────────────────────────────────────────────────────────

pub fn svelte_fields() -> Vec<Field> {
    vec![
        Field::select(
            "template",
            "Template",
            "minimal",
            vec![
                Opt::described("minimal", "Minimal", "A bare SvelteKit app"),
                Opt::described("demo", "Demo", "A small app showing the main features"),
                Opt::described("library", "Library", "A package built with svelte-package"),
            ],
        ),
        Field::select("types", "Type checking", "ts", vec![Opt::new("ts", "TypeScript"), Opt::new("jsdoc", "JavaScript with JSDoc"), Opt::new("none", "None")]),
        pm_field(),
        install_field(),
    ]
}

pub fn plan_svelte(cx: &PlanCx) -> Result<Plan, Problem> {
    let pm = package_manager(cx)?;
    let template = cx.opts.str("template");
    if !["minimal", "demo", "library"].contains(&template) {
        return Err(Problem::new("template", "Pick one of the listed templates"));
    }
    let mut argv = args(&["npx", "--yes", "sv", "create", cx.name, "--template", template, "--no-add-ons"]);
    match cx.opts.str("types") {
        "ts" => argv.extend(args(&["--types", "ts"])),
        "jsdoc" => argv.extend(args(&["--types", "jsdoc"])),
        "none" => argv.push("--no-types".into()),
        _ => return Err(Problem::new("types", "Pick one of the listed options")),
    }
    if cx.opts.flag("install") {
        argv.extend(args(&["--install", pm]));
    } else {
        argv.push("--no-install".into());
    }
    Ok(command_plan(
        argv,
        cx.name,
        vec![format!("SvelteKit project “{}” ({template})", cx.name), "Scaffolded by `sv create`".into()],
        &["README.md", "package.json"],
    ))
}

// ── Vue ──────────────────────────────────────────────────────────────────────────────────

pub fn vue_fields() -> Vec<Field> {
    vec![
        Field::toggle("ts", "TypeScript", true),
        Field::toggle("router", "Vue Router", true),
        Field::toggle("pinia", "Pinia (state management)", false),
        Field::toggle("vitest", "Vitest (unit tests)", false),
        Field::toggle("eslint", "ESLint", false),
        Field::toggle("prettier", "Prettier", false).advanced(),
        pm_field(),
        install_field(),
    ]
}

pub fn plan_vue(cx: &PlanCx) -> Result<Plan, Problem> {
    let pm = package_manager(cx)?;
    let chosen: Vec<&str> = ["ts", "router", "pinia", "vitest", "eslint", "prettier"]
        .into_iter()
        .filter(|f| cx.opts.flag(f))
        .collect();
    let mut argv = create_prefix(pm, "vue");
    argv.push(cx.name.to_string());
    argv.extend(passthrough(pm));
    if chosen.is_empty() {
        argv.push("--default".into());
    } else {
        argv.extend(chosen.iter().map(|f| format!("--{f}")));
    }
    Ok(with_install(
        command_plan(
            argv,
            cx.name,
            vec![format!("Vue project “{}”", cx.name), if chosen.is_empty() { "Default features".into() } else { chosen.join(", ") }],
            &["README.md", "package.json"],
        ),
        cx,
        pm,
    ))
}

// ── Next.js ──────────────────────────────────────────────────────────────────────────────

pub fn next_fields() -> Vec<Field> {
    vec![
        Field::toggle("ts", "TypeScript", true),
        Field::toggle("app_router", "App Router", true),
        Field::toggle("tailwind", "Tailwind CSS", true),
        Field::toggle("eslint", "ESLint", true),
        Field::toggle("src_dir", "Use a src/ folder", false),
        pm_field(),
        install_field(),
    ]
}

pub fn plan_next(cx: &PlanCx) -> Result<Plan, Problem> {
    let pm = package_manager(cx)?;
    let on = |key: &str, yes: &'static str, no: &'static str| if cx.opts.flag(key) { yes } else { no };
    let mut argv = args(&["npx", "--yes", "create-next-app@latest", cx.name]);
    argv.extend(args(&[
        on("ts", "--ts", "--js"),
        on("app_router", "--app", "--no-app"),
        on("tailwind", "--tailwind", "--no-tailwind"),
        on("eslint", "--eslint", "--no-eslint"),
        on("src_dir", "--src-dir", "--no-src-dir"),
        "--import-alias",
        "@/*",
        "--disable-git",
        "--yes",
    ]));
    argv.push(format!("--use-{pm}"));
    if !cx.opts.flag("install") {
        argv.push("--skip-install".into());
    }
    Ok(command_plan(
        argv,
        cx.name,
        vec![format!("Next.js app “{}”", cx.name), "Scaffolded by create-next-app".into()],
        &["README.md", "package.json"],
    ))
}

// ── plain package ────────────────────────────────────────────────────────────────────────

const PACKAGE_JSON: &str = include_str!("templates/node/package.json.tpl");
const TSCONFIG: &str = include_str!("templates/node/tsconfig.json.tpl");
const INDEX_JS: &str = include_str!("templates/node/index.js.tpl");
const INDEX_TS: &str = include_str!("templates/node/index.ts.tpl");
const GITIGNORE: &str = include_str!("templates/node/gitignore.tpl");
const README: &str = include_str!("templates/node/README.md.tpl");

pub fn package_fields() -> Vec<Field> {
    vec![
        Field::toggle("ts", "TypeScript", false),
        Field::select(
            "module",
            "Module system",
            "esm",
            vec![Opt::new("esm", "ES modules (import / export)"), Opt::new("commonjs", "CommonJS (require)")],
        ),
        Field::select("license", "License", "ISC", ["ISC", "MIT", "Apache-2.0", "UNLICENSED"].iter().map(|l| Opt::new(l, l)).collect()).advanced(),
        pm_field(),
        Field::toggle("install", "Install dependencies", false).hint("Needed for TypeScript (adds typescript and @types/node)"),
    ]
}

pub fn plan_package(cx: &PlanCx) -> Result<Plan, Problem> {
    let ts = cx.opts.flag("ts");
    let esm = match cx.opts.str("module") {
        "esm" => true,
        "commonjs" => false,
        _ => return Err(Problem::new("module", "Pick ES modules or CommonJS")),
    };
    let license = cx.opts.str("license");
    if !["ISC", "MIT", "Apache-2.0", "UNLICENSED"].contains(&license) {
        return Err(Problem::new("license", "Pick one of the listed licenses"));
    }
    let install = cx.opts.flag("install");
    let pm = if install { package_manager(cx)? } else { "npm" };

    let mut v = Vars::new();
    v.set("name", cx.name)
        .set("title", names::title_case(cx.name))
        .set("license", license)
        .set("module_type", if esm { "module" } else { "commonjs" })
        .set("entry", if ts { "dist/index.js" } else { "index.js" })
        .flag("ts", ts)
        .set("pm", pm);
    v.set("import_os", if esm { "import os from \"node:os\";" } else { "const os = require(\"node:os\");" });
    let r = |t: &str| render(t, &v).map_err(|m| Problem::new("name", m));
    let mut files = vec![GenFile::new("package.json", r(PACKAGE_JSON)?), GenFile::new("README.md", r(README)?)];
    if ts {
        files.push(GenFile::new("tsconfig.json", r(TSCONFIG)?));
        files.push(GenFile::new("src/index.ts", r(INDEX_TS)?));
    } else {
        files.push(GenFile::new("index.js", r(INDEX_JS)?));
    }
    if cx.git {
        files.push(GenFile::new(".gitignore", r(GITIGNORE)?));
    }
    let mut plan = Plan::files(
        files,
        &[if ts { "src/index.ts" } else { "index.js" }, "package.json"],
        vec![
            format!("{} package “{}”", if ts { "TypeScript" } else { "Node.js" }, cx.name),
            format!("{} modules", if esm { "ES" } else { "CommonJS" }),
        ],
    );
    if install {
        plan.post.push(Step { label: format!("Installing dependencies ({pm})"), argv: args(&[pm, "install"]) });
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::new_project::model::Opts;
    use serde_json::{json, Map, Value};

    fn plan(f: fn(&PlanCx) -> Result<Plan, Problem>, fields: Vec<Field>, given: Value) -> Result<Plan, Problem> {
        // `pm` is always npm in tests, and npm may not be installed where they run: the machine
        // check is bypassed by testing the pure builders where it matters.
        let map: Map<String, Value> = given.as_object().cloned().unwrap_or_default();
        f(&PlanCx { name: "my-app", opts: Opts::resolve(&fields, &map), git: true })
    }

    fn argv(p: &Plan) -> Vec<String> {
        match &p.source {
            Source::Command { argv, .. } => argv.clone(),
            _ => panic!("expected a command"),
        }
    }

    #[test]
    fn create_prefix_per_package_manager() {
        assert_eq!(create_prefix("npm", "vite"), ["npm", "create", "--yes", "vite@latest"]);
        assert_eq!(create_prefix("pnpm", "vite"), ["pnpm", "create", "vite@latest"]);
        assert_eq!(create_prefix("yarn", "vue"), ["yarn", "create", "vue"]);
        assert_eq!(passthrough("npm"), ["--"]);
        assert!(passthrough("yarn").is_empty());
    }

    #[test]
    fn unknown_values_are_refused_before_a_command_exists() {
        // The allow-list checks run before the machine check only for values; `pm` is first, so
        // these tests only run when npm is installed. Where it is not, the pm problem is the
        // (equally valid) answer.
        let r = plan(plan_vite, vite_fields(), json!({"framework": "x; rm -rf /"}));
        let field = r.unwrap_err().field;
        assert!(field == "framework" || field == "pm", "{field}");
        let r = plan(plan_angular, angular_fields(), json!({"style": "sty le"}));
        let field = r.unwrap_err().field;
        assert!(field == "style" || field == "pm", "{field}");
    }

    #[test]
    fn vite_command_when_npm_exists() {
        if tools::locate("npm").is_none() {
            return;
        }
        let p = plan(plan_vite, vite_fields(), json!({"framework": "vue", "ts": true})).unwrap();
        let a = argv(&p);
        assert_eq!(&a[..5], ["npm", "create", "--yes", "vite@latest", "my-app"]);
        assert!(a.windows(2).any(|w| w == ["--template", "vue-ts"]));
        assert_eq!(p.post.len(), 1, "install step");
    }

    #[test]
    fn next_flags_follow_the_toggles() {
        if tools::locate("npm").is_none() {
            return;
        }
        let p = plan(plan_next, next_fields(), json!({"ts": false, "tailwind": false, "install": false})).unwrap();
        let a = argv(&p);
        for flag in ["--js", "--no-tailwind", "--app", "--eslint", "--no-src-dir", "--use-npm", "--skip-install", "--disable-git"] {
            assert!(a.iter().any(|x| x == flag), "{flag} in {a:?}");
        }
        assert!(a.windows(2).any(|w| w == ["--import-alias", "@/*"]));
    }

    #[test]
    fn angular_name_rule() {
        if tools::locate("npm").is_none() {
            return;
        }
        let check = |name: &'static str| {
            let f = angular_fields();
            plan_angular(&PlanCx { name, opts: Opts::resolve(&f, &Map::new()), git: false })
        };
        assert!(check("1app").is_err() && check("my.app").is_err() && check("myapp").is_ok());
    }

    #[test]
    fn plain_package_javascript_and_typescript() {
        let js = plan(plan_package, package_fields(), json!({})).unwrap();
        let Source::Files(files) = &js.source else { panic!() };
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"index.js") && paths.contains(&".gitignore") && !paths.contains(&"tsconfig.json"));
        let pkg = &files.iter().find(|f| f.path == "package.json").unwrap().content;
        assert!(pkg.contains("\"type\": \"module\"") && pkg.contains("\"name\": \"my-app\""));
        assert!(serde_json::from_str::<Value>(pkg).is_ok(), "package.json must be valid JSON: {pkg}");

        let ts = plan(plan_package, package_fields(), json!({"ts": true, "module": "commonjs"})).unwrap();
        let Source::Files(files) = &ts.source else { panic!() };
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"src/index.ts") && paths.contains(&"tsconfig.json"));
        let pkg = &files.iter().find(|f| f.path == "package.json").unwrap().content;
        assert!(serde_json::from_str::<Value>(pkg).is_ok(), "{pkg}");
        assert!(pkg.contains("typescript") && pkg.contains("\"type\": \"commonjs\""));
        let cjs = plan(plan_package, package_fields(), json!({"module": "commonjs"})).unwrap();
        let Source::Files(f) = &cjs.source else { panic!() };
        assert!(f.iter().find(|x| x.path == "index.js").unwrap().content.contains("require("));
    }
}
