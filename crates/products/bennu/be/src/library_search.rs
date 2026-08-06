//! `library_search` domain — finding a class or a file that lives inside a **dependency jar**.
//!
//! Go-to-class and go-to-file answer questions about the project's own sources. On a legacy
//! project that is a fraction of what you actually need to read: the `struts-default.xml` that
//! declares the interceptor stack, the `spring-beans-4.3.xsd` a config file is validated
//! against, the `@RequestMapping` you are trying to remember the package of — none of it is in
//! the tree, all of it is on the classpath, and today the only way to reach any of it is to
//! find something that already links to it.
//!
//! ## Why this is searched in the backend and not shipped to the frontend
//!
//! Everything else the navigator offers is small enough to hand over whole and filter in the
//! page — a project has thousands of files, and thousands is nothing. A legacy classpath is
//! two or three hundred jars holding **hundreds of thousands** of classes. Serialising that
//! across the seam would cost tens of megabytes per opening of a dialog, to answer a question
//! about twenty of them.
//!
//! So the query comes here and only the candidates go back. What this side does is
//! deliberately *candidate selection*, not ranking: a case-insensitive subsequence match, the
//! same relation the frontend's scorer uses, so nothing that would have scored is filtered out
//! before it can be scored. The real ordering and the matched-character highlighting stay in
//! one place, where they already are.
//!
//! ## The index, and what it costs
//!
//! Reading a jar's entry names is a seek and a small read — the central directory, not the
//! contents (see [`jar_entry_names`]). Over three hundred jars that is a second or so, once:
//! the result is cached per project against the jar list that produced it, so a changed
//! dependency set rebuilds and a stable one never does. The first search after opening a
//! project pays for it; the navigator shows the spinner it already has for a category that
//! answers asynchronously.
//!
//! What runs **per keystroke** is a walk over that index, and it is written to allocate nothing
//! until something matches: candidates are compared as borrowed byte slices, and the dot form of
//! a class is built only for the few hundred that survive. Building it up front — the obvious way
//! to write this — is two allocations per class per keystroke, which on a legacy classpath is
//! over half a million of them to answer one letter, and is why this used to feel hung.
//!
//! ## Opening what you found
//!
//! A **class** goes through exactly the path a stack frame in a library already takes
//! ([`crate::intel`]'s source view): the real `.java` when the JDK ships sources or a
//! `-sources.jar` has been downloaded, otherwise the decompiled stub. Nothing new.
//!
//! A **resource** has no such machinery — it is text in a zip — so [`bennu_library_file`]
//! extracts it to the same read-only cache the decompiled views live in and hands back a path
//! the editor opens like any other file. Read-only for the obvious reason: writing to it would
//! edit a copy of something inside a jar, which helps nobody.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use bennu_classpath::prelude::{jar_entry_names, read_jar_entry_bytes};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// How many candidates one search hands back at most.
///
/// The navigator caps its own list at 200 rows, so more than this can never be seen — and the
/// point of a cap here is different anyway: it bounds the payload for a one-letter query that
/// matches a hundred thousand classes.
const MAX_HITS: usize = 400;

/// A class found in a dependency jar.
#[derive(Serialize)]
pub struct LibraryClass {
    /// Dot form, nested types included (`java.util.Map$Entry` reads as `java.util.Map.Entry`).
    pub fqcn: String,
    /// What the row shows: the type's own name, without its package.
    pub simple: String,
    /// The package, which is what tells four `Service`s apart.
    pub package: String,
    /// The artifact it came from — the jar's file name, version and all.
    pub jar: String,
}

/// A non-class entry found in a dependency jar.
#[derive(Serialize)]
pub struct LibraryFile {
    /// `<jar file name>!/<entry>` — the conventional way to name a jar entry, and what
    /// [`bennu_library_file`] takes back to open it.
    pub id: String,
    /// What the row shows: the entry's last path segment.
    pub name: String,
    /// The entry's full path inside the jar, which is what tells two `web.xml`s apart.
    pub entry: String,
    /// The artifact it came from.
    pub jar: String,
}

// ── the index ───────────────────────────────────────────────────────────────────

/// One jar's contents, by name.
struct JarEntries {
    /// The jar's file name (not its path) — the only part of it anyone reads.
    name: String,
    /// The jar's absolute path, for reading an entry back out.
    path: PathBuf,
    /// Binary class names, slash form (`org/springframework/stereotype/Service`).
    classes: Vec<String>,
    /// Everything else that is not a directory.
    resources: Vec<String>,
}

