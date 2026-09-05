//! Dependency-classpath sourcing for the validation / completion index.
//!
//! Resolves a Maven project's `~/.m2` dependency jars and exposes them as a `ClassSource`, so the
//! resolver's dependency tier (`bennu_query`'s `ClasspathIndex`) can decode library types (Spring,
//! servlet, Hibernate, Struts, …) — not just the JDK + project sources.
//!
//! Two levels of caching keep this cheap:
//!   * the resolved **jar LIST** is persisted to disk keyed by the pom's mtime, so
//!     `mvn dependency:build-classpath` (seconds) runs at most once per pom across sessions;
//!   * the decoded **members** of each dep class are memoized (lazily, on first touch) to a
//!     per-project file by `JdkMemberIndex::persistent` — keyed by the resolved jar set, so a
//!     changed dependency set starts a fresh memo and never serves a stale decode.
//!
//! Non-fatal by construction: a project with no `pom.xml`, no resolvable dep jars, or a failed Maven
//! resolve leaves the resolver on JDK + project exactly as before. But "non-fatal" is not the same as
//! "fine": for a *Maven* project a missing dependency tier means every library type reads as "cannot
//! resolve", so [`DepOutcome`] separates "doesn't apply" from "failed, and here's why" and the caller
//! tells the user about the second.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bennu_classpath::prelude::{
    find_jdk_home, resolve_maven_classpath, source_from_jars, ClassSource, MavenResolveOpts,
};

/// Read a **text** entry out of a dependency jar the way the JVM would.
///
/// `bennu-classpath` hands back bytes on purpose — it opens zips, it does not hold opinions about
/// encodings — and this is the one place that turns them into text, so every reader of a jar
/// entry gets the same answer.
///
/// The rule is UTF-8, falling back to Windows-1252, and it is neither a guess nor a new policy:
///
///   * it is what the JVM itself does for a `.properties` since Java 9 (`PropertyResourceBundle`
///     reads UTF-8 and falls back to the single-byte encoding), which matters because a
///     `.properties` written before that is **ISO-8859-1 by specification** and is exactly the
///     entry most likely to carry an accent;
///   * Windows-1252 is a superset of ISO-8859-1 over every printable character, so it recovers a
///     Latin-1 file *and* the typographic quotes a file written on a Windows box actually
///     contains, where plain Latin-1 would give C1 control codes;
///   * it is byte-for-byte the recovery `bennu-project` already applies to a source file that is
///     not valid UTF-8, so a jar entry and a `.java` in a legacy tree are read by one rule rather
///     than by two that drift.
///
/// Never lossy — which was the point. Windows-1252 decodes every possible byte, so nothing
/// becomes `U+FFFD` and an accent in a library's error message arrives as the accent it is.
///
/// Not covered: an entry in an encoding neither of those recovers — a Shift_JIS descriptor, an
/// XML prolog declaring something exotic. Honouring a declared encoding would mean parsing the
/// prolog and is worth doing the day such a jar turns up; today it would be machinery for a case
/// nobody has hit.
pub fn jar_entry_text(bytes: &[u8]) -> String {
    bennu_project::prelude::decode_for_index(bytes, bennu_project::prelude::UTF8).text
}

/// The dependency tier for a project: an opened dep-jars source + the per-project memo path its
/// decoded members persist to. Handed to `NativeJavaProvider::for_project`.
pub struct DepClasspath {
    /// The dependency jars behind one `ClassSource` (JDK-free — the JDK is a separate tier).
    pub source: Box<dyn ClassSource>,
    /// The per-project, per-jar-set memo file the decoded dep members persist to.
    pub memo_path: PathBuf,
    /// The resolved dep jar paths (absolute) — surfaced to the index inspector's Jars list, so
    /// the count reflects exactly what the resolver loaded (not the Build's `target/` artifact).
    pub jars: Vec<String>,
    /// Set when Maven did NOT resolve everything: the reason, for the user.
    ///
    /// A partial tier is worse than no tier for one reason — it looks like a working one. Every
    /// type from a jar that is missing reads as "cannot resolve", and on a real project that was
    /// **3308 errors on a tree the compiler builds without a warning**. The list is also not
    /// cached in that state (see [`resolve_dep_classpath`]), so a later `mvn install` is picked up
    /// on the next open instead of being shadowed until a pom changes.
    pub partial: Option<String>,
}

