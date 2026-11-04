//! The endpoints of a project, for Hirundo's "Detected from code" and for Bennu's own "run this call".
//!
//! Hirundo (the HTTP client of the suite) lists the request mappings Bennu's index already holds — method, path,
//! parameters, return type, the file and the line — for the projects the user chose, and turns the ones they pick
//! into requests of its own. Bennu's editor asks for the endpoints of the open controller to put a ▶ on each. The
//! model is the one the HTTP-client generator reads, so the three cannot disagree about what an endpoint is.

use bennu_core::prelude::BennuState;
use bennu_generate::prelude::EndpointInfo;
use serde::Serialize;

use crate::code_generate::endpoint_infos;

#[derive(Debug, Serialize)]
pub struct DetectedEndpoint {
    #[serde(flatten)]
    pub info: EndpointInfo,
    /// The source file of the controller.
    pub file: String,
    /// The line of the handler method (1-based).
    pub line: u32,
}

/// Every endpoint with its file and line, in the order the index holds them.
fn detected(root: &str) -> Vec<DetectedEndpoint> {
    let Some(model) = crate::frameworks::spring_model_of(root) else { return Vec::new() };
    endpoint_infos(root)
        .into_iter()
        .zip(model.endpoints.iter())
        .map(|((info, file), e)| DetectedEndpoint { info, file, line: e.line })
        .collect()
}

/// The Spring request mappings of the project rooted at `root`. Empty when the project is not indexed (it has
/// not been opened in Bennu yet) or has none; the caller tells the two apart by asking for the project's state.
#[arbor_rpc::handler]
fn endpoints_detected(_ctx: &BennuState, root: String) -> Result<Vec<DetectedEndpoint>, String> {
    Ok(detected(&root))
}

/// The endpoints declared in one file — what the editor puts a ▶ on.
#[arbor_rpc::handler]
fn endpoints_in_file(_ctx: &BennuState, root: String, file: String) -> Result<Vec<DetectedEndpoint>, String> {
    let norm = |p: &str| p.replace('\\', "/").to_lowercase();
    let wanted = norm(&file);
    Ok(detected(&root).into_iter().filter(|e| norm(&e.file) == wanted).collect())
}