/// Every dependency jar of one project, listed. Built once per jar set.
struct LibraryIndex {
    /// The jar list this was built from. The cache key proper: a project whose dependencies
    /// changed gets a fresh index rather than yesterday's answer.
    jars: Vec<String>,
    entries: Vec<JarEntries>,
}

fn cache() -> &'static Mutex<HashMap<String, Arc<LibraryIndex>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<LibraryIndex>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The index for `root`, built if the project's jar set has changed since the last one.
///
/// The lock is released before the build: building holds no lock, so a second search arriving
/// mid-build waits for I/O rather than for a mutex, and at worst two threads build the same
/// index and the second wins. That is much cheaper than serialising every search behind the
/// one that happened to be first.
fn index_for(root: &str) -> Arc<LibraryIndex> {
    let jars = IndexService::global().dep_jars_of(root);
    if let Some(hit) = cache().lock().unwrap_or_else(|p| p.into_inner()).get(root) {
        if hit.jars == jars {
            return Arc::clone(hit);
        }
    }

    let entries = jars
        .iter()
        .map(|jar| {
            let path = PathBuf::from(jar);
            let (classes, resources) = jar_entry_names(&path);
            JarEntries {
                name: file_name_of(&path),
                path,
                classes,
                resources,
            }
        })
        .collect();

    let built = Arc::new(LibraryIndex { jars, entries });
    cache()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(root.to_string(), Arc::clone(&built));
    built
}

