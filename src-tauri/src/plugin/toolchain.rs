/// Per-kind toolchain registry.
///
/// Each kind (jdk / node / rust / …) is stored as a JSON array at
/// `~/.config/arbor/toolchains/<kind>.json`.
///
/// A single entry can be marked `active = true`; all others are false.
/// The active entry's env vars are injected into build/run jobs by plugins
/// that consume the toolchain API.
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Path helpers
// ---------------------------------------------------------------------------

pub fn toolchains_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("toolchains")
}

// ---------------------------------------------------------------------------
// ToolchainEntry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainEntry {
    pub id:      String,
    pub label:   String,
    /// For JDK: JAVA_HOME root. For Node: path to `node` binary. For Rust: `cargo` binary.
    pub path:    String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub active:  bool,
    /// Extra env vars to inject on top of the kind-default ones.
    #[serde(default)]
    pub env:     HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// ToolchainRegistry
// ---------------------------------------------------------------------------

pub struct ToolchainRegistry {
    config_dir: PathBuf,
    /// kind → list (lazy-loaded)
    caches:     HashMap<String, Vec<ToolchainEntry>>,
}

impl ToolchainRegistry {
    pub fn new() -> Self {
        Self {
            config_dir: toolchains_dir(),
            caches:     HashMap::new(),
        }
    }

    // ── I/O helpers ────────────────────────────────────────────────────────

    fn path_for(&self, kind: &str) -> PathBuf {
        self.config_dir.join(format!("{kind}.json"))
    }

    fn ensure_loaded(&mut self, kind: &str) {
        if self.caches.contains_key(kind) {
            return;
        }
        let path = self.path_for(kind);
        let entries: Vec<ToolchainEntry> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        self.caches.insert(kind.to_string(), entries);
    }

    fn persist(&self, kind: &str) {
        let Some(entries) = self.caches.get(kind) else { return };
        let path = self.path_for(kind);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(entries) {
            let _ = std::fs::write(path, json);
        }
    }

    // ── Public API ──────────────────────────────────────────────────────────

    pub fn list(&mut self, kind: &str) -> Vec<ToolchainEntry> {
        self.ensure_loaded(kind);
        self.caches.get(kind).cloned().unwrap_or_default()
    }

    pub fn active(&mut self, kind: &str) -> Option<ToolchainEntry> {
        self.ensure_loaded(kind);
        self.caches.get(kind)?.iter().find(|e| e.active).cloned()
    }

    /// Add or replace an entry (matched by id).
    pub fn add(&mut self, kind: &str, entry: ToolchainEntry) {
        self.ensure_loaded(kind);
        let list = self.caches.entry(kind.to_string()).or_default();
        list.retain(|e| e.id != entry.id);
        list.push(entry);
        self.persist(kind);
    }

    pub fn remove(&mut self, kind: &str, id: &str) {
        self.ensure_loaded(kind);
        if let Some(list) = self.caches.get_mut(kind) {
            list.retain(|e| e.id != id);
        }
        self.persist(kind);
    }

    /// Mark exactly one entry as active; all others become inactive.
    pub fn set_active(&mut self, kind: &str, id: &str) {
        self.ensure_loaded(kind);
        if let Some(list) = self.caches.get_mut(kind) {
            for e in list.iter_mut() {
                e.active = e.id == id;
            }
        }
        self.persist(kind);
    }

    /// Returns env vars to inject.
    /// If `id` is Some, uses that entry; otherwise falls back to the active entry.
    pub fn env_for(&mut self, kind: &str, id: Option<&str>) -> HashMap<String, String> {
        let entry = match id {
            Some(specific_id) => {
                self.ensure_loaded(kind);
                self.caches
                    .get(kind)
                    .and_then(|list| list.iter().find(|e| e.id == specific_id))
                    .cloned()
            }
            None => self.active(kind),
        };
        let Some(entry) = entry else {
            return HashMap::new();
        };
        let mut env = entry.env.clone();
        // Kind-specific standard env vars.
        match kind {
            "jdk" => {
                env.entry("JAVA_HOME".to_string())
                    .or_insert_with(|| entry.path.clone());
            }
            "node" => {
                // Add the binary's parent dir to PATH so npm/npx resolve correctly.
                if let Some(parent) = std::path::Path::new(&entry.path).parent() {
                    env.entry("PATH".to_string())
                        .or_insert_with(|| parent.to_string_lossy().to_string());
                }
            }
            _ => {}
        }
        env
    }

    /// Scan common locations and return newly discovered entries (not yet added).
    /// Caller decides whether to `add()` them.
    pub fn detect(&self, kind: &str) -> Vec<ToolchainEntry> {
        match kind {
            "jdk"  => self.detect_jdk(),
            "node" => self.detect_node(),
            "rust" => self.detect_rust(),
            _      => vec![],
        }
    }

