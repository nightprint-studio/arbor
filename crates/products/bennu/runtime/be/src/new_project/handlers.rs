//! The wizard's RPC surface: catalog, form fields, preview, create, cancel.
//!
//! Handlers stay thin: validation lives in `names`/`target`, generation in the generator modules,
//! execution in `exec`. The only logic here is [`prepare`], the one place a request becomes either
//! a list of problems or a plan — preview and create both go through it, so what the wizard shows
//! is what the run does.

use std::path::PathBuf;

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use super::catalog::{self, Category, CATEGORIES};
use super::model::{CreateArgs, Field, GeneratorInfo, Opts, Plan, PlanCx, Preview, Problem, Source};
use super::remote_meta::cached_json;
use super::target::{self, TargetCheck, TargetState};
use super::tools::{self, ToolStatus};
use super::{exec, names, quarkus, rust, spring};

// ── catalog ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Catalog {
    categories: &'static [Category],
    generators: Vec<GeneratorInfo>,
    tools: Vec<ToolStatus>,
    /// The user's home folder, for a first-run default location.
    home: String,
}

fn home_dir() -> String {
    ["USERPROFILE", "HOME"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
        .replace('\\', "/")
}

/// The generators, which tools are installed, and which generators that makes unavailable.
#[arbor_rpc::handler]
fn bennu_np_catalog(_ctx: &BennuState) -> Result<Catalog, String> {
    let tools = tools::detect();
    Ok(Catalog { categories: CATEGORIES, generators: catalog::infos(&tools), tools, home: home_dir() })
}

// ── fields ───────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FieldsArgs {
    pub generator: String,
    /// Ignore the cached service metadata.
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Serialize)]
pub struct FieldsResult {
    fields: Vec<Field>,
    /// Built from an old copy because the service could not be reached.
    stale: bool,
}

/// The form for a generator. Static for most; for Spring, Quarkus and Bevy the options come from
/// the service (or crates.io), so the call is async and can fail with a message to show.
#[arbor_rpc::handler]
async fn bennu_np_fields(_ctx: &BennuState, args: FieldsArgs) -> Result<FieldsResult, String> {
    let g = catalog::find(&args.generator).ok_or("unknown generator")?;
    let fixed = |fields| FieldsResult { fields, stale: false };
    match g.id {
        "spring" => {
            let got = cached_json("spring-metadata", spring::META_URL, spring::META_ACCEPT, args.refresh).await?;
            Ok(FieldsResult { fields: spring::fields_from_metadata(&got.value), stale: got.stale })
        }
        "quarkus" => {
            let got = cached_json("quarkus-extensions", quarkus::EXTENSIONS_URL, "application/json", args.refresh).await?;
            Ok(FieldsResult { fields: quarkus::fields(quarkus::extension_options(&got.value)), stale: got.stale })
        }
        "rust_bevy" => Ok(fixed(bevy_fields_with_versions(args.refresh).await)),
        _ => Ok(fixed((g.fields)())),
    }
}

/// The Bevy form with its version picker filled from the crates.io index, when that is allowed and
/// reachable; otherwise the free-text field with a known-good default.
async fn bevy_fields_with_versions(refresh: bool) -> Vec<Field> {
    let mut fields = rust::bevy_fields();
    let cfg = bennu_core::config::load().cargo;
    if !cfg.crates_io {
        return fields;
    }
    let releases: Vec<(String, bool)> = crate::crates_io::versions_of("bevy", &cfg, refresh)
        .await
        .into_iter()
        .map(|v| (v.version, v.yanked))
        .collect();
    let minors = rust::bevy_minors(&releases);
    if !minors.is_empty() {
        if let Some(slot) = fields.iter_mut().find(|f| f.key == "bevy_version") {
            *slot = rust::bevy_version_field(&minors);
        }
    }
    fields
}

// ── preview / create ─────────────────────────────────────────────────────────────────────

struct Prepared {
    problems: Vec<Problem>,
    target: TargetCheck,
    plan: Option<Plan>,
    path: Option<PathBuf>,
}

/// Turn a request into problems or a plan. The single gate for both preview and create.
fn prepare(args: &CreateArgs) -> Prepared {
    let none = |problems: Vec<Problem>| Prepared {
        problems,
        target: TargetCheck { path: String::new(), state: TargetState::Incomplete, message: None },
        plan: None,
        path: None,
    };
    let Some(g) = catalog::find(&args.generator) else {
        return none(vec![Problem::new("generator", "Unknown project type")]);
    };
    let mut problems = Vec::new();

    let requires: Vec<String> = g.requires.iter().map(|s| (*s).to_string()).collect();
    if let Some(reason) = tools::missing_reason(&requires, &tools::detect()) {
        problems.push(Problem::new("generator", reason));
    }

    let name = args.name.trim();
    let name_ok = match names::validate_name(g.name_rule, name) {
        Ok(()) => true,
        Err(m) => {
            problems.push(Problem::new("name", m));
            false
        }
    };
    let location_ok = match target::location_problem(&args.location) {
        None => true,
        Some(m) => {
            problems.push(Problem::new("location", m));
            false
        }
    };

    let mut checked = TargetCheck { path: String::new(), state: TargetState::Incomplete, message: None };
    if name_ok && location_ok {
        checked = target::inspect(&args.location, name);
        if !checked.usable() {
            let field = if checked.state == TargetState::BadLocation { "location" } else { "name" };
            problems.push(Problem::new(field, checked.message.clone().unwrap_or_else(|| "This location can't be used".into())));
        }
    }

    let mut plan = None;
    if name_ok {
        let fields = (g.fields)();
        let cx = PlanCx { name, opts: Opts::resolve(&fields, &args.options), git: args.git };
        match (g.plan)(&cx) {
            Ok(p) => plan = Some(p),
            Err(p) => problems.push(p),
        }
    }
    let path = (name_ok && location_ok).then(|| target::join(&args.location, name)).flatten();
    Prepared { problems, target: checked, plan, path }
}

