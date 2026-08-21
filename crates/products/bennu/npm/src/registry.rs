//! The npm registry, as URLs and files — never as a socket.
//!
//! The half of the version check that can be tested without a network: what to ask for, where the
//! answer is cached, how old is too old, and — the part that decides whether anything is shown at
//! all — whether a declared range still admits the latest release.
//!
//! Same split as `bennu-cargo`'s `registry.rs`, and the same reason: the module in `bennu-be` that
//! opens the connection is short and untestable, and everything interesting is here.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The public registry. A URL rather than a constant somewhere in the fetch code, because a
/// private registry is the obvious next parameter and this is where it will go.
pub const REGISTRY: &str = "https://registry.npmjs.org";

/// The URL for one package's **latest** release.
///
/// `/<name>/latest` and not `/<name>`, which is the whole packument: every version ever published
/// with its full metadata, tens of megabytes for a popular package. The question here is "what is
/// the newest", the registry answers exactly that in a few hundred bytes, and asking for the rest
/// to throw it away would be one of those requests that is fine on a laptop and indefensible on a
/// train.
///
/// A scoped name's `/` is percent-encoded — `@scope%2Fname` is the form the registry documents,
/// and leaving it raw works by accident rather than by contract.
pub fn latest_url(base: &str, name: &str) -> String {
    format!("{}/{}/latest", base.trim_end_matches('/'), name.replace('/', "%2F"))
}

/// Where one package's cached answer lives.
///
/// The name is flattened rather than nested, so `@scope/name` is one file and not a directory it
/// shares with its scope siblings — the cache has no reason to mirror a naming convention.
pub fn cache_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", name.replace(['/', '\\', ':'], "__")))
}

/// Whether the cached answer at `path` is younger than `ttl`.
pub fn is_fresh(path: &Path, ttl: Duration) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    let Ok(modified) = meta.modified() else { return false };
    SystemTime::now().duration_since(modified).map(|age| age < ttl).unwrap_or(false)
}

/// Read a cached answer, however old. Staleness is the caller's judgement — offline, last week's
/// version list is the right answer.
pub fn read_cache(dir: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(dir, name)).ok()
}

/// Write one. Failures are silent: a cache that cannot be written is a slower editor, not a broken
/// one, and there is nothing the user could do about it from here.
pub fn write_cache(dir: &Path, name: &str, body: &str) {
    let path = cache_path(dir, name);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, body);
}

// ── Range admission ───────────────────────────────────────────────────────────

/// Whether the declared `range` already admits `version` — in which case there is nothing to say.
///
/// **This function decides whether a hint appears at all, so it errs towards silence.** A wrong
/// "update available" is worse than a missing one: the missing one costs a person nothing, and the
/// wrong one on a version somebody pinned on purpose costs them the trust that makes them read the
/// next hint at all.
///
/// So only the three forms whose meaning is unambiguous are judged:
///
/// | Form | Admits |
/// |---|---|
/// | `^1.2.3` | the same left-most non-zero component — and `^0.2.3` means `0.2.x`, which is the rule everyone gets wrong |
/// | `~1.2.3` | the same major and minor |
/// | `1.2.3` | itself, exactly |
///
/// Everything else is treated as already satisfied — a comparator range (`>=1 <2`), an alternation
/// (`1.x || 2.x`), a wildcard, a dist-tag, and every non-registry protocol: `workspace:`, `file:`,
/// `link:`, `git+…`, `github:owner/repo`, `npm:alias@^1`. Some of those *could* be judged; none of
/// them can be judged reliably from a string, and a version check that is right most of the time is
/// a version check nobody can act on without re-checking.
pub fn range_admits(range: &str, version: &str) -> bool {
    let range = range.trim();
    let Some(latest) = Semver::parse(version) else { return true };

    let (op, rest) = match range.as_bytes().first() {
        Some(b'^') => ('^', &range[1..]),
        Some(b'~') => ('~', &range[1..]),
        Some(b'v') => ('=', &range[1..]),
        Some(c) if c.is_ascii_digit() => ('=', range),
        // Anything else: not ours to judge.
        _ => return true,
    };
    let Some(base) = Semver::parse(rest.trim()) else { return true };

    // A prerelease on either side is a conversation about which prerelease, and a caret does not
    // cross into one anyway. Silence.
    if base.pre || latest.pre {
        return true;
    }

    match op {
        '=' => base == latest,
        '~' => latest.major == base.major && latest.minor == base.minor && latest.patch >= base.patch,
        // The caret's real rule: compatible up to the **left-most non-zero** component. `^0.2.3`
        // does not admit `0.3.0`, and `^0.0.3` admits nothing but itself. Reading it as "same
        // major" is the common mistake, and on the 0.x packages that fill a lockfile it produces
        // exactly the wrong answer.
        '^' if base.major > 0 => latest.major == base.major && latest.ge_from(&base),
        '^' if base.minor > 0 => {
            latest.major == 0 && latest.minor == base.minor && latest.patch >= base.patch
        }
        '^' => latest.major == 0 && latest.minor == 0 && latest.patch == base.patch,
        _ => true,
    }
}

