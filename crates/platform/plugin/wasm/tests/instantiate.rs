//! Does a real component actually come up?
//!
//! Everything else in this crate is tested without a runtime: the index resolves, the gates
//! refuse, the ordering holds. None of that answers the question that matters most — whether a
//! module somebody built against `wit/` links against what this host offers.
//!
//! It cannot be a normal test because it needs a `.wasm`, and building one needs a toolchain
//! this repo does not own. So it is `#[ignore]`d and takes its fixture from the environment:
//!
//! ```sh
//! cargo build --release --target wasm32-wasip2      # in the package's repo
//! ARBOR_WASM_FIXTURE=…/target/wasm32-wasip2/release/cloud_gcs.wasm \
//!   ARBOR_WASM_WORLD=cloud-provider \
//!   cargo test -p arbor-plugin-wasm --features runtime -- --ignored
//! ```
//!
//! What it proves is narrow and load-bearing: the component's **imports resolve** against the
//! host functions this crate links, and its **exports match** the world it claims. Both are
//! failures that are otherwise invisible until somebody opens a file and nothing happens.

#![cfg(feature = "runtime")]

use std::sync::Arc;

use arbor_plugin_wasm::prelude::{GuestCaps, NoServices, Services, WasmHost};

fn fixture() -> Option<(std::path::PathBuf, String)> {
    let path = std::env::var("ARBOR_WASM_FIXTURE").ok()?;
    let world = std::env::var("ARBOR_WASM_WORLD").unwrap_or_else(|_| "cloud-provider".into());
    Some((std::path::PathBuf::from(path), world))
}

#[test]
#[ignore = "needs ARBOR_WASM_FIXTURE — see the module docs"]
fn a_real_component_instantiates() {
    let Some((module, world)) = fixture() else {
        panic!("set ARBOR_WASM_FIXTURE to a built .wasm");
    };
    assert!(module.is_file(), "no such module: {}", module.display());

    let host = WasmHost::new().expect("engine");
    // The capabilities do not matter for instantiation — nothing is called yet — but they are
    // what a real caller passes, so the shape stays honest.
    let caps = GuestCaps::new("fixture", vec!["example.com".into()], vec!["oauth".into()]);
    let services: Services = Arc::new(NoServices);

    let result = match world.as_str() {
        // Only `studio-format` still has a typed opener — it is Arbor's own interface. Every
        // other world, the cloud provider included, comes up through the dynamic path, which
        // is also how a plugin reaches it.
        "studio-format" => host.open_studio(&module, caps, services).map(|_| ()),
        _ => host.open_dynamic(&module, caps, services).map(|_| ()),
    };
    result.unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()));
}

#[test]
#[ignore = "needs ARBOR_WASM_FIXTURE — see the module docs"]
fn the_same_component_can_come_up_twice() {
    // Two instances share one compilation and must not share anything else. If this fails
    // where the first test passed, the engine is caching something per-module that is
    // per-instance.
    let Some((module, world)) = fixture() else { return };
    let host = WasmHost::new().expect("engine");
    for i in 0..2 {
        let caps = GuestCaps::new(format!("fixture-{i}"), vec![], vec![]);
        let services: Services = Arc::new(NoServices);
        let r = match world.as_str() {
            "studio-format" => host.open_studio(&module, caps, services).map(|_| ()),
            _ => host.open_dynamic(&module, caps, services).map(|_| ()),
        };
        r.unwrap_or_else(|e| panic!("instance {i} did not come up: {e}"));
    }
}

/// Open any component through the **dynamic** path and print what it exports.
///
/// The one that matters now: the typed opener knows one world, and the whole point of the
/// extension seam is that a package can export an interface this crate has never heard of.
/// This proves that end — a module comes up, and its own type information says what is
/// callable on it, with no `bindgen!` anywhere in the loop.
///
/// ```sh
/// ARBOR_WASM_FIXTURE=…/shader_preview_meshes.wasm \
///   cargo test -p arbor-plugin-wasm --features runtime -- --ignored dynamic --nocapture
/// ```
#[test]
#[ignore = "needs ARBOR_WASM_FIXTURE — see the module docs"]
fn a_component_of_an_unknown_world_still_reports_its_surface() {
    let Some((module, _)) = fixture() else {
        panic!("set ARBOR_WASM_FIXTURE to a built .wasm");
    };
    let host = WasmHost::new().expect("engine");
    let caps = GuestCaps::new("fixture", vec![], vec![]);
    let services: Services = Arc::new(NoServices);

    let guest = host
        .open_dynamic(&module, caps, services)
        .unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()));

    let surface = guest.surface(host.engine());
    for iface in &surface {
        println!("interface {}", iface.name);
        for f in &iface.funcs {
            println!("  {}({} args) -> {}", f.name, f.params, f.results);
        }
    }
    assert!(
        !surface.is_empty(),
        "{} exports no interface — a package that provides something must export it",
        module.display()
    );
}