/// What resolving the dependency tier produced. The three cases are genuinely different to the user,
/// which a bare `Option` conflated: a Cargo or plain-source project simply has no Maven tier, whereas a
/// `pom.xml` project that ends up with zero dependency jars is *broken* — every library type in it
/// will read as "cannot resolve" — and the reason has to reach the user, not just stderr.
pub enum DepOutcome {
    /// No `pom.xml` — the Maven dependency tier doesn't apply. Silent, and not a problem.
    NotApplicable,
    /// The tier is ready.
    Resolved(DepClasspath),
    /// A Maven project whose dependencies could NOT be resolved; the string is a user-facing reason.
    Failed(String),
}

/// Resolve the project's dependency jars and build the dependency tier. See [`DepOutcome`] — the
/// caller builds a JDK-only provider for anything other than [`DepOutcome::Resolved`], and surfaces
/// the reason when it's a failure.
///
/// ## Three resolvers, in the order that costs least
///
/// 1. **The cached jar list**, when it is still true. See [`load_entry`]: the cache records what was
///    *missing* as well as what was found, so a partial resolve can be cached safely and is
///    invalidated the moment one of the absent artifacts arrives.
/// 2. **The poms and `~/.m2`, read directly** (`bennu-maven`). Milliseconds, no JVM, no network, and
///    it names the coordinates it could not find. When it resolves everything, that is the answer —
///    running Maven to confirm it would cost seconds per project open to learn nothing.
/// 3. **`mvn dependency:build-classpath`**, when the direct read came up short. Maven is the ground
///    truth about a build; it is asked precisely when the cheap answer admits it is incomplete.
///
/// The two are **unioned** rather than one replacing the other: a Maven run that fails halfway
/// still wrote the entries it resolved, and the direct read may have found artifacts the failing
/// reactor never got to. Whatever is left missing after both is what the user is told about, by
/// coordinate — which is the whole difference between "0 jars resolved" and
/// `com.acme:legacy-core:2.4.0 is not in your local repository`.
pub fn resolve_dep_classpath(root: &Path, jdk_version: &str) -> DepOutcome {
    if !root.join("pom.xml").is_file() {
        return DepOutcome::NotApplicable;
    }
    let Some(pom_mtime) = poms_mtime(root) else {
        return DepOutcome::Failed("the project's pom.xml could not be read".to_string());
    };

    let (jars, partial) = match load_entry(root, pom_mtime) {
        Some(entry) => (entry.jars, entry.partial),
        None => match resolve_fresh(root, jdk_version) {
            Ok(fresh) => {
                if fresh.cacheable {
                    save_entry(root, pom_mtime, &fresh);
                }
                (fresh.jars, fresh.partial)
            }
            Err(reason) => return DepOutcome::Failed(reason),
        },
    };
    if jars.is_empty() {
        return DepOutcome::Failed("no dependency jars resolved".to_string());
    }

    let paths: Vec<PathBuf> = jars.iter().map(PathBuf::from).collect();
    let source: Box<dyn ClassSource> = Box::new(source_from_jars(&paths));
    let memo_path = memo_path_for(root, &jars);
    DepOutcome::Resolved(DepClasspath { source, memo_path, jars, partial })
}

/// One resolve's result, as it is cached and as it is handed back.
#[derive(Default)]
pub(crate) struct ResolvedList {
    pub jars: Vec<String>,
    /// Set when something is still missing — the sentence the user sees.
    pub partial: Option<String>,
    /// Where each missing artifact was looked for, so the next open can ask "has it arrived yet"
    /// with one `stat` each instead of re-running Maven.
    pub missing_paths: Vec<String>,
    /// Which resolver answered: `offline`, `maven`, or `union`. Reported, not acted on — but a
    /// classpath that came from the direct read is worth being able to say out loud when something
    /// looks wrong with it.
    pub source: &'static str,
    /// Whether this is worth writing to the disk cache.
    ///
    /// `false` for a shortfall reported by a resolve whose **Maven leg failed**, and the reason is
    /// that such an entry can never expire. The cache re-runs when a recorded missing artifact
    /// arrives — but nothing is going to fetch it, precisely because the run that would have failed.
    /// So the half-resolve gets served, and re-served, and the same warning is shown on every open
    /// of a project that a single working run would have fixed. A failure is not a result: it is
    /// re-tried next time, which costs one spawn on a machine with no Maven and heals a transient
    /// one for free.
    pub cacheable: bool,
}