    // Detection is intentionally I/O-only — no process spawning. Earlier
    // versions ran `java -version` / `node --version` / `cargo --version`
    // synchronously to populate `version`; on Windows that meant 1–2 s of
    // frozen UI per probe (the JVM startup tax). We now leave `version =
    // None` and let callers probe lazily if they need to display it.
    fn detect_jdk(&self) -> Vec<ToolchainEntry> {
        let mut found = vec![];
        let mut seen_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

        let push_jdk = |id: String, label: String, path: String, found: &mut Vec<ToolchainEntry>, seen: &mut std::collections::HashSet<String>| {
            if seen.contains(&path) { return; }
            seen.insert(path.clone());
            found.push(ToolchainEntry {
                id, label, path,
                version: None,
                active:  false,
                env:     HashMap::new(),
            });
        };

        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            let p = std::path::Path::new(&java_home);
            if !java_home.is_empty() && p.exists() {
                push_jdk(
                    "jdk-java_home".to_string(),
                    "JDK (JAVA_HOME)".to_string(),
                    java_home,
                    &mut found, &mut seen_paths,
                );
            }
        }

        // Common install roots — scan the immediate children for a `bin/java`
        // (or `bin\java.exe` on Windows). One level deep is enough: vendors
        // like Temurin / Oracle / Liberica all install per-version dirs at
        // the top level.
        #[allow(unused_mut)]
        let mut roots: Vec<std::path::PathBuf> = vec![];
        #[cfg(target_os = "windows")]
        {
            for env_var in &["ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(pf) = std::env::var(env_var) {
                    let base = std::path::PathBuf::from(pf);
                    roots.push(base.join("Java"));
                    roots.push(base.join("Eclipse Adoptium"));
                    roots.push(base.join("Eclipse Foundation"));
                    roots.push(base.join("Microsoft"));
                    roots.push(base.join("Amazon Corretto"));
                    roots.push(base.join("BellSoft"));
                    roots.push(base.join("Zulu"));
                }
            }
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join(".jdks")); // IntelliJ-managed JDKs
            }
        }
        #[cfg(target_os = "macos")]
        {
            roots.push(std::path::PathBuf::from("/Library/Java/JavaVirtualMachines"));
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join("Library/Java/JavaVirtualMachines"));
            }
        }
        #[cfg(target_os = "linux")]
        {
            roots.push(std::path::PathBuf::from("/usr/lib/jvm"));
            roots.push(std::path::PathBuf::from("/usr/java"));
            if let Some(home) = dirs::home_dir() {
                roots.push(home.join(".sdkman/candidates/java"));
                roots.push(home.join(".jdks"));
            }
        }

        let java_bin = if cfg!(windows) { "bin\\java.exe" } else { "bin/java" };
        // macOS: actual JDK lives under `Contents/Home/`.
        #[cfg(target_os = "macos")]
        let extra_subdir = Some("Contents/Home");
        #[cfg(not(target_os = "macos"))]
        let extra_subdir: Option<&str> = None;

        for root in roots {
            let Ok(entries) = std::fs::read_dir(&root) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let candidates: Vec<std::path::PathBuf> = match extra_subdir {
                    Some(sub) => vec![path.join(sub), path.clone()],
                    None      => vec![path.clone()],
                };
                for cand in candidates {
                    if cand.join(java_bin).exists() {
                        let dir_name = cand.file_name()
                            .or_else(|| path.file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "jdk".to_string());
                        let path_str = cand.to_string_lossy().to_string();
                        let id = format!("jdk-{}", sanitize_id(&dir_name));
                        push_jdk(id, dir_name, path_str, &mut found, &mut seen_paths);
                        break;
                    }
                }
            }
        }

        found
    }

    fn detect_node(&self) -> Vec<ToolchainEntry> {
        let which = if cfg!(windows) {
            std::process::Command::new("where").arg("node").output()
        } else {
            std::process::Command::new("which").arg("node").output()
        };
        if let Ok(out) = which {
            if out.status.success() {
                let path = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !path.is_empty() {
                    return vec![ToolchainEntry {
                        id:      "node-path".to_string(),
                        label:   "Node.js (PATH)".to_string(),
                        path,
                        version: None,
                        active:  false,
                        env:     HashMap::new(),
                    }];
                }
            }
        }
        vec![]
    }

    fn detect_rust(&self) -> Vec<ToolchainEntry> {
        let cargo = dirs::home_dir().map(|h| {
            h.join(".cargo")
             .join("bin")
             .join(if cfg!(windows) { "cargo.exe" } else { "cargo" })
        });
        if let Some(path) = cargo {
            if path.exists() {
                return vec![ToolchainEntry {
                    id:      "rust-stable".to_string(),
                    label:   "Rust (stable)".to_string(),
                    path:    path.to_string_lossy().to_string(),
                    version: None,
                    active:  false,
                    env:     HashMap::new(),
                }];
            }
        }
        vec![]
    }
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c.to_ascii_lowercase() } else { '-' })
        .collect()
}
