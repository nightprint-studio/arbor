//! Finding the shader modules a project can import — the half [`bennu_wgsl::library`] will
//! not do, because it is a question about a machine rather than about a shader.
//!
//! Two sources, and they answer different halves:
//!
//! * **the project's own `.wgsl`**, walked from its root. The user wrote them, so no
//!   catalogue can know them.
//! * **the `bevy_*` crates the project resolved**, read out of the cargo registry cache.
//!
//! ## Why the registry is not a guess
//!
//! It reads like one — "look under `~/.cargo` and hope" — and the previous version of this
//! feature declined to do it for exactly that reason. But nothing here is hoped at:
//! `Cargo.lock` names the package and the exact version the project resolved, and the
//! registry lays an extracted crate out at `$CARGO_HOME/registry/src/<index>/<name>-<version>`.
//! Both halves are facts on disk. If the directory is not there — the project has never been
//! built, or Bevy came from a git or path dependency — the index simply comes back without
//! those modules, and the curated `BEVY_IMPORTS` list still covers the common paths.
//!
//! Reading a dependency's sources to answer questions about the file in front of you is what
//! the Java side already does with jars; a registry checkout is the same move against a
//! directory instead of an archive.
//!
//! ## Why it is memoised, and why the memo never expires on its own
//!
//! Building the index means walking a few hundred files and scanning each one. That is fine
//! once and unthinkable per keystroke — completion runs while the user types. The result is
//! cached per project root and only ever dropped explicitly ([`forget`]), because the inputs
//! do not move under a session: a crate in the registry is immutable by construction, and a
//! change to the project's own shaders is a change to a file the editor already knows about.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bennu_wgsl::prelude::ShaderLibrary;

/// How many `.wgsl` files to read from the project itself.
///
/// The registry side is naturally bounded — a crate holds what it holds — but the project
/// side walks whatever the user opened, which can be a monorepo. Generous, because the walk
/// happens once per root and not per keystroke.
const MAX_PROJECT_SHADERS: usize = 4_000;

/// Directories that never hold an importable shader and always cost a lot to walk.
const SKIP_DIRS: &[&str] = &["target", ".git", "node_modules", ".arbor", "dist", "build"];

/// Crates whose shader sources are worth reading.
///
/// Prefix-matched against `Cargo.lock`, so a project on a fork or a renamed crate still
/// gets picked up as long as it is called `bevy…`. Nothing else in a normal dependency tree
/// ships `.wgsl` in its `src/`, so the cost of being generous here is a directory walk that
/// finds nothing.
fn is_shader_crate(name: &str) -> bool {
    name.starts_with("bevy")
}

/// The nearest ancestor that looks like a project root.
fn project_root(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .find(|p| p.join("Cargo.toml").is_file() || p.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The nearest `Cargo.lock` at or above `root` — a workspace member's lock lives at the
/// workspace root, which is often several levels up from the crate whose shader is open.
fn find_lock(root: &Path) -> Option<PathBuf> {
    root.ancestors().map(|p| p.join("Cargo.lock")).find(|p| p.is_file())
}

/// `(name, version)` for every shader-bearing package in a `Cargo.lock`.
///
/// Parsed as TOML rather than by hand: the lock is generated, but it is still TOML, and a
/// line-wise reader would be one `[[package]]` layout change away from silently returning
/// nothing.
fn locked_shader_crates(lock: &str) -> Vec<(String, String)> {
    let Ok(doc) = lock.parse::<toml::Value>() else { return Vec::new() };
    let Some(packages) = doc.get("package").and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    packages
        .iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?;
            let version = p.get("version")?.as_str()?;
            is_shader_crate(name).then(|| (name.to_string(), version.to_string()))
        })
        .collect()
}

/// The extracted source directory of one locked crate, if it is unpacked on this machine.
///
/// `registry_dirs("src")` rather than a path built here: the index directory in the middle
/// is a hash of the registry URL that changes between cargo versions, and `bennu-cargo`
/// already owns that lookup for the dependency report. A second copy of it is a second place
/// to get the `CARGO_HOME` fallback wrong.
fn crate_src_dir(name: &str, version: &str) -> Option<PathBuf> {
    let wanted = format!("{name}-{version}");
    bennu_cargo::prelude::registry_dirs("src")
        .into_iter()
        .map(|index| index.join(&wanted))
        .find(|p| p.is_dir())
}

/// Every `.wgsl` under `dir`, as `(forward-slashed path, source)`.
fn collect_wgsl(dir: &Path, budget: &mut usize) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if *budget == 0 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
                continue;
            }
            if !name.ends_with(".wgsl") || *budget == 0 {
                continue;
            }
            if let Ok(source) = std::fs::read_to_string(&p) {
                *budget -= 1;
                out.push((p.to_string_lossy().replace('\\', "/"), source));
            }
        }
    }
    out
}