/// Resolve from scratch: the direct read first, Maven only if it is not enough.
fn resolve_fresh(root: &Path, jdk_version: &str) -> Result<ResolvedList, String> {
    let repo = bennu_maven::prelude::LocalRepo::discover();
    let offline = bennu_maven::prelude::resolve_offline(root, &repo);
    let offline_jars = offline.jar_strings();

    if offline.is_complete() && !offline_jars.is_empty() {
        eprintln!(
            "bennu-be: dependency classpath read straight from {} for {} ({} jars, no Maven run)",
            repo.root().display(),
            root.display(),
            offline_jars.len()
        );
        return Ok(ResolvedList {
            jars: offline_jars,
            source: "offline",
            cacheable: true,
            ..ResolvedList::default()
        });
    }

    // Not everything is there. Maven knows things this cannot — an active profile, a mirror, a
    // packaging plugin that rewrites a coordinate — so it gets the second word.
    let maven = resolve_via_maven(root, jdk_version);
    let mut jars = offline_jars;
    let mut source = "offline";
    if let Ok((maven_jars, _)) = &maven {
        source = if jars.is_empty() { "maven" } else { "union" };
        for jar in maven_jars {
            if !jars.contains(jar) {
                jars.push(jar.clone());
            }
        }
    }

    // What is *still* missing, re-checked after Maven ran rather than taken from the direct read:
    // Maven installs what it downloads into the same local repository, so an artifact it fetched
    // from a mirror is now sitting exactly where the direct read looked for it and is no longer
    // missing at all. Reporting the pre-Maven list would warn about the artifacts Maven just fixed.
    let (missing_paths, missing_coords) = still_missing(&offline, &repo);

    if jars.is_empty() {
        // Nothing from either resolver. Maven's own words when it ran, ours when it could not.
        return Err(match maven {
            Err(reason) => reason,
            Ok(_) => shortfall_message(&missing_coords, &offline)
                .unwrap_or_else(|| "no dependency jars resolved".to_string()),
        });
    }

    // When Maven failed, that is the FIRST thing to say, shortfall or no shortfall. It used to be
    // said only when nothing else was wrong — so the commonest case, "Maven did not run AND some
    // jars are absent", reported the absent jars and stayed silent about the one thing that would
    // have fetched them. The list then looks like the whole problem while the cause is missing from
    // it, and the natural conclusion is that the download simply does not work.
    let partial = match (&maven, shortfall_message(&missing_coords, &offline)) {
        (Err(reason), Some(short)) => Some(format!(
            "{short} Maven did not run to fetch them ({reason}), so the classpath is what the local \
             repository already held."
        )),
        (Err(reason), None) => Some(format!(
            "Maven could not be run ({reason}), so the classpath was read straight from the local \
             repository. It may be missing artifacts only a build would resolve."
        )),
        (Ok(_), short) => short,
    };
    if let Some(reason) = &partial {
        eprintln!("bennu-be: partial dependency classpath for {}: {reason}", root.display());
    }
    // A resolve whose Maven leg failed is not a result, it is an interruption — see `cacheable`.
    let cacheable = maven.is_ok() || partial.is_none();
    Ok(ResolvedList { jars, partial, missing_paths, source, cacheable })
}

/// Whether the dependency resolve may reach the network — `BennuConfig::maven_auto_download`.
///
/// Read at each resolve rather than cached: a resolve is rare and slow, and the one thing worse
/// than asking the config again is a switch that only takes effect after a restart.
fn auto_download() -> bool {
    bennu_core::prelude::load_config().maven_auto_download
}

/// The artifacts that are still absent from the repository, as `(path, coordinate)` — the direct
/// read's list, minus whatever has arrived since (see [`resolve_fresh`]).
fn still_missing(
    offline: &bennu_maven::prelude::Resolution,
    repo: &bennu_maven::prelude::LocalRepo,
) -> (Vec<String>, Vec<String>) {
    let mut paths = Vec::new();
    let mut coords = Vec::new();
    // Parallel by construction: both are derived from `Resolution::missing`, in its order.
    for (path, coord) in offline.missing_paths(repo).into_iter().zip(offline.missing.iter()) {
        if Path::new(&path).is_file() {
            continue;
        }
        paths.push(path);
        coords.push(coord.gav());
    }
    (paths, coords)
}

