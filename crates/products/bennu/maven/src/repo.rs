//! Where the local repository is, and what is in it.
//!
//! ## Why this is not `~/.m2/repository`
//!
//! It usually is. But a machine that shares one repository between users, a CI image that keeps it
//! on a fast disk, or a developer who simply moved it, all say so in the same two places Maven
//! looks — `settings.xml` and `-Dmaven.repo.local` — and every one of them was a project where
//! bennu reported *nothing resolved* on a tree that builds. The answer costs one file read, once
//! per process, and being wrong about it costs the whole dependency tier.
//!
//! ## The layout is the coordinate
//!
//! ```text
//! <repo>/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar
//!        └────── groupId ─────┘└artifactId┘└version┘
//! ```
//!
//! Which is why [`LocalRepo`] can answer *"is this dependency here"* without an index, without
//! Maven and without a network: it is one `stat` on a path built from the coordinate. That single
//! fact is what the red underline in a pom, the offline resolver and the version list all stand on.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The coordinate lives in [`bennu_deps::prelude::Coord`] — one type, read off a path and built
/// back into one, in the crate that owns the layout. Re-exported because every call site here is
/// about a coordinate and importing it from two crates would be the wrong kind of honesty.
pub use bennu_deps::prelude::{coord_of, coord_under, Coord};

/// The local repository: a root directory, and the questions its layout answers.
#[derive(Debug, Clone)]
pub struct LocalRepo {
    root: PathBuf,
}

impl LocalRepo {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The repository this machine uses — see [`local_repository`].
    pub fn discover() -> Self {
        Self { root: local_repository() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    /// `<repo>/<group as dirs>/<artifactId>/<version>`.
    pub fn version_dir(&self, coord: &Coord) -> PathBuf {
        coord.version_dir(&self.root)
    }

    /// `<repo>/<group as dirs>/<artifactId>` — the directory the versions sit in.
    pub fn artifact_dir(&self, group_id: &str, artifact_id: &str) -> PathBuf {
        Coord::new(group_id, artifact_id, "").artifact_dir(&self.root)
    }

    /// The artifact's own file (`spring-web-5.3.27.jar`, `spring-core-5.3.27-tests.jar`).
    pub fn artifact_file(&self, coord: &Coord) -> PathBuf {
        coord.file_in(&self.root)
    }

    /// The artifact's `.pom`, which is what the offline resolver reads to find its own dependencies.
    pub fn pom_file(&self, coord: &Coord) -> PathBuf {
        coord.pom_in(&self.root)
    }

    /// The coordinate a path **inside this repository** encodes — the reading direction, for a
    /// caller that has a jar and wants to know what it is.
    pub fn coord_at(&self, path: &Path) -> Option<Coord> {
        coord_under(&self.root, path)
    }

    /// The artifact file, when it is actually there.
    ///
    /// A BOM (`<type>pom</type>`) resolves to its `.pom`: it has no jar and never will, and
    /// answering `None` for one would mark every BOM in a modern project as missing.
    pub fn resolve(&self, coord: &Coord) -> Option<PathBuf> {
        if !coord.is_complete() {
            return None;
        }
        let file = if coord.is_pom() { self.pom_file(coord) } else { self.artifact_file(coord) };
        file.is_file().then_some(file)
    }

    pub fn has(&self, coord: &Coord) -> bool {
        self.resolve(coord).is_some()
    }

    /// Whether the repository knows this `groupId:artifactId` at all, at any version. The question
    /// behind the *other* half of the red underline: a wrong version and a wrong artifactId are the
    /// same symptom and very different mistakes, so they get different messages.
    pub fn knows(&self, group_id: &str, artifact_id: &str) -> bool {
        self.artifact_dir(group_id, artifact_id).is_dir()
    }

    /// Every version of an artifact that is installed, newest first.
    ///
    /// A directory with nothing but a `_remote.repositories` or a `*.lastUpdated` in it is Maven's
    /// record of a download that **failed**, and counting it as an installed version is how a
    /// completion list ends up offering the one version that is guaranteed not to work.
    pub fn versions(&self, group_id: &str, artifact_id: &str) -> Vec<String> {
        let dir = self.artifact_dir(group_id, artifact_id);
        let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
        let mut out: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| has_artifact_file(&e.path()))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        sort_versions_desc(&mut out);
        out
    }

    /// The newest installed version of an artifact, if any.
    pub fn latest(&self, group_id: &str, artifact_id: &str) -> Option<String> {
        self.versions(group_id, artifact_id).into_iter().next()
    }
}

/// Whether a version directory holds **anything Maven published** rather than the residue of a
/// failed download (a lone `.lastUpdated`).
///
/// Deliberately *not* "is this usable as a jar". A pom counts, because for a BOM the pom **is** the
/// artifact — and because [`LocalRepo::versions`] feeds version completion and the dependency
/// panel, which should offer a version that exists rather than only one already compiled against.
///
/// The distinction matters and has been got wrong here: `resolve` asks the narrower question — is
/// the *jar* on disk — so a version can be listed by `versions` and refused by `resolve` at the
/// same time. That is not a contradiction, it is two questions; the message that reports it has to
/// say which one it asked (see `check::unresolved`, whose third state exists for exactly this).
fn has_artifact_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name.ends_with(".jar") || name.ends_with(".pom") || name.ends_with(".war") || name.ends_with(".aar")
    })
}