/// The three numbers, plus whether anything followed them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Semver {
    major: u64,
    minor: u64,
    patch: u64,
    /// A `-alpha.1` or a `+build` was present. Enough to know to stay quiet; the contents are not
    /// something this needs to order.
    pre: bool,
}

impl Semver {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (core, pre) = match text.find(['-', '+']) {
            Some(i) => (&text[..i], true),
            None => (text, false),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        // A two- or one-component version is legal in a range (`^1.2`, `^1`); the missing pieces
        // are zero, which is what npm does with them.
        let minor = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
        let patch = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { major, minor, patch, pre })
    }

    /// `self >= other`, on the three numbers.
    fn ge_from(&self, other: &Self) -> bool {
        (self.major, self.minor, self.patch) >= (other.major, other.minor, other.patch)
    }
}

// ── Which package manager runs this project ──────────────────────────────────

/// The tool a project's scripts should be run with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

impl PackageManager {
    /// The program to spawn.
    pub fn program(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Yarn => "yarn",
            Self::Pnpm => "pnpm",
            Self::Bun => "bun",
        }
    }

    /// The argv that runs script `name`.
    ///
    /// `yarn <script>` and not `yarn run <script>`: both work on Yarn 1 and only the first works
    /// the same on Berry, and a project that has moved to Berry is not a project that wants to be
    /// told its editor is running the wrong command.
    pub fn run_args(self, name: &str) -> Vec<String> {
        match self {
            Self::Yarn => vec![name.to_string()],
            _ => vec!["run".to_string(), name.to_string()],
        }
    }
}