/// The user-facing sentence for what could not be resolved, or `None` when nothing is left.
fn shortfall_message(
    missing: &[String],
    offline: &bennu_maven::prelude::Resolution,
) -> Option<String> {
    /// Enough to recognise the problem; the full list is in the Dependencies panel.
    const SHOW: usize = 3;
    if missing.is_empty() && offline.unversioned.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !missing.is_empty() {
        // "not in the local repository" is true of the JAR and false of the coordinate: Maven
        // downloads a POM to walk the dependency graph and the jar only when something compiles
        // against it, so the folder is usually right there with just the pom in it. Saying "jar"
        // is what stops the next twenty minutes being spent proving the folder exists.
        parts.push(format!("{} whose jar is not in the local repository ({})", missing.len(), sample(missing, SHOW)));
    }
    if !offline.unversioned.is_empty() {
        let names: Vec<String> = offline.unversioned.iter().map(|c| c.gav()).collect();
        parts.push(format!(
            "{} with no resolvable version ({})",
            names.len(),
            sample(&names, SHOW)
        ));
    }
    Some(format!(
        "{} of this project's dependencies could not be resolved: {}. Types from them will read as \
         unresolved until they are.{}",
        missing.len() + offline.unversioned.len(),
        parts.join("; "),
        if auto_download() {
            ""
        } else {
            " Automatic download is off, so nothing was fetched."
        }
    ))
}

/// The first few of a list, then a count — a message, not a dump.
fn sample(items: &[String], show: usize) -> String {
    let head = items.iter().take(show).cloned().collect::<Vec<_>>().join(", ");
    if items.len() > show {
        format!("{head}, +{} more", items.len() - show)
    } else {
        head
    }
}

/// The dependency jars **already resolved** for `root`, without running Maven.
///
/// For consumers that want to read something out of the jars but have no business triggering a
/// resolve to get it — the framework-extension descriptors are the case this exists for. The
/// index service resolves the classpath as part of its own work; this reads whatever that left
/// behind and returns nothing when it has not run yet, which is the correct degradation (the
/// extension falls back to what it knows on its own).
pub(crate) fn cached_dep_jars(root: &Path) -> Vec<PathBuf> {
    poms_mtime(root)
        .and_then(|mtime| load_entry(root, mtime))
        .map(|entry| entry.jars)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

/// Run Maven's `dependency:build-classpath` (offline, pointed at the project's JDK) and return the
/// resolved jar paths as strings. `Err` carries a short user-facing reason — a "0 jars" state has to be
/// diagnosable from the UI, not only from the process's stderr.
fn resolve_via_maven(root: &Path, jdk_version: &str) -> Result<(Vec<String>, Option<String>), String> {
    let mut opts = MavenResolveOpts::default();
    // The one line that decides whether a missing jar is a warning or a download. The goal is the
    // same either way — only `-o` changes — so turning this off costs nothing but the network.
    opts.offline = !auto_download();
    // Resolve the REAL launcher: on Windows Maven ships `mvn.cmd`, and a bare `Command::new("mvn")`
    // only finds `mvn.exe` — so `"mvn"` silently fails to spawn (this is why deps showed 0 jars).
    opts.mvn_path = find_mvn_launcher(root);
    if let Some(jh) = find_jdk_home(jdk_version) {
        opts.java_home = Some(jh);
    }
    match resolve_maven_classpath(root, &opts) {
        Ok(cp) if !cp.jars.is_empty() => {
            // Maven wrote something, but did it write EVERYTHING? A non-zero exit or an entry that
            // is not on disk both mean no: the resolve runs offline, so an artifact never
            // downloaded is simply absent, and the goal reports the ones it could find anyway.
            let missing = cp.unresolved.len();
            let partial = (!cp.mvn_ok || missing > 0).then(|| {
                format!(
                    "Maven resolved {} of this project's dependency jars but could not resolve {missing} \
                     more{}. Types from those jars will read as unresolved until they are.",
                    cp.jars.len(),
                    if auto_download() {
                        " even with the download allowed — they may not exist at those coordinates"
                    } else {
                        ", and automatic download is off, so nothing was fetched"
                    }
                )
            });
            if let Some(reason) = &partial {
                eprintln!("bennu-be: partial dependency classpath for {}: {reason}", root.display());
            }
            Ok((cp.jars.iter().map(|p| p.display().to_string()).collect(), partial))
        }
        Ok(cp) => {
            eprintln!(
                "bennu-be: Maven resolved 0 dependency jars for {} ({} unresolved entries) — index \
                 runs JDK-only. Build the project once so its deps land in ~/.m2 (offline resolve).",
                root.display(),
                cp.unresolved.len()
            );
            Err(format!(
                "Maven resolved no dependency jars ({} entries missing from ~/.m2).{}",
                cp.unresolved.len(),
                if auto_download() {
                    " The download was allowed and still came up empty — check that the \
                     repositories the pom names can be reached."
                } else {
                    " Automatic download is off; turn it on in the settings, or build the project \
                     once so its dependencies land in ~/.m2."
                }
            ))
        }
        Err(e) => {
            eprintln!(
                "bennu-be: Maven dependency resolve failed for {} ({e}) — index runs JDK-only. \
                 Is Maven installed / on PATH? (launcher tried: {})",
                root.display(),
                opts.mvn_path
            );
            // NOT "Maven could not be run" any more: that is one of the reasons, and the
            // resolver already says so (`spawn mvn (…)`) when it is the one that happened.
            // Claiming it for every failure is what buried "your pom doesn't build" under a
            // sentence about the launcher — the one thing that was working.
            Err(format!("{e} (launcher: {})", opts.mvn_path))
        }
    }
}

/// The Maven launcher for `root`, as an absolute path where one can be found.
///
/// Four sources, in order:
///   1. **`PATH`** — preferring the Windows batch launchers (`mvn.cmd`/`mvn.bat`), because a bare
///      `Command::new("mvn")` only locates `mvn.exe` and a Maven install that ships only `mvn.cmd`
///      (the norm on Windows) would never spawn.
///   2. **Well-known install directories** ([`mvn_bin_dirs`]) — a desktop app launched from Finder /
///      the Dock / a desktop launcher inherits the system's minimal environment, *not* the user's
///      shell profile, so a Homebrew (`/opt/homebrew/bin`), MacPorts or SDKMAN Maven is invisible to
///      the `PATH` scan above even though `mvn` works fine in a terminal. That made the dependency
///      tier fail instantly, and the only trace was a line on stderr.
///   3. **The project's own Maven wrapper** (`mvnw`) — last, because it works even when Maven isn't
///      installed at all, but a cold wrapper *downloads* its distribution; an installed `mvn` is the
///      better answer whenever there is one.
///   4. The bare `"mvn"`, letting the child process resolve it.
pub(crate) fn find_mvn_launcher(root: &Path) -> String {
    let names: &[&str] =
        if cfg!(windows) { &["mvn.cmd", "mvn.bat", "mvn.exe", "mvn"] } else { &["mvn"] };
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Some(hit) = first_launcher(&dir, names) {
                return hit;
            }
        }
    }
    for dir in mvn_bin_dirs() {
        if let Some(hit) = first_launcher(&dir, names) {
            return hit;
        }
    }
    let wrapper = root.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
    if wrapper.is_file() {
        return wrapper.display().to_string();
    }
    "mvn".to_string()
}

