//! Project open orchestration + file reading.
//!
//! [`open_project`] ties the leaf pieces together — pom parse, capability detection,
//! JDK detection, encoding label — into the [`ProjectInfo`] the `bennu_open_project`
//! handler returns. [`read_file`] decodes a file in the project's resolved encoding
//! (docs §5 #21). Both are pure over (filesystem + config inputs); the backend glue
//! (which config, which overrides) stays in `bennu-be`.

use std::path::Path;

use bennu_proto::prelude::{FileContents, ProjectInfo};

use crate::error::ProjectError;
use crate::{capability, encoding, jdk, pom};

/// Inputs the backend supplies from its config (the per-project overrides + default
/// encoding). Keeping them as an explicit struct means this leaf never reaches into
/// `bennu-core`'s config type — the backend maps its config into this.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions<'a> {
    /// Fallback encoding label when the pom declares none (`"UTF-8"` typically).
    pub default_encoding: &'a str,
    /// Explicit JDK version override for this project root, if the user set one.
    pub jdk_override: Option<&'a str>,
}

/// Open the Maven project rooted at `root`: parse the root pom, detect capabilities /
/// JDK, and assemble the [`ProjectInfo`]. Errors when `root` isn't a directory or has
/// no `pom.xml` (Phase 0 is Maven-only).
pub fn open_project(root: &Path, opts: &OpenOptions) -> Result<ProjectInfo, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::NotADirectory(root.display().to_string()));
    }
    let pom_path = root.join("pom.xml");
    if !pom_path.is_file() {
        return Err(ProjectError::NoPom(root.display().to_string()));
    }
    let xml = std::fs::read_to_string(&pom_path).map_err(|e| ProjectError::Io(e.to_string()))?;
    let pom = pom::parse(&xml);

    let name = if pom.name.is_empty() {
        root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        pom.name.clone()
    };

    let capabilities = capability::detect(root, &pom);
    let jdk = jdk::detect(&pom, opts.jdk_override);

    Ok(ProjectInfo {
        root: root.display().to_string(),
        name,
        modules: pom.modules.clone(),
        jdk,
        capabilities,
    })
}

/// Read `file` decoded in the project's encoding. `encoding_override` (per-project or
/// per-file, from config) wins; else the pom's declared encoding; else
/// `default_encoding`. Returns the decoded text + the encoding that applied.
pub fn read_file(
    project_root: &Path,
    file: &Path,
    default_encoding: &str,
    encoding_override: Option<&str>,
) -> Result<FileContents, ProjectError> {
    let bytes = std::fs::read(file).map_err(|e| ProjectError::Io(e.to_string()))?;

    // Resolve the label: explicit override → pom-declared → default.
    let label = match encoding_override.filter(|s| !s.is_empty()) {
        Some(l) => l.to_string(),
        None => {
            // Re-read the project encoding from the pom if present; cheap and keeps
            // read_file self-contained (no cross-call state).
            let pom_path = project_root.join("pom.xml");
            let declared = std::fs::read_to_string(&pom_path)
                .ok()
                .map(|xml| encoding::project_encoding_label(&pom::parse(&xml), default_encoding));
            declared.unwrap_or_else(|| default_encoding.to_string())
        }
    };

    let (text, applied) = encoding::decode(&bytes, &label);
    Ok(FileContents { text, encoding: applied })
}
