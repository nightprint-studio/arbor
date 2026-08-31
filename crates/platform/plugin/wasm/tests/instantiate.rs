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
}