/// The first of `names` that exists as a file directly in `dir`.
fn first_launcher(dir: &Path, names: &[&str]) -> Option<String> {
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file()).map(|p| p.display().to_string())
}

/// Directories that hold a `mvn` launcher on a typical developer machine, for when `PATH` doesn't
/// carry it (see [`find_mvn_launcher`]). A directory that doesn't exist costs one failed `is_file`.
fn mvn_bin_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    // An explicit Maven home wins over any guess.
    for var in ["MAVEN_HOME", "M2_HOME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                out.push(PathBuf::from(v).join("bin"));
            }
        }
    }
    out.push(PathBuf::from("/opt/homebrew/bin")); // Homebrew, Apple silicon
    out.push(PathBuf::from("/usr/local/bin")); // Homebrew on Intel, and manual installs
    out.push(PathBuf::from("/opt/local/bin")); // MacPorts
    out.push(PathBuf::from("/usr/share/maven/bin")); // Debian / Ubuntu package
    out.push(PathBuf::from("/opt/maven/bin"));
    if let Some(home) = bennu_classpath::prelude::user_home() {
        out.push(home.join(".sdkman/candidates/maven/current/bin"));
    }
    out
}

/// Drop the persisted jar-list cache for `root`, so the next [`resolve_dep_classpath`] re-runs Maven
/// instead of serving the recorded list.
///
/// Called by a **manual** index rebuild, which the user reaches for precisely when the dependency
/// tier looks wrong. Without this the rebuild could never recover from a bad list: the cache is keyed
/// on pom mtimes, so nothing the user could do short of editing a pom (or finding the cache directory)
/// would invalidate it.
pub(crate) fn clear_list_cache(root: &Path) {
    let _ = std::fs::remove_file(list_cache_path(root));
}

