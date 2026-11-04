//! The collections tree: collections, folders and requests as files under `<project>/.hirundo/`.

use hirundo_core::prelude::{CollectionNode, HirundoState};
use hirundo_model::prelude::{HttpRequest, Meta};

use crate::context::store_for;

/// Every collection of the project with its folders and requests, in tree order. Empty when the project has
/// no `.hirundo/` yet.
#[arbor_rpc::handler]
fn collections_tree(_state: &HirundoState, project: String) -> Result<Vec<CollectionNode>, String> {
    store_for(&project)?.tree()
}

#[arbor_rpc::handler]
fn request_get(_state: &HirundoState, project: String, path: String) -> Result<HttpRequest, String> {
    store_for(&project)?.read_request(&path)
}

/// Save a request. With a `path` it replaces that file; without one it is created in `parent` (a collection
/// or folder) and named from its name. Returns its path either way.
#[arbor_rpc::handler]
fn request_save(_state: &HirundoState, project: String, path: Option<String>, parent: Option<String>, request: HttpRequest) -> Result<String, String> {
    let store = store_for(&project)?;
    match (path, parent) {
        (Some(path), _) => store.write_request(&path, &request).map(|_| path),
        (None, Some(parent)) => store.create_request(&parent, &request),
        (None, None) => Err("a new request needs the collection or folder it goes in".into()),
    }
}

#[arbor_rpc::handler]
fn request_duplicate(_state: &HirundoState, project: String, path: String) -> Result<String, String> {
    store_for(&project)?.duplicate_request(&path)
}

/// Delete a request, or a folder / collection with everything in it.
#[arbor_rpc::handler]
fn node_delete(_state: &HirundoState, project: String, path: String) -> Result<(), String> {
    store_for(&project)?.delete(&path)
}

#[arbor_rpc::handler]
fn collection_create(_state: &HirundoState, project: String, name: String) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("the collection needs a name".into());
    }
    store_for(&project)?.create_collection(&name)
}

#[arbor_rpc::handler]
fn folder_create(_state: &HirundoState, project: String, parent: String, name: String) -> Result<String, String> {
    if name.trim().is_empty() {
        return Err("the folder needs a name".into());
    }
    store_for(&project)?.create_folder(&parent, &name)
}

/// The settings of a collection (`bandi`) or folder (`bandi/auth`): name, auth, shared headers, variables.
#[arbor_rpc::handler]
fn meta_get(_state: &HirundoState, project: String, path: String) -> Result<Meta, String> {
    store_for(&project)?.read_meta(&path)
}

#[arbor_rpc::handler]
fn meta_save(_state: &HirundoState, project: String, path: String, meta: Meta) -> Result<(), String> {
    if meta.name.trim().is_empty() {
        return Err("the name cannot be empty".into());
    }
    store_for(&project)?.write_meta(&path, &meta)
}