fn file_name_of(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

// ── matching ────────────────────────────────────────────────────────────────────

/// Lower-case one byte of a candidate.
///
/// Bytes and not `char`s: these are class and entry names, ASCII in every artifact that has ever
/// shipped, and decoding UTF-8 per character over hundreds of thousands of names is work with no
/// answer to show for it. A non-ASCII byte is never equal to an ASCII needle byte, so the worst a
/// stray one can do is fail to match — never match something it should not.
fn lower(b: u8) -> u8 {
    b.to_ascii_lowercase()
}

/// The same, reading a **binary** class name as its dot form: `org/springframework/Service` and
/// `Map$Entry` compare as `org.springframework.Service` and `Map.Entry`.
///
/// This is what lets the fully-qualified fallback match the binary name in place — without it the
/// only way to answer `springframework.Service` was to build the dot form of every class on the
/// classpath first, which is two allocations per class per keystroke.
fn lower_dotted(b: u8) -> u8 {
    match b {
        b'/' | b'$' => b'.',
        other => other.to_ascii_lowercase(),
    }
}

/// Whether `needle`'s bytes appear in `hay`, in order, under `norm`.
///
/// A subsequence and not a substring, because that is the relation the frontend's scorer uses:
/// filtering here with anything stricter would drop rows that would have matched, which reads as
/// the search being broken rather than as this being an optimisation. `needle` arrives already
/// lowercased — it is the same for every candidate, and lowering it per candidate is the one
/// avoidable cost in the loop.
fn subsequence(hay: &str, needle_lower: &str, norm: fn(u8) -> u8) -> bool {
    let needle = needle_lower.as_bytes();
    let Some(&first) = needle.first() else { return true };
    let mut wanted = first;
    let mut i = 0;
    for &b in hay.as_bytes() {
        if norm(b) == wanted {
            i += 1;
            match needle.get(i) {
                Some(&next) => wanted = next,
                None => return true,
            }
        }
    }
    false
}

/// How good a candidate looks, lower being better — used only to decide **which** candidates
/// survive the cap, never the order they are shown in (the frontend scores and sorts).
///
/// A prefix of the name beats a match somewhere inside it, and among equals the shorter name
/// wins: typing `Servlet` should not have `AbstractAnnotationConfigDispatcherServletInitializer`
/// crowd out `Servlet` itself before the frontend gets to see either.
fn rank(name: &str, needle_lower: &str, norm: fn(u8) -> u8) -> usize {
    let hay = name.as_bytes();
    let needle = needle_lower.as_bytes();
    // Byte-wise rather than `name[..n].eq_ignore_ascii_case(..)`: slicing a `str` by a byte count
    // panics when the cut lands inside a multi-byte character, which a class name from an
    // unusual artifact could produce.
    let prefix = hay.len() >= needle.len()
        && hay.iter().zip(needle).all(|(&a, &b)| norm(a) == b);
    if prefix {
        name.len()
    } else {
        name.len() + 10_000
    }
}

/// Keep the `MAX_HITS` best of `scored`, in the order they will be handed back.
fn take_best<T>(mut scored: Vec<(usize, T)>) -> Vec<T> {
    scored.sort_by_key(|(score, _)| *score);
    scored.truncate(MAX_HITS);
    scored.into_iter().map(|(_, item)| item).collect()
}

/// The dot-form name of a binary class name, with nested types read as members:
/// `java/util/Map$Entry` → `java.util.Map.Entry`.
///
/// Called **only for the candidates that matched** — a few hundred of them. Calling it for every
/// class first, to have something to match against, was two allocations per class per keystroke:
/// on a legacy classpath that is over half a million allocations to answer one letter, and it is
/// why searching the dependencies felt like it had hung.
fn dot_form(binary: &str) -> String {
    binary.replace('/', ".").replace('$', ".")
}

/// The type's own name inside a **binary** name — after the last `/`, then after the last `$`,
/// which is where `dot_form` + [`simple_of`] would have landed without building the dot form.
fn simple_binary_of(binary: &str) -> &str {
    let after_package = binary.rsplit('/').next().unwrap_or(binary);
    after_package.rsplit('$').next().unwrap_or(after_package)
}

/// The part of a dot-form name after its last dot — the type's own name.
fn simple_of(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

/// The part before the last dot — the package (plus any enclosing types).
fn package_of(fqcn: &str) -> &str {
    match fqcn.rfind('.') {
        Some(cut) => &fqcn[..cut],
        None => "",
    }
}

// ── handlers ────────────────────────────────────────────────────────────────────

/// Args for the two searches: which project's classpath, and what to look for.
#[derive(Deserialize)]
pub struct LibrarySearchArgs {
    /// Absolute path to the open project whose dependencies are searched.
    pub root: String,
    /// What the user typed. An empty query matches nothing rather than everything — "every
    /// class on the classpath" is not an answer anyone wants delivered.
    pub query: String,
}

/// Classes on the project's dependency classpath whose name matches `query`.
///
/// Matched against the **simple name first**, falling back to the fully-qualified one, so
/// typing `Service` finds `org.springframework.stereotype.Service` without also surfacing every
/// class in a package that happens to contain those letters — while `springframework.Service`
/// still works for someone who knows where they are going.
///
/// Synthetic classes are dropped: a lambda carrier or a generated accessor has no source
/// anywhere, so offering it is offering a row that cannot open. `package-info` and
/// `module-info` go too — they are declarations about a package, not types you navigate to.
#[arbor_rpc::handler]
fn bennu_library_classes(
    _ctx: &BennuState,
    args: LibrarySearchArgs,
) -> Result<Vec<LibraryClass>, String> {
    let needle = args.query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let index = index_for(&args.root);

    let mut scored = Vec::new();
    for jar in &index.entries {
        for binary in &jar.classes {
            // The order of the three tests is the whole performance story. Almost every class on
            // the classpath fails the first one, so it has to be the cheapest thing that can say
            // no — a byte walk over a borrowed slice. Everything that allocates, and the synthetic
            // check that scans the name, happen only for what survived.
            let simple = simple_binary_of(binary);
            // Simple name first: it is what was typed in nine cases out of ten, and matching it
            // ranks better than the same letters scattered through a package path.
            let score = if subsequence(simple, &needle, lower) {
                rank(simple, &needle, lower)
            } else if subsequence(binary, &needle, lower_dotted) {
                rank(binary, &needle, lower_dotted) + 100_000
            } else {
                continue;
            };
            if is_uninteresting(binary) {
                continue;
            }
            let fqcn = dot_form(binary);
            scored.push((
                score,
                LibraryClass {
                    simple: simple_of(&fqcn).to_string(),
                    package: package_of(&fqcn).to_string(),
                    fqcn,
                    jar: jar.name.clone(),
                },
            ));
        }
    }
    Ok(take_best(scored))
}

/// Whether a class on the classpath is one nobody would navigate to: made at runtime (so no
/// source for it exists anywhere), or a per-package declaration rather than a type.
fn is_uninteresting(binary: &str) -> bool {
    let simple = binary.rsplit('/').next().unwrap_or(binary);
    simple == "package-info"
        || simple == "module-info"
        || arbor_logscan::prelude::is_synthetic(binary)
}

/// Non-class files on the project's dependency classpath whose name matches `query`.
///
/// The `struts-default.xml`, the `spring-beans-4.3.xsd`, the `.tld` a JSP declares — the files a
/// legacy project is configured by and which live nowhere on disk you can open.
///
/// Matched against the entry's **last segment first**, then its whole path inside the jar, for
/// the same reason the class search is: `web.xml` should find `web.xml`, and
/// `META-INF/web.xml` should find it too.
#[arbor_rpc::handler]
fn bennu_library_files(
    _ctx: &BennuState,
    args: LibrarySearchArgs,
) -> Result<Vec<LibraryFile>, String> {
    let needle = args.query.trim().to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let index = index_for(&args.root);

    let mut scored = Vec::new();
    for jar in &index.entries {
        for entry in &jar.resources {
            let name = entry.rsplit('/').next().unwrap_or(entry);
            let score = if subsequence(name, &needle, lower) {
                rank(name, &needle, lower)
            } else if subsequence(entry, &needle, lower) {
                rank(entry, &needle, lower) + 100_000
            } else {
                continue;
            };
            scored.push((
                score,
                LibraryFile {
                    id: format!("{}!/{}", jar.name, entry),
                    name: name.to_string(),
                    entry: entry.clone(),
                    jar: jar.name.clone(),
                },
            ));
        }
    }
    Ok(take_best(scored))
}

/// Args for [`bennu_library_file`].
#[derive(Deserialize)]
pub struct LibraryFileArgs {
    /// The open project whose classpath holds the jar.
    pub root: String,
    /// `<jar file name>!/<entry>`, as [`LibraryFile::id`] gave it.
    pub id: String,
}

/// Extract a jar entry to the read-only view cache and return the path the editor opens.
///
/// The extension is kept, which is the whole point: an `.xml` that arrives as `.txt` loses its
/// highlighting, its folding and its structure view, and a schema you cannot read the shape of
/// is barely better than not having opened it.
///
/// `Err` when the jar is not on this project's classpath or the entry is not in it — both mean
/// the caller is holding an id from a classpath that has since changed, which is worth saying
/// rather than silently opening nothing.
#[arbor_rpc::handler]
fn bennu_library_file(_ctx: &BennuState, args: LibraryFileArgs) -> Result<String, String> {
    let (jar_name, entry) = args
        .id
        .split_once("!/")
        .ok_or_else(|| format!("not a jar entry: {}", args.id))?;

    let index = index_for(&args.root);
    let jar = index
        .entries
        .iter()
        .find(|j| j.name == jar_name)
        .ok_or_else(|| format!("{jar_name} is not on this project's classpath"))?;

    let bytes = read_jar_entry_bytes(&jar.path, entry)
        .ok_or_else(|| format!("{entry} could not be read from {jar_name}"))?;
    // Decoded by the one rule that reads every jar entry — a `.properties` in a library is
    // ISO-8859-1 by specification, and reading it as UTF-8 would put a `U+FFFD` where the accent
    // in an error message is. See `jar_entry_text`.
    let text = crate::dep_classpath::jar_entry_text(&bytes);

    write_library_view(jar_name, entry, &text)
        .ok_or_else(|| format!("{entry} could not be written to the view cache"))
}

/// Write an extracted jar entry to the read-only cache, under a path that keeps both the jar it
/// came from and its own path inside it — so two `web.xml`s from two artifacts are two files,
/// and the tab title still says `web.xml`.
///
/// Rewritten only when missing or changed, so a warm view opens instantly and keeps a stable
/// mtime — the same rule the decompiled views follow.
fn write_library_view(jar_name: &str, entry: &str, text: &str) -> Option<String> {
    let mut path = arbor_core::prelude::bennu_data_dir().join("library").join(sanitise(jar_name));
    for segment in entry.split('/').filter(|s| !s.is_empty()) {
        path = path.join(sanitise(segment));
    }
    std::fs::create_dir_all(path.parent()?).ok()?;
    let fresh = std::fs::read_to_string(&path).map(|c| c == text).unwrap_or(false);
    if !fresh {
        std::fs::write(&path, text).ok()?;
    }
    Some(path.to_string_lossy().replace('\\', "/"))
}

/// One path segment, with anything that cannot be in a file name replaced.
///
/// A zip entry name is not a path this filesystem agreed to: it can hold a colon, a `..`, or a
/// character Windows reserves. Replacing rather than rejecting keeps a legal name legible
/// (`spring-beans-4.3.xsd` survives untouched), and `..` becoming `__` is what stops an entry
/// name from writing outside the cache directory.
fn sanitise(segment: &str) -> String {
    if segment == ".." || segment == "." {
        return "__".to_string();
    }
    segment
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_. +()[]".contains(c) { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subsequence_is_what_matches_not_a_substring() {
        // The relation the frontend's scorer uses — filtering with anything stricter would drop
        // rows before they could be scored.
        assert!(subsequence("ServletActionContext", "sac", lower));
        assert!(subsequence("ServletActionContext", "servlet", lower));
        // The needle arrives lowercased from the handler; `lower` normalises the candidate.
        assert!(subsequence("SERVLETACTIONCONTEXT", "servlet", lower), "matching ignores case");
        assert!(!subsequence("ServletActionContext", "xyz", lower));
        assert!(!subsequence("Short", "shortish", lower), "the needle must fit");
    }

    #[test]
    fn a_binary_name_matches_as_its_dot_form_without_being_built() {
        // What lets the fully-qualified fallback run over the classpath's own strings: the
        // separators compare as dots, so nothing has to be allocated to be matched.
        assert!(subsequence("org/springframework/stereotype/Service", "springframework.service", lower_dotted));
        assert!(subsequence("java/util/Map$Entry", "map.entry", lower_dotted));
        // Under the plain normaliser the same query cannot match — which is exactly why the
        // fallback needs its own.
        assert!(!subsequence("java/util/Map$Entry", "map.entry", lower));
    }

    #[test]
    fn an_empty_needle_matches_everything() {
        // Not reachable through a handler (an empty query returns early), but the loop must not
        // depend on that to be correct.
        assert!(subsequence("anything", "", lower));
    }

    #[test]
    fn a_prefix_outranks_a_match_buried_inside() {
        // Typing `Servlet`: the type of that name must survive the cap, not be crowded out by
        // the forty framework classes that merely contain the letters.
        assert!(rank("Servlet", "servlet", lower) < rank("ServletActionContext", "servlet", lower));
        assert!(rank("ServletContext", "servlet", lower) < rank("HttpServlet", "servlet", lower));
    }

    #[test]
    fn the_simple_name_of_a_binary_name_needs_no_dot_form() {
        // The two must agree, because the search matches the first and shows the second.
        for binary in ["java/util/Map$Entry", "java/util/List", "Unpackaged", "a/b/C$D$E"] {
            assert_eq!(simple_binary_of(binary), simple_of(&dot_form(binary)));
        }
    }

    #[test]
    fn the_best_candidates_survive_the_cap() {
        let scored: Vec<(usize, usize)> = (0..MAX_HITS + 50).map(|i| (MAX_HITS + 50 - i, i)).collect();
        let kept = take_best(scored);
        assert_eq!(kept.len(), MAX_HITS);
        // Best score first, and the worst 50 are gone.
        assert_eq!(kept[0], MAX_HITS + 49);
    }

    #[test]
    fn a_nested_class_reads_as_a_member_of_its_owner() {
        assert_eq!(dot_form("java/util/Map$Entry"), "java.util.Map.Entry");
        assert_eq!(simple_of(&dot_form("java/util/Map$Entry")), "Entry");
        assert_eq!(package_of(&dot_form("java/util/Map$Entry")), "java.util.Map");
        assert_eq!(package_of("Unpackaged"), "");
    }

    #[test]
    fn classes_with_no_source_anywhere_are_not_offered() {
        assert!(is_uninteresting("com/acme/package-info"));
        assert!(is_uninteresting("module-info"));
        // Made at runtime — a link to it would always fail, which teaches you not to click any.
        assert!(is_uninteresting("com/acme/Order$$EnhancerBySpringCGLIB$$1a2b"));
        assert!(is_uninteresting("com/acme/Thing$$Lambda$14"));
        assert!(!is_uninteresting("org/springframework/stereotype/Service"));
        assert!(!is_uninteresting("java/util/Map$Entry"), "a real nested class is navigable");
    }

    /// A zip entry name is not a path this filesystem agreed to — and `..` in one is how an
    /// archive writes outside the directory it was extracted into.
    #[test]
    fn an_entry_name_cannot_escape_the_cache_directory() {
        assert_eq!(sanitise(".."), "__");
        assert_eq!(sanitise("."), "__");
        assert_eq!(sanitise("spring-beans-4.3.xsd"), "spring-beans-4.3.xsd");
        assert_eq!(sanitise("weird:name*here"), "weird_name_here");
    }
}