// ── on-disk jar-list cache (keyed by pom mtime) ─────────────────────────────────

/// Bumped when the resolver would now give a **different answer for the same poms**.
///
/// The cache's own freshness rules cannot express that. They ask whether the declarations changed
/// (pom mtime) and whether a recorded missing artifact has arrived — both about the project, neither
/// about us. So a fix to the resolver reaches nobody who already has an entry: the wrong list is
/// still "true" by every test the cache knows how to run, and on a project whose phantom artifacts
/// nothing will ever download it is true forever.
///
/// 2: the project's `<dependencyManagement>` now decides the version of transitive dependencies
/// (`bennu_maven::resolve::project_management`). Every list resolved before it can name artifacts
/// at versions the build does not use.
const RESOLVER_EPOCH: u64 = 2;

/// The freshness stamp for the whole project's poms: the **newest** `pom.xml` mtime under `root`.
///
/// Not just the root pom, and that is the fix: in a multi-module project the dependencies live in the
/// MODULE poms, so keying the cache on the root's mtime alone meant adding a dependency to a module
/// never invalidated anything — the stale jar list was served forever and the new library stayed
/// unresolvable until the root pom happened to be touched.
///
/// The max (rather than a hash of all of them) is enough: any edit to any pom moves it forward. Same
/// bounded walk the classpath collector uses, so a deep reactor is covered and a large repo isn't
/// crawled. `None` when no pom is readable at all.
fn poms_mtime(root: &Path) -> Option<u64> {
    /// Matches the classpath collector's depth — the same reactor shape.
    const MAX_DEPTH: usize = 6;
    let mut newest: Option<u64> = None;
    collect_pom_mtimes(root, MAX_DEPTH, &mut newest);
    newest
}

fn collect_pom_mtimes(dir: &Path, depth_left: usize, newest: &mut Option<u64>) {
    if let Some(secs) = std::fs::metadata(dir.join("pom.xml"))
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
    {
        if newest.is_none_or(|cur| secs > cur) {
            *newest = Some(secs);
        }
    }
    if depth_left == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "target" | ".git" | "node_modules" | ".idea" | "src") {
            continue;
        }
        collect_pom_mtimes(&entry.path(), depth_left - 1, newest);
    }
}

/// `bennu_data_dir()/dep-classpath/<root-hash>.json` — the persisted resolved jar list for `root`.
fn list_cache_path(root: &Path) -> PathBuf {
    arbor_core::prelude::bennu_data_dir()
        .join("dep-classpath")
        .join(format!("{}.json", fnv(root.to_string_lossy().as_bytes())))
}

/// A cached resolve, read back.
pub(crate) struct CachedList {
    pub jars: Vec<String>,
    /// The sentence the resolve produced, when it was a partial one — kept so a cached open says
    /// the same thing the resolve did instead of going quiet about a half classpath.
    pub partial: Option<String>,
}

/// The cached jar list for `root`, when it is still true.
///
/// Two conditions, and the second is the one that was missing. The pom mtime says the
/// *declarations* have not changed. The recorded missing paths say the *repository* has not: a pom
/// does not move when the artifact it names finally lands in `~/.m2`, so keying on the mtime alone
/// pinned a project to its half-resolved classpath until somebody edited a pom or found the cache
/// directory by hand. One `stat` per missing artifact answers it, and a resolve that was complete
/// records none — so the common case costs nothing.
fn load_entry(root: &Path, pom_mtime: u64) -> Option<CachedList> {
    load_entry_from(&list_cache_path(root), pom_mtime)
}

/// Persist a resolve for `root` with its pom mtime (best-effort — a write failure just means the
/// next session resolves again).
fn save_entry(root: &Path, pom_mtime: u64, list: &ResolvedList) {
    save_entry_to(&list_cache_path(root), pom_mtime, list);
}