/// Call an unknown component's functions, with JSON in and JSON out.
///
/// The end-to-end proof of the extension seam: no `bindgen!`, no world this crate knows, no
/// generated types — the arguments are coerced from JSON against the component's own type
/// information, and the answer comes back the same way. If this passes, a plugin's
/// `arbor.ext.call` works for an interface nobody here has heard of.
///
/// ```sh
/// ARBOR_WASM_FIXTURE=…/shader_preview_meshes.wasm ARBOR_WASM_CALL=build \
///   cargo test -p arbor-plugin-wasm --features runtime -- --ignored dynamic_call --nocapture
/// ```
#[test]
#[ignore = "needs ARBOR_WASM_FIXTURE — see the module docs"]
fn a_dynamic_call_crosses_in_json_and_comes_back_in_json() {
    let Some((module, _)) = fixture() else {
        panic!("set ARBOR_WASM_FIXTURE to a built .wasm");
    };
    let host = WasmHost::new().expect("engine");
    let caps = GuestCaps::new("fixture", vec![], vec![]);
    let services: Services = Arc::new(NoServices);
    let mut guest = host
        .open_dynamic(&module, caps, services)
        .unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()));

    let surface = guest.surface(host.engine());
    let iface = surface.first().expect("no exported interface").name.clone();

    // Discovery first: a caller that guessed the function name would be a caller that has to
    // be updated when the package is.
    let names: Vec<&str> = surface[0].funcs.iter().map(|f| f.name.as_str()).collect();
    println!("interface {iface}: {names:?}");

    if names.contains(&"catalogue") {
        let out = guest
            .call(Some(&iface), "catalogue", &[])
            .expect("catalogue should answer");
        let entries = out.as_array().expect("a catalogue is a list");
        assert!(!entries.is_empty(), "the catalogue is empty");
        println!("catalogue: {} entries", entries.len());

        // Every id the catalogue offered has to build — the two are separate code paths in
        // the guest, and a listed-but-unbuildable id is a picker entry that does nothing.
        for e in entries {
            let id = e.get("id").and_then(|v| v.as_str()).expect("an entry has an id");
            let mesh = guest
                .call(
                    Some(&iface),
                    "build",
                    &[serde_json::json!(id), serde_json::json!("{}")],
                )
                .unwrap_or_else(|err| panic!("{id} did not build: {err}"));
            let n = mesh
                .get("positions")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            assert!(n > 0 && n % 3 == 0, "{id}: {n} position floats");
            println!("  {id}: {} vertices", n / 3);
        }

        // And the error path: a guest's own `Err` has to arrive as this call's error, not as
        // an Ok carrying a result object nobody unwraps.
        let err = guest
            .call(
                Some(&iface),
                "build",
                &[serde_json::json!("nope"), serde_json::json!("{}")],
            )
            .expect_err("an unknown id should fail");
        println!("refusal reads: {err}");
        assert!(err.contains("nope"), "the refusal does not name what was asked for: {err}");
    }

    // A `language-intel` backend (the Patina one, `arbor-extensions-private/plugins/patina`): every call carries
    // the whole workspace, so one request object drives all the questions.
    if names.contains(&"completion") && names.contains(&"diagnostics") {
        let src = "fn f() {\n    let a = 1\n    a = 2\n}\n";
        let request = serde_json::json!({
            "workspace": {
                "dir": "/mods/marmo",
                "manifest": "(id: \"marmo\")",
                "files": [{ "path": "/mods/marmo/script/tally.ptn", "text": "pub struct Tally { pub n: int }\n" }],
            },
            "path": "/mods/marmo/script/mine.ptn",
            "text": src,
            "lang": "",
        });
        let diags = guest
            .call(Some(&iface), "diagnostics", &[request.clone()])
            .expect("diagnostics should answer");
        println!("diagnostics: {diags}");
        let codes: Vec<&str> = diags.as_array().expect("a list").iter().filter_map(|d| d.get("code")?.as_str()).collect();
        assert_eq!(codes, vec!["check.not_mut"], "the assignment to a non-mut local");

        let items = guest
            .call(Some(&iface), "completion", &[request.clone(), serde_json::json!(src.find("a = 2").unwrap() + 1)])
            .expect("completion should answer");
        let labels: Vec<&str> = items.as_array().expect("a list").iter().filter_map(|i| i.get("label")?.as_str()).collect();
        assert!(labels.contains(&"a"), "the local is proposed: {labels:?}");

        let err = guest
            .call(Some(&iface), "hover", &[request, serde_json::json!(10_000)])
            .expect_err("an offset past the end is refused");
        println!("refusal reads: {err}");

        // What `persistent = true` buys, measured on a real mod when one is given
        // (`ARBOR_LANGUAGE_WORKSPACE` = a folder of `.ptn`): a fresh instance per call rebuilds the
        // whole workspace; a live one keeps its session and re-reads the one file that changed.
        if let Ok(dir) = std::env::var("ARBOR_LANGUAGE_WORKSPACE") {
            let mut files: Vec<(String, String)> = std::fs::read_dir(&dir)
                .expect("a folder")
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "ptn"))
                .map(|p| (p.to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
                .collect();
            files.sort();
            let (path, text) = files[0].clone();
            let req = |text: &str| serde_json::json!({
                "workspace": { "dir": dir, "manifest": null,
                    "files": files.iter().map(|(p, t)| serde_json::json!({ "path": p, "text": t })).collect::<Vec<_>>() },
                "path": path, "text": text, "lang": "",
            });
            let at = text.len() / 2;
            let at = (0..=at).rev().find(|&i| text.is_char_boundary(i)).unwrap();
            let median = |mut v: Vec<f64>| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] };

            let mut cold = vec![];
            for _ in 0..7 {
                let t = std::time::Instant::now();
                let mut fresh = host.open_dynamic(&module, GuestCaps::new("fixture", vec![], vec![]), Arc::new(NoServices)).unwrap();
                fresh.call(Some(&iface), "completion", &[req(&text), serde_json::json!(at)]).unwrap();
                cold.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let mut warm = vec![];
            for i in 0..15 {
                // One keystroke's worth of change, like the editor sends.
                let edited = format!("{text}// {i}\n");
                let t = std::time::Instant::now();
                guest.call(Some(&iface), "completion", &[req(&edited), serde_json::json!(at)]).unwrap();
                warm.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("{} files: fresh instance per call {:.1} ms, live instance {:.1} ms (medians)", files.len(), median(cold), median(warm));
        }
    }
}


