//! The project's annotation values and configuration keys, kept per project root and refreshed off the request path.
//!
//! A request never walks the project: it asks for the tables as they are, and if they are older than
//! [`REFRESH_EVERY`] a background thread brings them up to date — only the files whose size or modification time
//! changed are read again, and a file that disappeared is forgotten. The first request of a session therefore finds
//! the tables empty, and the next one (a few hundred milliseconds later on a large project) finds them full.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

use bennu_proto::prelude::CompletionItem;

use crate::annotation_values::{config_key_typed, config_keys, is_config_file, usages_in, AnnotationCensus, ArgSite, ConfigKeys};

const REFRESH_EVERY: Duration = Duration::from_secs(30);
const MAX_FILE_BYTES: u64 = 1_000_000;
const SKIP_DIRS: &[&str] = &[".git", "target", "build", "node_modules", "out", ".idea", ".gradle", ".mvn"];
const BUILD_FILES: &[&str] = &["pom.xml", "build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"];

/// What is known about one project root.
pub struct ProjectValues {
    root: PathBuf,
    annotations: RwLock<AnnotationCensus>,
    config: RwLock<ConfigKeys>,
    /// Size and modification time of each file as last read.
    seen: Mutex<HashMap<PathBuf, (SystemTime, u64)>>,
    running: AtomicBool,
    last_refresh: Mutex<Option<Instant>>,
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<ProjectValues>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<ProjectValues>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// The topmost directory above `file` with a build file in it, stopping at the first one that holds a `.git`.
fn root_of(file: &Path) -> Option<PathBuf> {
    let mut best = None;
    for dir in file.parent()?.ancestors() {
        if BUILD_FILES.iter().any(|b| dir.join(b).is_file()) {
            best = Some(dir.to_path_buf());
        }
        if dir.join(".git").exists() {
            best = best.or_else(|| Some(dir.to_path_buf()));
            break;
        }
    }
    best
}

impl ProjectValues {
    fn new(root: PathBuf) -> Self {
        ProjectValues {
            root,
            annotations: RwLock::default(),
            config: RwLock::default(),
            seen: Mutex::default(),
            running: AtomicBool::new(false),
            last_refresh: Mutex::new(None),
        }
    }

    /// The tables of the project `file` belongs to, with a refresh started if they are stale. `None` when the file is
    /// in no project this can find.
    pub fn for_file(file: &Path) -> Option<Arc<ProjectValues>> {
        let root = root_of(file)?;
        let values = {
            let mut all = registry().lock().unwrap_or_else(|p| p.into_inner());
            Arc::clone(all.entry(root.clone()).or_insert_with(|| Arc::new(ProjectValues::new(root))))
        };
        values.refresh_in_background();
        Some(values)
    }

    fn refresh_in_background(self: &Arc<Self>) {
        let stale = self.last_refresh.lock().unwrap_or_else(|p| p.into_inner()).is_none_or(|t| t.elapsed() > REFRESH_EVERY);
        if !stale || self.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new().name("annotation-values-scan".into()).spawn(move || {
            me.refresh();
            *me.last_refresh.lock().unwrap_or_else(|p| p.into_inner()) = Some(Instant::now());
            me.running.store(false, Ordering::Release);
        });
        if spawned.is_err() {
            self.running.store(false, Ordering::Release);
        }
    }

    /// Reads again what changed since the last pass. Blocking: the request path never calls it.
    pub(crate) fn refresh(&self) {
        let mut found = Vec::new();
        walk(&self.root, &mut found);
        let mut seen = self.seen.lock().unwrap_or_else(|p| p.into_inner());
        let present: std::collections::HashSet<&PathBuf> = found.iter().map(|(p, ..)| p).collect();
        let gone: Vec<PathBuf> = seen.keys().filter(|p| !present.contains(p)).cloned().collect();
        for path in gone {
            seen.remove(&path);
            self.annotations.write().unwrap_or_else(|p| p.into_inner()).remove_file(&path);
            self.config.write().unwrap_or_else(|p| p.into_inner()).remove_file(&path);
        }
        for (path, modified, len) in found {
            if seen.get(&path) == Some(&(modified, len)) {
                continue;
            }
            seen.insert(path.clone(), (modified, len));
            let Ok(bytes) = std::fs::read(&path) else { continue };
            // legacy sources are not always UTF-8; a lossy read keeps the structure, which is all this needs
            let content = String::from_utf8_lossy(&bytes);
            if is_config_file(&path) {
                let yaml = path.extension().and_then(|e| e.to_str()).is_some_and(|e| e != "properties");
                self.config.write().unwrap_or_else(|p| p.into_inner()).set_file(path, config_keys(&content, yaml));
            } else {
                self.annotations.write().unwrap_or_else(|p| p.into_inner()).set_file(path, usages_in(&content));
            }
        }
    }