/// Build the index for one project root.
///
/// Order matters: the project's own modules go in first, so a shader in the repo that
/// declares a path Bevy also declares wins. That is the right way round — somebody who has
/// shadowed a Bevy module has done it on purpose, and the editor should describe the file
/// they will actually get.
fn build(root: &Path) -> ShaderLibrary {
    let mut budget = MAX_PROJECT_SHADERS;
    let mut sources = collect_wgsl(root, &mut budget);

    let crates = find_lock(root)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|lock| locked_shader_crates(&lock))
        .unwrap_or_default();
    for (name, version) in crates {
        let Some(dir) = crate_src_dir(&name, &version) else { continue };
        // No budget on a crate: it holds what it holds, and truncating a dependency's
        // modules would drop them silently — the failure mode where completion is missing
        // one entry and nothing says why.
        let mut unbounded = usize::MAX;
        sources.extend(collect_wgsl(&dir, &mut unbounded));
    }

    ShaderLibrary::index(sources)
}

type Cache = Mutex<HashMap<PathBuf, Arc<ShaderLibrary>>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The index for whatever project `file` belongs to, built on first ask.
///
/// Returns an empty library rather than `None` when there is no project: every caller wants
/// to ask it a question and get nothing back, not to branch on whether it exists.
pub(crate) fn for_file(file: &str) -> Arc<ShaderLibrary> {
    let path = Path::new(file);
    // Absolute only. A relative path has no location to walk up from, and resolving one
    // against the process's working directory would index whatever happens to be there —
    // in a unit test, the crate being tested.
    if !path.is_absolute() {
        return Arc::new(ShaderLibrary::default());
    }
    let Some(root) = project_root(path) else {
        return Arc::new(ShaderLibrary::default());
    };
    if let Ok(guard) = cache().lock() {
        if let Some(hit) = guard.get(&root) {
            return Arc::clone(hit);
        }
    }
    // Built outside the lock: this walks a few hundred files, and holding the mutex across
    // it would stall every other shader in the editor behind the first one to ask. Two
    // callers racing here both build, and the second one's result is dropped — cheaper than
    // the contention, and the input is immutable so the two agree.
    let built = Arc::new(build(&root));
    if let Ok(mut guard) = cache().lock() {
        return Arc::clone(guard.entry(root).or_insert(built));
    }
    built
}

/// Drop the cached index for a project, so the next ask rebuilds it.
///
/// For the one case the memo's assumption does not cover: the user added a
/// `#define_import_path` to a shader that had none, or pulled a dependency in mid-session.
#[allow(dead_code)]
pub(crate) fn forget(root: &Path) {
    if let Ok(mut guard) = cache().lock() {
        guard.remove(root);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_yields_bevy_crates_and_nothing_else() {
        let lock = concat!(
            "version = 3\n\n",
            "[[package]]\nname = \"bevy_pbr\"\nversion = \"0.18.1\"\n",
            "source = \"registry+https://github.com/rust-lang/crates.io-index\"\n\n",
            "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n\n",
            "[[package]]\nname = \"bevy_render\"\nversion = \"0.18.1\"\n",
        );
        let found = locked_shader_crates(lock);
        assert_eq!(
            found,
            vec![
                ("bevy_pbr".to_string(), "0.18.1".to_string()),
                ("bevy_render".to_string(), "0.18.1".to_string()),
            ]
        );
    }

    #[test]
    fn a_lock_that_does_not_parse_yields_nothing_rather_than_panicking() {
        // A half-written lock is a thing that exists during a `cargo add`.
        assert!(locked_shader_crates("[[package]\nname = ").is_empty());
        assert!(locked_shader_crates("").is_empty());
    }

    #[test]
    fn a_project_without_bevy_asks_the_registry_for_nothing() {
        let lock = "[[package]]\nname = \"serde\"\nversion = \"1.0.0\"\n";
        assert!(locked_shader_crates(lock).is_empty());
    }

    #[test]
    fn shader_crates_are_recognised_by_prefix() {
        assert!(is_shader_crate("bevy_pbr"));
        assert!(is_shader_crate("bevy_render"));
        assert!(is_shader_crate("bevy"));
        assert!(!is_shader_crate("serde"));
        assert!(!is_shader_crate("not_bevy_pbr"));
    }

    #[test]
    fn a_file_outside_any_project_gets_an_empty_library_not_a_panic() {
        let lib = for_file("/nowhere/at/all/x.wgsl");
        assert!(lib.is_empty());
    }

    #[test]
    fn a_relative_path_indexes_nothing() {
        // Otherwise the walk starts at the process's working directory, which under `cargo
        // test` is this crate — every test that passes a bare filename would index Bennu.
        assert!(for_file("s.wgsl").is_empty());
        assert!(for_file("shaders/x.wgsl").is_empty());
    }
}