/// The pure read of a cache FILE (path-injectable, so the gating is unit-testable without the
/// profile-scoped `bennu_data_dir`).
fn load_entry_from(path: &Path, pom_mtime: u64) -> Option<CachedList> {
    let bytes = std::fs::read(path).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    if v.get("resolver").and_then(|m| m.as_u64()) != Some(RESOLVER_EPOCH) {
        return None;
    }
    if v.get("pom_mtime").and_then(|m| m.as_u64()) != Some(pom_mtime) {
        return None;
    }
    // Anything recorded as missing that has since arrived invalidates the whole list: the resolve
    // that produced it would now find more.
    if let Some(missing) = v.get("missing_paths").and_then(|m| m.as_array()) {
        if missing.iter().filter_map(|p| p.as_str()).any(|p| Path::new(p).is_file()) {
            return None;
        }
    }
    let jars = v.get("jars")?.as_array()?;
    Some(CachedList {
        jars: jars.iter().filter_map(|j| j.as_str().map(str::to_string)).collect(),
        partial: v.get("partial").and_then(|p| p.as_str()).map(str::to_string),
    })
}

/// The pure write of a cache FILE (path-injectable, best-effort).
fn save_entry_to(path: &Path, pom_mtime: u64, list: &ResolvedList) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let value = serde_json::json!({
        "resolver": RESOLVER_EPOCH,
        "pom_mtime": pom_mtime,
        "jars": list.jars,
        "missing_paths": list.missing_paths,
        "partial": list.partial,
        "source": list.source,
    });
    if let Ok(bytes) = serde_json::to_vec(&value) {
        let _ = std::fs::write(path, bytes);
    }
}

/// `bennu_data_dir()/dep-index/<root-and-jarset-hash>.json` — the per-project decoded-members memo.
/// Keyed by the project root AND the (sorted) resolved jar set, so a changed dependency set starts a
/// fresh memo file rather than serving a stale decode of a since-removed jar.
fn memo_path_for(root: &Path, jars: &[String]) -> PathBuf {
    arbor_core::prelude::bennu_data_dir().join("dep-index").join(memo_file_name(root, jars))
}

/// The pure `<hash>.json` file name for a project's dependency memo — hashes the root plus the
/// SORTED jar set, so jar order doesn't matter but a changed set gives a fresh name.
fn memo_file_name(root: &Path, jars: &[String]) -> String {
    let mut hash = fnv_u64(root.to_string_lossy().as_bytes());
    let mut sorted: Vec<&String> = jars.iter().collect();
    sorted.sort();
    for j in sorted {
        hash = fnv_mix(hash, j.as_bytes());
    }
    format!("{hash:016x}.json")
}

// ── tiny FNV-1a hashing (filesystem-safe cache keys; mirrors index_service) ──────

fn fnv_u64(bytes: &[u8]) -> u64 {
    fnv_mix(0xcbf29ce484222325, bytes)
}

