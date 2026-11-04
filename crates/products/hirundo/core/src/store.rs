//! The `.hirundo/` store: collections, environments and this machine's local values, as files.
//!
//! Every operation names things by a **relative path** inside `collections/` (`bandi/auth/login.toml`), which
//! is also how the window refers to a node. A path is cleaned before it is used — no `..`, no drive letters,
//! no empty parts — so a crafted one cannot leave the store. Parsing and formatting are `hirundo-model`'s; this
//! file is only the disk.

use std::fs;
use std::path::{Path, PathBuf};

use hirundo_model::prelude::*;
use serde::Serialize;

pub const DIR: &str = ".hirundo";
const COLLECTIONS: &str = "collections";
const ENVIRONMENTS: &str = "environments";
const COLLECTION_FILE: &str = "collection.toml";
const FOLDER_FILE: &str = "folder.toml";
const TYPES_FILE: &str = "env-types.toml";
const LOCAL_FILE: &str = "local.toml";

/// One entry of a collection's tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Folder { path: String, name: String, order: i64, auth: String, children: Vec<Node> },
    /// `error` is set when the file could not be read: the node is still listed, so it can be opened and fixed.
    Request { path: String, name: String, method: Method, order: i64, error: Option<String> },
}

impl Node {
    fn sort_key(&self) -> (i64, String) {
        match self {
            Node::Folder { order, name, .. } | Node::Request { order, name, .. } => (*order, name.to_lowercase()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollectionNode {
    pub path: String,
    pub name: String,
    pub order: i64,
    pub description: String,
    pub auth: String,
    pub children: Vec<Node>,
}

pub struct Store {
    dir: PathBuf,
}

/// Clean a relative path: `/`-separated, no empty, `.` or `..` parts, no drive or absolute prefix.
pub fn clean_rel(rel: &str) -> Result<String, String> {
    let unified = rel.replace('\\', "/");
    let parts: Vec<&str> = unified.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".." || p.contains(':')) {
        return Err(format!("'{rel}' is not a path inside the collections"));
    }
    Ok(parts.join("/"))
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write_text(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    // A temp file renamed over the real one: a crash cannot leave half a request.
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn stem(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl Store {
    /// The store of a project: `<project>/.hirundo`.
    pub fn for_project(project: &Path) -> Self {
        Self { dir: project.join(DIR) }
    }

    fn collections(&self) -> PathBuf {
        self.dir.join(COLLECTIONS)
    }

    fn abs(&self, rel: &str) -> Result<PathBuf, String> {
        Ok(self.collections().join(clean_rel(rel)?))
    }

    // ── Reading the tree ────────────────────────────────────────────────────────────────────────

    pub fn tree(&self) -> Result<Vec<CollectionNode>, String> {
        let root = self.collections();
        if !root.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&root).map_err(|e| format!("{}: {e}", root.display()))?.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let slug = entry.file_name().to_string_lossy().into_owned();
            let meta = self.meta_in(&path, COLLECTION_FILE, &slug);
            out.push(CollectionNode {
                path: slug.clone(),
                name: meta.name.clone(),
                order: meta.order,
                description: meta.description.clone(),
                auth: meta.auth.label().to_string(),
                children: self.children(&path, &slug),
            });
        }
        out.sort_by_key(|c| (c.order, c.name.to_lowercase()));
        Ok(out)
    }

    fn meta_in(&self, dir: &Path, file: &str, fallback_name: &str) -> Meta {
        let mut meta = fs::read_to_string(dir.join(file)).ok().and_then(|t| meta_from_text(&t).ok()).unwrap_or_default();
        if meta.name.trim().is_empty() {
            meta.name = fallback_name.to_string();
        }
        meta
    }

    fn children(&self, dir: &Path, rel: &str) -> Vec<Node> {
        let mut nodes = Vec::new();
        let Ok(entries) = fs::read_dir(dir) else { return nodes };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let child_rel = format!("{rel}/{file_name}");
            if path.is_dir() {
                let meta = self.meta_in(&path, FOLDER_FILE, &file_name);
                nodes.push(Node::Folder { path: child_rel.clone(), name: meta.name, order: meta.order, auth: meta.auth.label().to_string(), children: self.children(&path, &child_rel) });
            } else if file_name.ends_with(".toml") && file_name != COLLECTION_FILE && file_name != FOLDER_FILE {
                match read_text(&path).and_then(|t| request_from_text(&t)) {
                    Ok(r) => nodes.push(Node::Request { path: child_rel, name: if r.name.is_empty() { stem(&path) } else { r.name }, method: r.method, order: r.order, error: None }),
                    Err(e) => nodes.push(Node::Request { path: child_rel, name: stem(&path), method: Method::Get, order: i64::MAX, error: Some(e) }),
                }
            }
        }
        nodes.sort_by_key(Node::sort_key);
        nodes
    }

    // ── Requests ────────────────────────────────────────────────────────────────────────────────

    pub fn read_request(&self, rel: &str) -> Result<HttpRequest, String> {
        request_from_text(&read_text(&self.abs(rel)?)?)
    }

    /// Save over an existing request file (or create it at exactly this path).
    pub fn write_request(&self, rel: &str, request: &HttpRequest) -> Result<(), String> {
        let path = self.abs(rel)?;
        if !rel.ends_with(".toml") || rel.ends_with(COLLECTION_FILE) || rel.ends_with(FOLDER_FILE) {
            return Err(format!("'{rel}' is not a request file name"));
        }
        write_text(&path, &request_to_text(request))
    }

    /// A new request in `parent` (a collection or folder), named from its name. Returns its path.
    pub fn create_request(&self, parent: &str, request: &HttpRequest) -> Result<String, String> {
        let dir = self.abs(parent)?;
        if !dir.is_dir() {
            return Err(format!("'{parent}' does not exist"));
        }
        let slug = unique_slug(&slugify(&request.name), |s| dir.join(format!("{s}.toml")).exists() || dir.join(s).exists());
        let mut request = request.clone();
        if request.order == 0 {
            request.order = self.next_order(&dir);
        }
        let rel = format!("{}/{slug}.toml", clean_rel(parent)?);
        self.write_request(&rel, &request)?;
        Ok(rel)
    }

    pub fn duplicate_request(&self, rel: &str) -> Result<String, String> {
        let mut copy = self.read_request(rel)?;
        copy.name = format!("{} copy", copy.name);
        copy.order = 0;
        let parent = rel.rsplit_once('/').map(|(p, _)| p).ok_or("a request lives in a collection")?;
        self.create_request(parent, &copy)
    }

    fn next_order(&self, dir: &Path) -> i64 {
        self.children(dir, "").iter().map(|n| match n { Node::Folder { order, .. } | Node::Request { order, .. } => *order }).filter(|o| *o != i64::MAX).max().map_or(1, |m| m + 1)
    }

    /// Delete a request file, or a folder / collection with everything in it.
    pub fn delete(&self, rel: &str) -> Result<(), String> {
        let path = self.abs(rel)?;
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| format!("{}: {e}", path.display()))
        } else if path.is_file() {
            fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))
        } else {
            Err(format!("'{rel}' does not exist"))
        }
    }

    // ── Collections and folders ─────────────────────────────────────────────────────────────────

    /// A new collection; returns its folder name.
    pub fn create_collection(&self, name: &str) -> Result<String, String> {
        let root = self.collections();
        let slug = unique_slug(&slugify(name), |s| root.join(s).exists());
        let order = self.tree()?.iter().map(|c| c.order).max().map_or(1, |m| m + 1);
        write_text(&root.join(&slug).join(COLLECTION_FILE), &meta_to_text(&Meta { name: name.trim().to_string(), order, ..Default::default() }))?;
        Ok(slug)
    }

    /// A new folder inside a collection or folder; returns its path.
    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String, String> {
        let dir = self.abs(parent)?;
        if !dir.is_dir() {
            return Err(format!("'{parent}' does not exist"));
        }
        let slug = unique_slug(&slugify(name), |s| dir.join(s).exists() || dir.join(format!("{s}.toml")).exists());
        let order = self.next_order(&dir);
        let rel = format!("{}/{slug}", clean_rel(parent)?);
        write_text(&dir.join(&slug).join(FOLDER_FILE), &meta_to_text(&Meta { name: name.trim().to_string(), order, ..Default::default() }))?;
        Ok(rel)
    }

    fn meta_file(rel: &str) -> &'static str {
        if rel.contains('/') { FOLDER_FILE } else { COLLECTION_FILE }
    }

    /// The meta of a collection (`bandi`) or folder (`bandi/auth`).
    pub fn read_meta(&self, rel: &str) -> Result<Meta, String> {
        let dir = self.abs(rel)?;
        if !dir.is_dir() {
            return Err(format!("'{rel}' does not exist"));
        }
        Ok(self.meta_in(&dir, Self::meta_file(rel), rel.rsplit('/').next().unwrap_or(rel)))
    }

    pub fn write_meta(&self, rel: &str, meta: &Meta) -> Result<(), String> {
        let dir = self.abs(rel)?;
        if !dir.is_dir() {
            return Err(format!("'{rel}' does not exist"));
        }
        write_text(&dir.join(Self::meta_file(rel)), &meta_to_text(meta))
    }

    /// The metas above a request, outermost first: its collection, then each folder down to its own.
    pub fn chain(&self, request_rel: &str) -> Result<Vec<(String, Meta)>, String> {
        let rel = clean_rel(request_rel)?;
        let mut dirs: Vec<&str> = rel.split('/').collect();
        dirs.pop();
        if dirs.is_empty() {
            return Err(format!("'{request_rel}' is not inside a collection"));
        }
        let mut out = Vec::new();
        for depth in 1..=dirs.len() {
            let dir_rel = dirs[..depth].join("/");
            out.push((dir_rel.clone(), self.read_meta(&dir_rel)?));
        }
        Ok(out)
    }

    // ── Environments, their types, and local values ─────────────────────────────────────────────

    /// The environment types: the file's, or the defaults when there is none (or it is empty).
    pub fn read_types(&self) -> Result<Vec<EnvType>, String> {
        match fs::read_to_string(self.dir.join(TYPES_FILE)) {
            Ok(text) => {
                let types = env_types_from_text(&text)?;
                Ok(if types.is_empty() { default_types() } else { types })
            }
            Err(_) => Ok(default_types()),
        }
    }

    pub fn write_types(&self, types: &[EnvType]) -> Result<(), String> {
        validate_types(types)?;
        write_text(&self.dir.join(TYPES_FILE), &env_types_to_text(types))
    }

    pub fn list_environments(&self) -> Result<Vec<Environment>, String> {
        let dir = self.dir.join(ENVIRONMENTS);
        let Ok(entries) = fs::read_dir(&dir) else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "toml") {
                out.push(environment_from_text(&stem(&path), &read_text(&path)?)?);
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        Ok(out)
    }

    pub fn write_environment(&self, env: &Environment) -> Result<(), String> {
        validate_environment(env)?;
        write_text(&self.dir.join(ENVIRONMENTS).join(format!("{}.toml", env.id)), &environment_to_text(env))
    }

    pub fn delete_environment(&self, id: &str) -> Result<(), String> {
        if !valid_id(id) {
            return Err(format!("'{id}' is not an environment id"));
        }
        fs::remove_file(self.dir.join(ENVIRONMENTS).join(format!("{id}.toml"))).map_err(|e| format!("{id}: {e}"))
    }

    pub fn read_local(&self) -> Local {
        fs::read_to_string(self.dir.join(LOCAL_FILE)).ok().and_then(|t| local_from_text(&t).ok()).unwrap_or_default()
    }

    /// Save the local values, and make sure git will not take them.
    pub fn write_local(&self, local: &Local) -> Result<(), String> {
        let ignore = self.dir.join(".gitignore");
        if !ignore.exists() {
            if let Some(parent) = ignore.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&ignore, GITIGNORE).map_err(|e| format!("{}: {e}", ignore.display()))?;
        }
        write_text(&self.dir.join(LOCAL_FILE), &local_to_text(local))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Self {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!("hirundo-store-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Temp(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn req(name: &str, method: Method) -> HttpRequest {
        HttpRequest { name: name.into(), method, url: "{{baseUrl}}/x".into(), ..Default::default() }
    }

    #[test]
    fn a_path_cannot_leave_the_collections() {
        for bad in ["../x", "a/../b", "/abs", "C:/x", "a//b", "", "a/./b", "..\\x"] {
            assert!(clean_rel(bad).is_err(), "{bad}");
        }
        assert_eq!(clean_rel("bandi\\auth/login.toml").unwrap(), "bandi/auth/login.toml");
    }

    #[test]
    fn requests_are_created_listed_in_order_and_read_back() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        assert!(store.tree().unwrap().is_empty(), "no .hirundo yet");
        let c = store.create_collection("Bandi").unwrap();
        assert_eq!(c, "bandi");
        let folder = store.create_folder(&c, "Auth").unwrap();
        assert_eq!(folder, "bandi/auth");
        let list = store.create_request(&c, &req("Lista bandi", Method::Get)).unwrap();
        let create = store.create_request(&c, &req("Crea bando", Method::Post)).unwrap();
        let login = store.create_request(&folder, &req("Login", Method::Post)).unwrap();
        assert_eq!((list.as_str(), create.as_str(), login.as_str()), ("bandi/lista-bandi.toml", "bandi/crea-bando.toml", "bandi/auth/login.toml"));

        let tree = store.tree().unwrap();
        assert_eq!(tree.len(), 1);
        let names: Vec<String> = tree[0].children.iter().map(|n| match n { Node::Folder { name, .. } | Node::Request { name, .. } => name.clone() }).collect();
        assert_eq!(names, ["Auth", "Lista bandi", "Crea bando"], "creation order, the folder first because it was created first");
        assert_eq!(store.read_request(&list).unwrap().name, "Lista bandi");
        assert!(t.0.join(".hirundo/collections/bandi/lista-bandi.toml").is_file());
    }

    #[test]
    fn two_requests_with_the_same_name_get_different_files() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        let c = store.create_collection("A").unwrap();
        let first = store.create_request(&c, &req("Ping", Method::Get)).unwrap();
        let second = store.create_request(&c, &req("Ping", Method::Get)).unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("a/ping.toml", "a/ping-2.toml"));
        let copy = store.duplicate_request(&first).unwrap();
        assert_eq!(store.read_request(&copy).unwrap().name, "Ping copy");
    }

    #[test]
    fn a_broken_request_file_is_still_listed_so_it_can_be_fixed() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        let c = store.create_collection("A").unwrap();
        fs::write(t.0.join(".hirundo/collections/a/broken.toml"), "name = ").unwrap();
        let tree = store.tree().unwrap();
        match &tree[0].children[0] {
            Node::Request { name, error, .. } => assert!(name == "broken" && error.is_some()),
            other => panic!("{other:?}"),
        }
        assert!(store.read_request(&format!("{c}/broken.toml")).is_err());
    }

    #[test]
    fn the_chain_runs_from_the_collection_down_to_the_folder() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        let c = store.create_collection("Bandi").unwrap();
        let f = store.create_folder(&c, "Auth").unwrap();
        store.write_meta(&c, &Meta { name: "Bandi".into(), auth: Auth::Bearer { token: "{{token}}".into() }, vars: vec![KeyValue::new("baseUrl", "http://x")], ..Default::default() }).unwrap();
        let login = store.create_request(&f, &req("Login", Method::Post)).unwrap();
        let chain = store.chain(&login).unwrap();
        assert_eq!(chain.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["bandi", "bandi/auth"]);
        assert_eq!(chain[0].1.auth, Auth::Bearer { token: "{{token}}".into() });
        assert_eq!(chain[1].1.auth, Auth::Inherit, "the folder says nothing, so it inherits");
        assert!(store.chain("loose.toml").is_err());
    }

    #[test]
    fn deleting_a_folder_removes_what_is_in_it() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        let c = store.create_collection("A").unwrap();
        let f = store.create_folder(&c, "Sub").unwrap();
        store.create_request(&f, &req("X", Method::Get)).unwrap();
        store.delete(&f).unwrap();
        assert!(store.tree().unwrap()[0].children.is_empty());
        assert!(store.delete(&f).is_err());
    }

    #[test]
    fn a_request_goes_only_where_a_collection_is() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        assert!(store.create_request("nowhere", &req("X", Method::Get)).is_err());
        assert!(store.write_request("a/collection.toml", &req("X", Method::Get)).is_err());
    }

    #[test]
    fn the_defaults_stand_in_until_types_are_saved() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        assert_eq!(store.read_types().unwrap(), default_types());
        let mut types = default_types();
        types.push(EnvType { id: "qa".into(), name: "QA".into(), color: "#a371f7".into(), confirm_mutating: true, vars: vec![KeyValue::new("timeout", "5000")] });
        store.write_types(&types).unwrap();
        assert_eq!(store.read_types().unwrap(), types);
        assert!(store.write_types(&[EnvType { id: "Bad".into(), name: "x".into(), color: "#fff".into(), confirm_mutating: false, vars: vec![] }]).is_err());
    }

    #[test]
    fn environments_are_saved_listed_and_deleted() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        assert!(store.list_environments().unwrap().is_empty());
        let stg = Environment { id: "stg".into(), name: "Staging".into(), type_id: "staging".into(), vars: vec![KeyValue::new("baseUrl", "https://stg")] };
        let dev = Environment { id: "dev".into(), name: "Dev".into(), type_id: "dev".into(), vars: vec![] };
        store.write_environment(&stg).unwrap();
        store.write_environment(&dev).unwrap();
        assert_eq!(store.list_environments().unwrap(), vec![dev, stg.clone()], "by name");
        store.delete_environment("stg").unwrap();
        assert_eq!(store.list_environments().unwrap().len(), 1);
        assert!(store.delete_environment("../x").is_err());
    }

    #[test]
    fn local_values_are_kept_out_of_git() {
        let t = Temp::new();
        let store = Store::for_project(&t.0);
        assert_eq!(store.read_local(), Local::default());
        store.write_local(&Local { active_env: "stg".into(), vars: vec![KeyValue::new("port", "9090")] }).unwrap();
        assert_eq!(store.read_local().active_env, "stg");
        assert_eq!(fs::read_to_string(t.0.join(".hirundo/.gitignore")).unwrap(), "local.toml\n");
    }
}