    /// The values the project writes for `annotation`'s `element` (other files than `current`), most written first.
    pub fn annotation_values(&self, annotation: &str, element: &str, current: &Path) -> Vec<(String, u32)> {
        self.annotations.read().unwrap_or_else(|p| p.into_inner()).values(annotation, element, current)
    }

    /// The configuration keys that continue `typed`, the nearest module's first.
    pub fn config_keys(&self, typed: &str, current: &Path) -> Vec<String> {
        self.config.read().unwrap_or_else(|p| p.into_inner()).matching(typed, current)
    }
}

/// The completion rows for a caret inside an annotation's parentheses: configuration keys where the annotation takes
/// one, otherwise the values the project writes for that element. `import_for(simple)` gives the import a type used
/// in a constant (`MediaType.X`) needs, when the file lacks it.
pub fn argument_items(
    values: &ProjectValues,
    file: &Path,
    offset: usize,
    site: &ArgSite,
    import_for: &dyn Fn(&str) -> Option<String>,
) -> Vec<CompletionItem> {
    if let Some((key, start)) = config_key_typed(site) {
        return values
            .config_keys(&key, file)
            .into_iter()
            .take(60)
            .enumerate()
            .map(|(rank, k)| CompletionItem {
                label: k.clone(),
                kind: "text".into(),
                detail: Some("configuration key".into()),
                insert_text: Some(k),
                replace_start: Some(start),
                replace_end: Some(offset),
                sort_text: Some(format!("{rank:04}")),
                ..Default::default()
            })
            .collect();
    }
    let typed = site.typed.to_lowercase();
    values
        .annotation_values(&site.annotation, &site.element, file)
        .into_iter()
        .filter_map(|(value, count)| {
            let literal = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
            let content = if literal { &value[1..value.len() - 1] } else { value.as_str() };
            // inside a string only strings fit; outside, a string is offered whole with its quotes
            if (site.in_string && !literal) || !content.to_lowercase().starts_with(&typed) || (site.in_string && content.len() == site.typed.len()) {
                return None;
            }
            Some((value.clone(), content.to_string(), literal, count))
        })
        .take(40)
        .enumerate()
        .map(|(rank, (value, content, literal, count))| {
            let first = value.split('.').next().unwrap_or("");
            let import = (!literal && value.contains('.') && first.starts_with(|c: char| c.is_ascii_uppercase())).then(|| import_for(first)).flatten();
            let mut item = CompletionItem {
                label: value.clone(),
                kind: if literal { "text" } else { "constant" }.into(),
                detail: Some(format!("{} in {} place{}", site.annotation, count, if count == 1 { "" } else { "s" })),
                auto_import: import,
                sort_text: Some(format!("{rank:04}")),
                ..Default::default()
            };
            if site.in_string {
                item.insert_text = Some(content);
                item.replace_start = Some(site.typed_start);
                item.replace_end = Some(offset);
            }
            item
        })
        .collect()
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, SystemTime, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                walk(&path, out);
            }
        } else if kind.is_file() && (name.ends_with(".java") || is_config_file(&path)) {
            if let Ok(meta) = e.metadata() {
                if meta.len() <= MAX_FILE_BYTES {
                    out.push((path, meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bennu-values-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/main/java")).unwrap();
        std::fs::create_dir_all(dir.join("src/main/resources")).unwrap();
        std::fs::write(dir.join("pom.xml"), "<project/>").unwrap();
        std::fs::write(dir.join("src/main/resources/application.yml"), "app:\n  timeout: 5\nserver:\n  port: 1\n").unwrap();
        for (name, path) in [("A", "/a"), ("B", "/a"), ("C", "/c")] {
            std::fs::write(dir.join(format!("src/main/java/{name}.java")), format!("class {name} {{ @GetMapping(\"{path}\") void f() {{}} }}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn a_refresh_reads_values_and_keys_and_a_second_one_reads_only_what_changed() {
        let dir = project();
        let values = ProjectValues::new(root_of(&dir.join("src/main/java/A.java")).unwrap());
        assert_eq!(values.root, dir);
        values.refresh();
        let got = values.annotation_values("GetMapping", "value", &dir.join("src/main/java/C.java"));
        assert_eq!(got, [("\"/a\"".to_string(), 2)]);
        assert_eq!(values.config_keys("app.", &dir.join("src/main/java/C.java")), ["app.timeout"]);

        std::fs::write(dir.join("src/main/java/B.java"), "class B { }\n").unwrap();
        std::fs::remove_file(dir.join("src/main/java/A.java")).unwrap();
        values.refresh();
        assert!(values.annotation_values("GetMapping", "value", &dir.join("src/main/java/Z.java")).iter().all(|(v, _)| v == "\"/c\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_root_is_the_topmost_directory_with_a_build_file_below_a_git_root() {
        let dir = project();
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        std::fs::create_dir_all(dir.join("module/src")).unwrap();
        std::fs::write(dir.join("module/pom.xml"), "<project/>").unwrap();
        assert_eq!(root_of(&dir.join("module/src/X.java")).unwrap(), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