fn fnv_mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv(bytes: &[u8]) -> String {
    format!("{:016x}", fnv_u64(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A modern jar. Nothing to recover, and the fast path must not touch it.
    #[test]
    fn a_utf8_entry_is_read_as_utf8() {
        assert_eq!(jar_entry_text("citt\u{e0} = city".as_bytes()), "citt\u{e0} = city");
        assert_eq!(jar_entry_text(b"plain ascii"), "plain ascii");
    }

    /// The bug this rule exists for: a `.properties` inside a library is ISO-8859-1 by the
    /// `Properties.load` specification, so byte `0xE0` is an `a`-grave and not a broken UTF-8
    /// sequence. Reading it as lossy UTF-8 put a replacement character where the accent was.
    #[test]
    fn a_latin1_entry_keeps_its_accents_instead_of_losing_them() {
        // `citta` + U+00E0, as a single byte — invalid UTF-8, valid ISO-8859-1.
        assert_eq!(jar_entry_text(b"city=citt\xe0"), "city=citt\u{e0}");
        assert!(!jar_entry_text(b"city=citt\xe0").contains('\u{fffd}'), "never lossy");
    }

    /// Windows-1252 over ISO-8859-1 as the fallback: the 0x80-0x9F block is where a file written
    /// on a Windows box keeps its typographic characters, and plain Latin-1 would decode those to
    /// invisible C1 control codes.
    #[test]
    fn the_fallback_recovers_windows_typography_not_control_codes() {
        // 0x92 is a right single quotation mark in Cp1252; in ISO-8859-1 it is a control code.
        assert_eq!(jar_entry_text(b"it\x92s \x80"), "it\u{2019}s \u{20ac}");
    }

    /// Every byte sequence decodes to something. There is no input for which this loses data,
    /// which is the property that makes it safe to apply to entries of unknown provenance.
    #[test]
    fn no_byte_sequence_is_undecodable() {
        let every_byte: Vec<u8> = (0u8..=255).collect();
        assert!(!jar_entry_text(&every_byte).is_empty());
    }

    /// A project with no `pom.xml` has no Maven tier — and that must stay SILENT (a Cargo or plain
    /// source project isn't broken), which is the distinction `DepOutcome` exists to keep.
    #[test]
    fn no_pom_is_not_applicable_rather_than_a_failure() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-nopom-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(resolve_dep_classpath(&dir, "1.8"), DepOutcome::NotApplicable));
    }

    /// The Maven launcher must never come back empty: the bare `"mvn"` is the documented last resort,
    /// so a caller always has something to spawn (and a spawn error to report).
    #[test]
    fn mvn_launcher_always_yields_something() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-mvn-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(!find_mvn_launcher(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With no Maven anywhere on PATH or in a well-known directory, the project's own wrapper is used
    /// — the case of a machine that has never had Maven installed.
    #[test]
    fn mvn_wrapper_is_used_when_present() {
        // Only meaningful when the host has no `mvn` of its own; skip rather than assert a false thing.
        let bare = std::env::temp_dir().join(format!("bennu-deps-bare-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&bare);
        if find_mvn_launcher(&bare) != "mvn" {
            return; // this machine has a real Maven — the wrapper is correctly not preferred
        }
        let wrapper = bare.join(if cfg!(windows) { "mvnw.cmd" } else { "mvnw" });
        std::fs::write(&wrapper, "#!/bin/sh\n").unwrap();
        assert_eq!(find_mvn_launcher(&bare), wrapper.display().to_string());
        let _ = std::fs::remove_dir_all(&bare);
    }

    #[test]
    fn memo_name_changes_with_jar_set() {
        let root = Path::new("C:/proj");
        let a = memo_file_name(root, &["x.jar".to_string(), "y.jar".to_string()]);
        // Same jars, different order → SAME memo name (sorted before hashing).
        let a2 = memo_file_name(root, &["y.jar".to_string(), "x.jar".to_string()]);
        assert_eq!(a, a2);
        // A different jar set → a different memo name.
        let b = memo_file_name(root, &["x.jar".to_string(), "z.jar".to_string()]);
        assert_ne!(a, b);
        // A different root → a different memo name.
        assert_ne!(a, memo_file_name(Path::new("C:/other"), &["x.jar".to_string(), "y.jar".to_string()]));
    }

    #[test]
    fn the_cache_respects_the_pom_mtime() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-list-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("list.json");
        let list = ResolvedList {
            jars: vec!["a.jar".to_string(), "b.jar".to_string()],
            source: "offline",
            ..ResolvedList::default()
        };
        save_entry_to(&path, 42, &list);
        assert_eq!(load_entry_from(&path, 42).unwrap().jars, list.jars);
        // A different pom mtime invalidates the cache.
        assert!(load_entry_from(&path, 43).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A list written by an older resolver is not read back, however fresh the poms are. Without
    /// this, a resolver fix reaches only projects whose poms happen to be edited afterwards.
    #[test]
    fn a_list_from_an_older_resolver_is_not_served() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-epoch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("list.json");
        std::fs::write(
            &path,
            br#"{"pom_mtime":9,"jars":["a.jar"],"missing_paths":[],"partial":null,"source":"offline"}"#,
        )
        .unwrap();
        assert!(load_entry_from(&path, 9).is_none(), "no `resolver` stamp — pre-epoch");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The half of the freshness key that an mtime cannot express: the artifact arrived. Without
    /// this, a project that was missing one jar served its half-classpath until a pom was edited.
    #[test]
    fn a_missing_artifact_that_arrives_invalidates_the_cache() {
        let dir = std::env::temp_dir().join(format!("bennu-deps-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("list.json");
        let absent = dir.join("not-yet.jar");
        let list = ResolvedList {
            jars: vec!["a.jar".to_string()],
            partial: Some("one missing".to_string()),
            missing_paths: vec![absent.display().to_string()],
            source: "offline",
            cacheable: true,
        };
        save_entry_to(&path, 7, &list);
        // Still missing → the cache is still true, and the sentence survives with it.
        let hit = load_entry_from(&path, 7).unwrap();
        assert_eq!(hit.partial.as_deref(), Some("one missing"));
        // It arrives → the list is stale, whatever the poms say.
        std::fs::write(&absent, b"x").unwrap();
        assert!(load_entry_from(&path, 7).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