/// Which package manager a project uses, from the lockfile beside its manifest.
///
/// The **lockfile**, not a setting and not `packageManager` in the manifest: the lockfile is the
/// fact — it is what the install actually produced — and it is present in every project that has
/// been installed once. `packageManager` is a declaration that is regularly absent and
/// occasionally aspirational.
///
/// npm last, as the default, because a project with no lockfile at all has not been installed and
/// `npm` is what a machine has.
pub fn package_manager_for(manifest_dir: &Path) -> PackageManager {
    for (file, pm) in [
        ("bun.lockb", PackageManager::Bun),
        ("bun.lock", PackageManager::Bun),
        ("pnpm-lock.yaml", PackageManager::Pnpm),
        ("yarn.lock", PackageManager::Yarn),
    ] {
        if manifest_dir.join(file).is_file() {
            return pm;
        }
    }
    PackageManager::Npm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_caret_is_compatible_up_to_the_leftmost_nonzero_component() {
        // The ordinary case everybody knows.
        assert!(range_admits("^1.2.3", "1.9.0"));
        assert!(range_admits("^1.2.3", "1.2.3"));
        assert!(!range_admits("^1.2.3", "2.0.0"));
        // …and an older release than the range's floor is not an upgrade either way.
        assert!(!range_admits("^1.2.3", "1.2.0"));

        // The rule everybody gets wrong, and the one that matters most: a lockfile is full of
        // 0.x packages, and reading `^` as "same major" would call every one of them up to date.
        assert!(range_admits("^0.2.3", "0.2.9"));
        assert!(!range_admits("^0.2.3", "0.3.0"));
        assert!(!range_admits("^0.0.3", "0.0.4"));
    }

    #[test]
    fn a_tilde_is_the_same_minor_and_an_exact_version_is_itself() {
        assert!(range_admits("~1.2.3", "1.2.9"));
        assert!(!range_admits("~1.2.3", "1.3.0"));
        assert!(range_admits("1.2.3", "1.2.3"));
        assert!(!range_admits("1.2.3", "1.2.4"));
        // `v` prefixed, which a hand-edited manifest carries often enough.
        assert!(!range_admits("v1.2.3", "1.3.0"));
    }

    #[test]
    fn everything_it_cannot_judge_reliably_stays_silent() {
        // A hint is a claim, and a wrong claim on a deliberate pin is what stops somebody reading
        // the next one. Every form here COULD be reasoned about and none can be reasoned about
        // from a string alone.
        for range in [
            ">=1.0.0 <2.0.0", "1.x || 2.x", "*", "", "x", "latest", "next",
            "workspace:*", "file:../shared", "link:../shared",
            "git+https://github.com/o/r.git", "github:owner/repo", "npm:other@^1.0.0",
            "1.2.3 - 2.0.0",
        ] {
            assert!(range_admits(range, "9.9.9"), "`{range}` must not produce a hint");
        }
        // A prerelease on either side is a conversation about which prerelease.
        assert!(range_admits("^1.2.3", "2.0.0-beta.1"));
        assert!(range_admits("^1.2.3-beta.1", "2.0.0"));
        // A version that is not a version at all.
        assert!(range_admits("^1.2.3", "not-a-version"));
    }

    #[test]
    fn a_short_range_fills_its_missing_components_with_zero() {
        // `^1` and `^1.2` are legal and mean what npm says they mean.
        assert!(range_admits("^1", "1.9.9"));
        assert!(!range_admits("^1", "2.0.0"));
        assert!(range_admits("~1.2", "1.2.7"));
        assert!(!range_admits("~1.2", "1.3.0"));
    }

    #[test]
    fn a_scoped_name_is_encoded_in_the_url_and_flattened_in_the_cache() {
        assert_eq!(
            latest_url(REGISTRY, "@sveltejs/kit"),
            "https://registry.npmjs.org/@sveltejs%2Fkit/latest",
        );
        assert_eq!(latest_url("https://r.example/", "lodash"), "https://r.example/lodash/latest");
        // One file, not a directory shared with the scope's siblings.
        let p = cache_path(Path::new("/c"), "@sveltejs/kit");
        assert_eq!(p, Path::new("/c").join("@sveltejs__kit.json"));
    }

    #[test]
    fn the_package_manager_is_read_off_the_lockfile() {
        let root = std::env::temp_dir().join("bennu-npm-pm-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // No lockfile: not installed yet, and npm is what a machine has.
        assert_eq!(package_manager_for(&root), PackageManager::Npm);

        std::fs::write(root.join("yarn.lock"), "").unwrap();
        assert_eq!(package_manager_for(&root), PackageManager::Yarn);
        // Yarn, not `yarn run`: both work on Yarn 1 and only this works the same on Berry.
        assert_eq!(PackageManager::Yarn.run_args("dev"), ["dev"]);
        assert_eq!(PackageManager::Npm.run_args("dev"), ["run", "dev"]);

        // A repository carrying both is a repository mid-migration, and pnpm's lock is the one
        // that was written last in every migration that goes that way.
        std::fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
        assert_eq!(package_manager_for(&root), PackageManager::Pnpm);

        let _ = std::fs::remove_dir_all(&root);
    }
}