/// The local repository for this machine, resolved once.
///
/// Four sources, in Maven's own order of authority:
///   1. `-Dmaven.repo.local` in `MAVEN_OPTS` / `MAVEN_ARGS` — what a wrapper script or a CI job sets;
///   2. the user's `~/.m2/settings.xml`;
///   3. the global `$MAVEN_HOME/conf/settings.xml`;
///   4. `~/.m2/repository`.
pub fn local_repository() -> PathBuf {
    static CACHED: OnceLock<PathBuf> = OnceLock::new();
    CACHED.get_or_init(discover_local_repository).clone()
}

fn discover_local_repository() -> PathBuf {
    for var in ["MAVEN_OPTS", "MAVEN_ARGS", "MAVEN_CLI_OPTS"] {
        if let Some(p) = std::env::var(var).ok().as_deref().and_then(repo_local_flag) {
            return PathBuf::from(p);
        }
    }
    let home = arbor_core::prelude::user_home();
    if let Some(home) = &home {
        if let Some(p) = settings_local_repository(&home.join(".m2").join("settings.xml"), home) {
            return p;
        }
    }
    for var in ["MAVEN_HOME", "M2_HOME"] {
        let Ok(dir) = std::env::var(var) else { continue };
        if dir.is_empty() {
            continue;
        }
        let settings = PathBuf::from(dir).join("conf").join("settings.xml");
        if let Some(p) = settings_local_repository(&settings, home.as_deref().unwrap_or(Path::new(""))) {
            return p;
        }
    }
    home.unwrap_or_default().join(".m2").join("repository")
}

/// `-Dmaven.repo.local=<path>` out of an options string, quotes stripped.
fn repo_local_flag(opts: &str) -> Option<String> {
    let at = opts.find("-Dmaven.repo.local=")? + "-Dmaven.repo.local=".len();
    let rest = &opts[at..];
    let value = match rest.starts_with('"') {
        true => rest[1..].split('"').next().unwrap_or_default(),
        false => rest.split_whitespace().next().unwrap_or_default(),
    };
    (!value.is_empty()).then(|| value.to_string())
}

/// `<localRepository>` out of a `settings.xml`, with `${user.home}` and `${env.X}` expanded — both
/// are ordinary in a hand-written settings file, and a path left holding a literal `${user.home}`
/// resolves to nothing at all.
fn settings_local_repository(settings: &Path, home: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(settings).ok()?;
    let doc = bennu_xml::prelude::scan(&text);
    let tags = &doc.tags;
    let open = tags.iter().position(|t| {
        t.local() == "localRepository" && t.kind != bennu_xml::prelude::TagKind::Close
    })?;
    let start = tags[open].end;
    let close = tags[open + 1..]
        .iter()
        .find(|t| t.local() == "localRepository" && t.kind == bennu_xml::prelude::TagKind::Close)?;
    let raw = text.get(start..close.start)?.trim();
    if raw.is_empty() {
        return None;
    }
    let expanded = expand_settings_vars(raw, home);
    (!expanded.is_empty()).then(|| PathBuf::from(expanded))
}

fn expand_settings_vars(raw: &str, home: &Path) -> String {
    let mut out = raw.replace("${user.home}", &home.to_string_lossy());
    while let Some(start) = out.find("${env.") {
        let Some(len) = out[start..].find('}') else { break };
        let name = out[start + 6..start + len].to_string();
        let value = std::env::var(&name).unwrap_or_default();
        out.replace_range(start..start + len + 1, &value);
    }
    out
}

// ── version ordering ─────────────────────────────────────────────────────────

/// Sort versions newest-first, the way a person reads them.
///
/// Not Maven's full `ComparableVersion` — that one has qualifier aliases and an unbounded item
/// grammar — but the part that decides every list a human looks at: numeric segments compare as
/// numbers (so `10` beats `9`), and a release beats its own pre-releases (`1.2` over
/// `1.2-SNAPSHOT`, `1.2-RC1`). A tie falls back to the string, so the order is always total.
pub fn sort_versions_desc(versions: &mut [String]) {
    versions.sort_by(|a, b| compare_versions(b, a));
}

/// Compare two versions the way [`sort_versions_desc`] orders them. `Greater` means *newer*.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_num, a_qual) = split_version(a);
    let (b_num, b_qual) = split_version(b);
    for i in 0..a_num.len().max(b_num.len()) {
        let x = a_num.get(i).copied().unwrap_or(0);
        let y = b_num.get(i).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    // Same numbers: no qualifier is the release, and a release is newer than anything qualified.
    match (a_qual.is_empty(), b_qual.is_empty()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => qualifier_rank(&a_qual).cmp(&qualifier_rank(&b_qual)).then_with(|| a_qual.cmp(&b_qual)),
    }
}