/// A real guest reads files through the host's `fs` interface, end to end.
///
/// The Patina package declares `fs = "read"` and reads the open mod's UI documents to check `ui!(…)`; this drives it
/// the way Arbor does — one live guest, the active repository set before each call — and proves the four things the
/// unit tests of the gate cannot: the import links, the read happens inside the repository, a change on disk reaches
/// the **same instance**, and a refusal degrades to silence instead of an error.
///
/// ```sh
/// ARBOR_PATINA_FIXTURE=…/patina_language_intel.wasm \
///   cargo test -p arbor-plugin-wasm --features runtime -- --ignored guest_reads_files --nocapture
/// ```
#[test]
#[ignore = "needs ARBOR_PATINA_FIXTURE — the built Patina package"]
fn a_guest_reads_files_through_the_fs_interface() {
    use arbor_plugin_types::prelude::AccessLevel;

    let Ok(module) = std::env::var("ARBOR_PATINA_FIXTURE") else {
        panic!("set ARBOR_PATINA_FIXTURE to the built patina_language_intel.wasm");
    };
    let module = std::path::PathBuf::from(module);

    let root = std::env::temp_dir().join(format!("arbor-guest-fs-{}", std::process::id()));
    let outside = std::env::temp_dir().join(format!("arbor-guest-fs-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(root.join("presentazione/hud")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let doc = |id: &str| format!("UiDoc(name: \"Hud\", root: Node(name: \"Kills\", id: \"{id}\", kind: Container))\n");
    std::fs::write(root.join("presentazione/hud/hud.ron"), doc("kills")).unwrap();

    let host = WasmHost::new().expect("engine");
    let iface = "arbor:extensions/language-intel@1.0.0";
    let dir = root.to_string_lossy().replace('\\', "/");
    let request = |text: &str| {
        serde_json::json!({
            "workspace": { "dir": dir, "manifest": "(id: \"modx\")", "files": [] },
            "path": format!("{dir}/script/mine.ptn"),
            "text": text,
            "lang": "",
        })
    };
    let codes = |guest: &mut arbor_plugin_wasm::prelude::DynGuest, text: &str| -> Vec<String> {
        let out = guest.call(Some(iface), "diagnostics", &[request(text)]).expect("diagnostics");
        out.as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect()
    };
    let known = "use voriz::prelude::*\nconst A: UiNode = ui!(\"modx/hud\", \"kills\")\n";
    let nonsense = "use voriz::prelude::*\nconst A: UiNode = ui!(\"no\", \"such\")\n";

    let open = |level: AccessLevel, repo: &std::path::Path| {
        let caps = GuestCaps::new("patina", vec![], vec![])
            .with_fs(level, vec![])
            .with_repo(Some(repo.to_path_buf()));
        host.open_dynamic(&module, caps, Arc::new(NoServices) as Services)
            .unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()))
    };

    // Permitted, repository = the mod: the node the mod defines is known, nonsense is reported.
    let mut g = open(AccessLevel::Read, &root);
    assert!(codes(&mut g, known).is_empty(), "the mod's own document was read");
    assert!(codes(&mut g, nonsense).contains(&"index.unknown_catalog_scope".to_string()));

    // The file changes on disk; the same live instance sees it on the next call.
    std::fs::write(root.join("presentazione/hud/hud.ron"), doc("casualties")).unwrap();
    assert_eq!(codes(&mut g, known), vec!["index.unknown_catalog_id".to_string()]);

    // The user switches repository: the mod is no longer inside the sandbox, so the read is refused and the package
    // degrades to no data — silence, not an error.
    g.set_active_repo(Some(outside.clone()));
    assert!(codes(&mut g, nonsense).is_empty(), "a refused read must degrade quietly");
    g.set_active_repo(Some(root.clone()));
    assert!(!codes(&mut g, nonsense).is_empty(), "and come back when the repository does");

    // No `fs` permission at all: the same.
    let mut none = open(AccessLevel::None, &root);
    assert!(codes(&mut none, nonsense).is_empty());

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&outside);
}

