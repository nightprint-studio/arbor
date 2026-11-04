//! OpenAPI 3 import, in two steps the modal shows between: **read** (a spec, from text, a URL or a file path,
//! becomes a plan of operations grouped by tag) and **apply** (the operations the user kept are written into
//! the project: one collection or folder named after the spec, a sub-folder per tag, a request per operation).
//!
//! The reading is `hirundo-model`'s `openapi` module (pure, tested); this file is the I/O around it.

use std::path::Path;

use hirundo_core::prelude::HirundoState;
use hirundo_model::prelude::{parse_openapi, Auth, KeyValue, OpenApiOperation, OpenApiPlan, UNTAGGED};
use serde::Serialize;

use crate::context::store_for;

/// The largest spec read from a file.
const MAX_SPEC_BYTES: u64 = 32 * 1024 * 1024;

/// A spec given as text, as a URL, or as the path of a file.
pub(crate) async fn source_text(source: &str) -> Result<String, String> {
    let s = source.trim();
    if s.starts_with("http://") || s.starts_with("https://") {
        return crate::soap::fetch(s).await;
    }
    if !s.starts_with('{') && !s.contains('\n') && Path::new(s).is_file() {
        let meta = std::fs::metadata(s).map_err(|e| e.to_string())?;
        if meta.len() > MAX_SPEC_BYTES {
            return Err("the file is larger than 32 MB".into());
        }
        return std::fs::read_to_string(s).map_err(|e| format!("could not read {s}: {e}"));
    }
    if s.is_empty() {
        return Err("paste an OpenAPI 3 spec in JSON, or give its URL or the path of its file".into());
    }
    Ok(s.to_string())
}

#[arbor_rpc::handler]
async fn openapi_read(_state: &HirundoState, source: String) -> Result<OpenApiPlan, String> {
    parse_openapi(&source_text(&source).await?)
}

/// The operations grouped by tag, tags in the order they first appear.
fn group_by_tag(operations: &[OpenApiOperation]) -> Vec<(String, Vec<&OpenApiOperation>)> {
    let mut groups: Vec<(String, Vec<&OpenApiOperation>)> = Vec::new();
    for op in operations {
        match groups.iter_mut().find(|(tag, _)| *tag == op.tag) {
            Some((_, list)) => list.push(op),
            None => groups.push((op.tag.clone(), vec![op])),
        }
    }
    groups
}

#[derive(Debug, Serialize)]
pub struct Applied {
    /// The collection or folder the requests went into.
    pub root: String,
    pub count: usize,
}

/// Write the chosen operations. `parent` empty: a new collection called `name`; otherwise a folder `name` inside
/// that collection or folder. `auth` and `vars` (typically `baseUrl`) are set on what was created.
#[arbor_rpc::handler]
fn openapi_apply(
    _state: &HirundoState,
    project: String,
    parent: Option<String>,
    name: String,
    description: String,
    auth: Auth,
    vars: Vec<KeyValue>,
    operations: Vec<OpenApiOperation>,
) -> Result<Applied, String> {
    if name.trim().is_empty() {
        return Err("the import needs a name".into());
    }
    if operations.is_empty() {
        return Err("nothing is selected".into());
    }
    let store = store_for(&project)?;
    let root = match parent.as_deref().filter(|p| !p.is_empty()) {
        Some(parent) => store.create_folder(parent, name.trim())?,
        None => store.create_collection(name.trim())?,
    };
    let mut meta = store.read_meta(&root)?;
    meta.description = description;
    if !matches!(auth, Auth::Inherit) {
        meta.auth = auth;
    }
    for v in vars {
        if !meta.vars.iter().any(|existing| existing.key == v.key) {
            meta.vars.push(v);
        }
    }
    store.write_meta(&root, &meta)?;

    let groups = group_by_tag(&operations);
    // A spec with no tags at all needs no sub-folder to hold its one anonymous group.
    let flat = groups.len() == 1 && groups[0].0 == UNTAGGED;
    let mut count = 0;
    for (tag, ops) in groups {
        let dir = if flat { root.clone() } else { store.create_folder(&root, &tag)? };
        for (i, op) in ops.into_iter().enumerate() {
            let mut request = op.request.clone();
            request.order = i as i64 + 1;
            store.create_request(&dir, &request)?;
            count += 1;
        }
    }
    Ok(Applied { root, count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hirundo_model::prelude::HttpRequest;

    fn op(tag: &str, id: &str) -> OpenApiOperation {
        OpenApiOperation { id: id.into(), tag: tag.into(), path: "/x".into(), summary: String::new(), request: HttpRequest::default() }
    }

    #[test]
    fn operations_are_grouped_by_tag_in_the_order_the_tags_appear() {
        let ops = vec![op("pets", "a"), op("admin", "b"), op("pets", "c")];
        let groups = group_by_tag(&ops);
        assert_eq!(groups.iter().map(|(t, l)| (t.as_str(), l.len())).collect::<Vec<_>>(), [("pets", 2), ("admin", 1)]);
        assert_eq!(groups[0].1[1].id, "c");
    }

    #[test]
    fn something_that_is_neither_text_nor_a_file_is_taken_as_the_text() {
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert_eq!(rt.block_on(source_text("  {\"a\":1}  ")).unwrap(), "{\"a\":1}");
        assert!(rt.block_on(source_text("   ")).is_err());
    }
}