/// The numeric prefix (`1.2.3`) and whatever qualifies it (`-SNAPSHOT`, `.RELEASE`, `-rc1`).
fn split_version(v: &str) -> (Vec<u64>, String) {
    let mut nums = Vec::new();
    let mut rest = v;
    loop {
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if end == 0 {
            break;
        }
        let Ok(n) = rest[..end].parse::<u64>() else { break };
        nums.push(n);
        rest = &rest[end..];
        match rest.strip_prefix('.') {
            Some(next) if next.starts_with(|c: char| c.is_ascii_digit()) => rest = next,
            _ => break,
        }
    }
    (nums, rest.trim_start_matches(['.', '-', '_']).to_string())
}

/// Where a qualifier sits relative to the others: a snapshot is the oldest thing that can carry a
/// version number, a milestone precedes a release candidate, and anything unrecognised is assumed
/// to be a vendor suffix (`1.2.3-jre`, `1.2.3.RELEASE`) rather than a pre-release.
fn qualifier_rank(q: &str) -> i32 {
    let lower = q.to_ascii_lowercase();
    if lower.starts_with("snapshot") {
        return 0;
    }
    if lower.starts_with("alpha") || lower.starts_with('a') && lower.len() <= 3 {
        return 1;
    }
    if lower.starts_with("beta") || lower.starts_with('b') && lower.len() <= 3 {
        return 2;
    }
    if lower.starts_with('m') && lower[1..].starts_with(|c: char| c.is_ascii_digit()) {
        return 3;
    }
    if lower.starts_with("rc") || lower.starts_with("cr") {
        return 4;
    }
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delegation, in one test: a repository is a root plus [`Coord`]'s layout, and the two
    /// packagings that name a different file than they claim go through it unchanged. What the
    /// layout itself does is [`bennu_deps::repo`]'s own test.
    #[test]
    fn a_repository_is_a_root_plus_the_coordinates_layout() {
        let repo = LocalRepo::at("/r");
        let web = Coord::new("org.springframework", "spring-web", "5.3.27");
        assert!(repo
            .artifact_file(&web)
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with("/r/org/springframework/spring-web/5.3.27/spring-web-5.3.27.jar"));
        let test_jar = Coord { packaging: "test-jar".into(), ..Coord::new("g", "core", "1.0") };
        assert!(repo.artifact_file(&test_jar).to_string_lossy().ends_with("core-1.0-tests.jar"));
        // A BOM has no jar and never will, so it resolves to its `.pom` — the reason `resolve`
        // asks the coordinate what it is instead of always appending `.jar`.
        let bom = Coord { packaging: "pom".into(), ..Coord::new("g", "bom", "1.0") };
        assert!(repo.artifact_file(&bom).to_string_lossy().ends_with("bom-1.0.pom"));
    }

    #[test]
    fn versions_read_newest_first_with_numbers_as_numbers() {
        let mut v: Vec<String> =
            ["1.9", "1.10", "1.2", "2.0-SNAPSHOT", "2.0"].iter().map(|s| s.to_string()).collect();
        sort_versions_desc(&mut v);
        assert_eq!(v, ["2.0", "2.0-SNAPSHOT", "1.10", "1.9", "1.2"]);
    }

    #[test]
    fn a_release_outranks_its_own_pre_releases() {
        assert_eq!(compare_versions("1.2.0", "1.2.0-RC1"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("1.2.0-RC1", "1.2.0-M3"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("1.2.0-M3", "1.2.0-SNAPSHOT"), std::cmp::Ordering::Greater);
        // A vendor suffix is not a pre-release: 31.1-jre is not older than 31.1-alpha.
        assert_eq!(compare_versions("31.1-jre", "31.1-alpha"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn a_relocated_repository_is_read_out_of_the_options() {
        assert_eq!(repo_local_flag("-Xmx2g -Dmaven.repo.local=/fast/m2 -q").as_deref(), Some("/fast/m2"));
        assert_eq!(
            repo_local_flag("-Dmaven.repo.local=\"C:/Program Files/m2\"").as_deref(),
            Some("C:/Program Files/m2")
        );
        assert_eq!(repo_local_flag("-Xmx2g"), None);
    }

    #[test]
    fn settings_local_repository_expands_the_variables_it_is_written_with() {
        let dir = std::env::temp_dir().join(format!("bennu-mvn-settings-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let settings = dir.join("settings.xml");
        std::fs::write(
            &settings,
            "<settings><localRepository>${user.home}/repo-elsewhere</localRepository></settings>",
        )
        .unwrap();
        let got = settings_local_repository(&settings, Path::new("/home/u")).unwrap();
        assert_eq!(got, PathBuf::from("/home/u/repo-elsewhere"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