/// A real guest builds a mod end to end, and an export it lacks answers with the sentence Bennu recognises.
///
/// The Patina package exports `build` (optional in `language-intel`): it takes the workspace and a language, finds the
/// mod's dependencies by id through the `fs` interface, compiles with the real compiler and returns a flat report. Asked for
/// a function that does not exist — what an older package answers to `build` — the host's error names the function in the
/// words `bennu-be` turns into "this backend has no build".
///
/// ```sh
/// ARBOR_PATINA_FIXTURE=…/patina_language_intel.wasm \
///   cargo test -p arbor-plugin-wasm --features runtime -- --ignored guest_builds --nocapture
/// ```
#[test]
#[ignore = "needs ARBOR_PATINA_FIXTURE — the built Patina package"]
fn a_guest_builds_a_mod_and_a_missing_export_says_so() {
    use arbor_plugin_types::prelude::AccessLevel;

    let Ok(module) = std::env::var("ARBOR_PATINA_FIXTURE") else {
        panic!("set ARBOR_PATINA_FIXTURE to the built patina_language_intel.wasm");
    };
    let module = std::path::PathBuf::from(module);

    let root = std::env::temp_dir().join(format!("arbor-guest-build-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("m/script")).unwrap();
    std::fs::create_dir_all(root.join("marmo/script")).unwrap();
    std::fs::write(root.join("marmo/manifest.ron"), "(id: \"marmo\")").unwrap();
    std::fs::write(root.join("marmo/script/lib.ptn"), "pub struct Gem {\n    pub n: int,\n}\n").unwrap();
    let manifest = "(id: \"m\", dependencies: [(id: \"marmo\")])";
    std::fs::write(root.join("m/manifest.ron"), manifest).unwrap();
    let dir = root.join("m").to_string_lossy().replace('\\', "/");
    let main = "use marmo::Gem\nfn value(g: Gem) -> int { g.n }\nfn bad(x: fx) -> fx { x.sin() }\n";
    let workspace = |text: &str| {
        serde_json::json!({
            "dir": dir,
            "manifest": manifest,
            "files": [{ "path": format!("{dir}/script/main.ptn"), "text": text }],
        })
    };

    let host = WasmHost::new().expect("engine");
    let iface = "arbor:extensions/language-intel@1.0.0";
    // The repository is the folder holding both mods: the sibling `marmo` is inside it, so the dependency can be read.
    let caps = GuestCaps::new("patina", vec![], vec![]).with_fs(AccessLevel::Read, vec![]).with_repo(Some(root.clone()));
    let mut g = host
        .open_dynamic(&module, caps, Arc::new(NoServices) as Services)
        .unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()));

    let out = g.call(Some(iface), "build", &[workspace(main), serde_json::json!("")]).expect("build");
    println!("{out}");
    assert_eq!(out["ok"], false, "the fx::sin line is a construct the compiler does not lower yet");
    assert_eq!(out["mod-id"], "m");
    assert_eq!(out["files"], 1);
    assert!(out["dependencies"][0].as_str().unwrap().starts_with("marmo @ "), "the dependency was found by id: {out}");
    let d = out["diagnostics"].as_array().unwrap().iter().find(|d| d["code"] == "compile.unsupported").expect("the compiler's own error");
    assert_eq!(d["origin"], "compiler");
    assert!(d["span"]["start"].as_u64().is_some(), "an option<span> that is set reads back as the record: {d}");

    let good = g.call(Some(iface), "build", &[workspace("use marmo::Gem\nfn value(g: Gem) -> int { g.n }\n"), serde_json::json!("")]).expect("build");
    assert_eq!(good["ok"], true, "{good}");
    assert!(good["functions"].as_u64().unwrap() >= 1);
    assert!(good["diagnostics"].as_array().unwrap().is_empty());

    // An older package has no `build`: the words the host uses are the ones `bennu-be` looks for.
    let err = g.call(Some(iface), "no_such_function", &[]).unwrap_err();
    assert!(err.contains("has no function 'no_such_function'"), "{err}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Go to definition and the name search, through the real package: what the editor's go-to and Cmd+N get back for a name in the same
/// file, in another file of the mod, in a dependency, in the built-in library and in a catalog.
///
/// ```sh
/// ARBOR_PATINA_FIXTURE=…/patina_language_intel.wasm \
///   cargo test -p arbor-plugin-wasm --features runtime -- --ignored guest_answers_definitions --nocapture
/// ```
#[test]
#[ignore = "needs ARBOR_PATINA_FIXTURE — the built Patina package"]
fn a_guest_answers_definitions_and_name_searches() {
    use arbor_plugin_types::prelude::AccessLevel;

    let Ok(module) = std::env::var("ARBOR_PATINA_FIXTURE") else {
        panic!("set ARBOR_PATINA_FIXTURE to the built patina_language_intel.wasm");
    };
    let module = std::path::PathBuf::from(module);
    let root = std::env::temp_dir().join(format!("arbor-guest-defs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("m/script")).unwrap();
    std::fs::create_dir_all(root.join("marmo/script")).unwrap();
    std::fs::write(root.join("marmo/manifest.ron"), "(id: \"marmo\")").unwrap();
    std::fs::write(root.join("marmo/script/lib.ptn"), "pub struct Gem {\n    pub n: int,\n}\n").unwrap();
    let manifest = "(id: \"m\", dependencies: [(id: \"marmo\")])";
    std::fs::write(root.join("m/manifest.ron"), manifest).unwrap();
    let dir = root.join("m").to_string_lossy().replace('\\', "/");
    let other = "pub struct Bag { pub n: int }\npub enum Kind { Rock, Ore(int) }\nimpl Bag {\n    pub fn weight(self) -> int { self.n }\n}\npub fn helper() -> int { 1 }\n";
    let main = "use voriz::prelude::*\nuse marmo::Gem\nuse crate::other::Bag\nuse crate::other::Kind\nuse crate::other::helper\n\nfn local() -> int { 2 }\n\n#[server]\n#[on(PlayerJoined)]\nfn greet(ev: PlayerJoined, g: Bag, gem: Gem) {\n    let a = local()\n    let b = helper()\n    let k = Kind::Ore(1)\n    let w = g.weight()\n    let f = g.n\n    let m = gem.n\n    let n = ev.player.name()\n}\n";
    std::fs::write(root.join("m/script/other.ptn"), other).unwrap();
    std::fs::write(root.join("m/script/main.ptn"), main).unwrap();
    let main_path = format!("{dir}/script/main.ptn");
    let other_path = format!("{dir}/script/other.ptn");
    let req = serde_json::json!({
        "workspace": { "dir": dir, "manifest": manifest, "files": [
            { "path": other_path, "text": other }, { "path": main_path, "text": main },
        ] },
        "path": main_path, "text": main, "lang": "",
    });

    let host = WasmHost::new().expect("engine");
    let iface = "arbor:extensions/language-intel@1.0.0";
    let caps = GuestCaps::new("patina", vec![], vec![]).with_fs(AccessLevel::Read, vec![]).with_repo(Some(root.clone()));
    let mut g = host.open_dynamic(&module, caps, Arc::new(NoServices) as Services).unwrap_or_else(|e| panic!("{} did not come up: {e}", module.display()));

    // (file, text of that file) → what `definition` says about the `nth` byte of `needle` in `main`.
    let mut def = |needle: &str, shift: usize| -> (String, String) {
        let at = main.find(needle).unwrap_or_else(|| panic!("`{needle}` is not in the source")) + shift;
        let out = g.call(Some(iface), "definition", &[req.clone(), serde_json::json!(at)]).expect("definition");
        let first = out.as_array().and_then(|a| a.first()).unwrap_or_else(|| panic!("no definition for `{needle}`: {out}"));
        let file = first["file"].as_str().unwrap().to_string();
        let (s, e) = (first["span"]["start"].as_u64().unwrap() as usize, first["span"]["end"].as_u64().unwrap() as usize);
        let text = if file.starts_with("patina-lib://") {
            g.call(Some(iface), "library-source", &[serde_json::json!(file)]).expect("library-source").as_str().expect("the package carries that file").to_string()
        } else {
            std::fs::read_to_string(&file).unwrap()
        };
        (file, text[s..e].to_string())
    };
    // The same file.
    assert_eq!(def("local()", 1), (main_path.clone(), "local".into()));
    // Another file of the mod: a function, a type, a variant, a method, a field.
    assert_eq!(def("helper()\n    let k", 1), (other_path.clone(), "helper".into()));
    assert_eq!(def("g: Bag", 4), (other_path.clone(), "Bag".into()));
    assert_eq!(def("Kind::Ore", 7), (other_path.clone(), "Ore".into()));
    assert_eq!(def("g.weight", 3), (other_path.clone(), "weight".into()));
    assert_eq!(def("g.n\n", 2), (other_path.clone(), "n".into()));
    // A dependency: a real path, outside this mod's folder.
    let (file, name) = def("gem: Gem", 6);
    assert!(file.ends_with("/marmo/script/lib.ptn") && !file.starts_with("patina-lib://") && name == "Gem", "{file} {name}");
    // The library the package carries: a made-up path whose text `library-source` gives.
    let (file, name) = def("name()", 1);
    assert!(file.starts_with("patina-lib://voriz/") && name == "name", "{file} {name}");

    // The name search: the mod's types first, then the dependency's, then the library's.
    let mut search = |q: &str| -> Vec<serde_json::Value> {
        g.call(Some(iface), "workspace-symbols", &[req.clone(), serde_json::json!(q)]).expect("workspace-symbols").as_array().unwrap().clone()
    };
    let bag = search("Bag");
    assert_eq!((bag[0]["name"].as_str(), bag[0]["kind"].as_str(), bag[0]["library"].as_bool()), (Some("Bag"), Some("struct"), Some(false)), "{bag:?}");
    assert_eq!(bag[0]["file"].as_str(), Some(other_path.as_str()));
    assert!(search("Gem").iter().any(|s| s["name"] == "Gem" && s["owner"] == "marmo" && s["library"] == false));
    assert!(search("Player").iter().any(|s| s["name"] == "Player" && s["library"] == true));
    assert!(search("BP").iter().any(|s| s["name"] == "BlockPos"), "camel-case initials");
    assert!(search("Ore").iter().any(|s| s["name"] == "Ore" && s["kind"] == "enum-member" && s["container"] == "Kind"));

    let _ = std::fs::remove_dir_all(&root);
}