/// What a request would do, without doing it: the problems, the destination check and the files.
#[arbor_rpc::handler]
fn bennu_np_preview(_ctx: &BennuState, args: CreateArgs) -> Result<Preview, String> {
    let p = prepare(&args);
    let (mut files, mut summary, mut command) = (Vec::new(), Vec::new(), None);
    if let Some(plan) = &p.plan {
        summary = plan.summary.clone();
        match &plan.source {
            Source::Files(fs) => {
                files = fs.iter().map(|f| f.path.clone()).collect();
                files.sort();
                files.truncate(300);
            }
            Source::Command { argv, .. } => command = Some(argv.join(" ")),
            Source::Download { url } => command = Some(format!("GET {url}")),
        }
        if args.git {
            summary.push("Git repository (git init)".into());
        }
    }
    let ok = p.problems.is_empty() && p.plan.is_some();
    Ok(Preview { problems: p.problems, target: p.target, files, summary, command, ok })
}

#[derive(Deserialize)]
pub struct StartArgs {
    /// Chosen by the caller, which subscribes to the progress events under it before calling.
    pub run_id: String,
    pub request: CreateArgs,
}

#[derive(Serialize)]
pub struct Started {
    path: String,
}

/// Start creating a project. Returns at once; progress and the result arrive as
/// `arbor://bennu/new-project` events keyed by `run_id`.
#[arbor_rpc::handler]
fn bennu_np_create(ctx: &BennuState, args: StartArgs) -> Result<Started, String> {
    let p = prepare(&args.request);
    if let Some(first) = p.problems.first() {
        return Err(first.message.clone());
    }
    let (Some(plan), Some(path)) = (p.plan, p.path) else {
        return Err("nothing to create".into());
    };
    let shown = path.to_string_lossy().replace('\\', "/");
    exec::start(ctx.event_sink(), args.run_id, plan, path, args.request.git)?;
    Ok(Started { path: shown })
}

#[derive(Deserialize)]
pub struct CancelArgs {
    pub run_id: String,
}

/// Cancel a running creation: kills its process tree and removes everything it made.
#[arbor_rpc::handler]
fn bennu_np_cancel(_ctx: &BennuState, args: CancelArgs) -> Result<bool, String> {
    Ok(exec::cancel(&args.run_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn req(generator: &str, name: &str, location: &str, options: Value) -> CreateArgs {
        CreateArgs {
            generator: generator.into(),
            name: name.into(),
            location: location.into(),
            git: true,
            options: options.as_object().cloned().unwrap_or_default(),
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("arbor-np-handlers-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_good_request_has_a_plan_and_no_problems() {
        let d = scratch("good");
        let p = prepare(&req("maven", "my-app", &d.to_string_lossy(), json!({})));
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        assert!(p.plan.is_some() && p.path.is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn problems_are_attached_to_their_fields() {
        let p = prepare(&req("rust_bin", "2bad name", "relative/dir", json!({})));
        let fields: Vec<_> = p.problems.iter().map(|p| p.field.as_str()).collect();
        assert!(fields.contains(&"name") && fields.contains(&"location"), "{fields:?}");
        assert!(p.plan.is_none(), "an invalid name never reaches a generator");
        assert!(p.path.is_none());
        assert_eq!(prepare(&req("nope", "x", "/tmp", json!({}))).problems[0].field, "generator");
    }

    #[test]
    fn an_existing_non_empty_folder_blocks_the_request() {
        let d = scratch("busy");
        std::fs::create_dir_all(d.join("my-app")).unwrap();
        std::fs::write(d.join("my-app").join("x"), "1").unwrap();
        let p = prepare(&req("maven", "my-app", &d.to_string_lossy(), json!({})));
        assert!(p.problems.iter().any(|p| p.field == "name" && p.message.contains("isn't empty")));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn generator_problems_pass_through() {
        let d = scratch("gen");
        let p = prepare(&req("maven", "my-app", &d.to_string_lossy(), json!({"group": "a..b"})));
        assert!(p.problems.iter().any(|p| p.field == "group"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
