//! `copy_paste` domain — copying files in the project tree, and putting them somewhere else.
//!
//! ## Why a paste is not a file copy
//!
//! For anything but Java it is exactly a file copy, and this module says so by doing that. For a
//! Java file it cannot be: the `package` declaration names the directory the file is in, so a copy
//! that keeps it is a file that does not compile — and a file that does not compile *the moment it
//! appears*, before anybody has typed anything in it, is the kind of paper cut that makes people
//! stop using a feature. So the declaration is rewritten, and if the file is being renamed on the
//! way in, the type is renamed with it.
//!
//! ## The part that needs the index
//!
//! A class refers to its neighbours by simple name, because they share a package. Copied into
//! another package it no longer shares one, and those names stop resolving — silently, in the sense
//! that nothing in the file changed. Fixing it means knowing which types the *old* package
//! declares, which is a question only the index can answer, so it is answered here rather than in
//! `bennu-refactor`: that crate is given a file and returns the names it mentions, and this one
//! turns the ones that have moved out of reach into imports.
//!
//! An import is added only where the answer is certain — the old package declares that exact simple
//! name, and the new package does not. Where two packages both declare it, the copy resolves to its
//! new neighbour and nothing is written: that is a decision about which class was meant, and
//! guessing it wrong would be a silent change of behaviour rather than a compile error somebody
//! sees.
//!
//! ## What it refuses
//!
//! Overwriting. A paste that lands on an existing file offers a name, and the *caller* decides;
//! nothing here silently replaces a file, because a paste is a reflex and an overwritten class is
//! not recoverable from one.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_intentions::prelude::insert_import_edit;
use bennu_java::prelude::infer_package;
use bennu_refactor::prelude::copy_class;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_paste_plan`].
#[derive(Deserialize)]
pub struct PastePlanArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The files being pasted, absolute.
    pub sources: Vec<String>,
    /// The directory they are being pasted into, absolute.
    pub target_dir: String,
}

/// One file a paste would create, described before anything is written.
#[derive(Debug, Clone, Serialize)]
pub struct PasteItem {
    /// The file being copied, forward-slashed.
    pub source: String,
    /// The **name** it would take in the target — what a rename dialog starts from.
    pub name: String,
    /// Where it would land, forward-slashed.
    pub target: String,
    /// A Java type, so the package (and, on a rename, the type) will be rewritten.
    pub java: bool,
    /// The type the file declares, when it declares one.
    pub type_name: String,
    /// The package it would declare after the paste. Empty for the default package, and for a
    /// target outside any recognised source root — where a Java file simply keeps what it had.
    pub package: String,
    /// Set when something is already there. The caller must offer another name; nothing here
    /// overwrites.
    pub collides: bool,
    /// Set when the paste cannot be done at all, with the reason.
    pub refused: Option<String>,
}

/// What a paste would come to.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PastePlan {
    pub items: Vec<PasteItem>,
}

/// Describe the paste without performing it: where each file lands, what it would be called, and
/// whether anything is in the way.
///
/// The dialog is built from this — a single Java file is the case that asks for a name, because it
/// is the one where "paste" means "make me another one of these".
#[arbor_rpc::handler]
fn bennu_paste_plan(_ctx: &BennuState, args: PastePlanArgs) -> Result<PastePlan, String> {
    let target = PathBuf::from(&args.target_dir);
    let package = infer_package(&target).unwrap_or_default();
    let items = args
        .sources
        .iter()
        .map(|source| describe(Path::new(source), &target, &package))
        .collect();
    Ok(PastePlan { items })
}

fn describe(source: &Path, target_dir: &Path, package: &str) -> PasteItem {
    let name = source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let java = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("java"));
    let landing = target_dir.join(&name);
    let refused = if source.is_dir() {
        // A directory holds files whose packages each have to be rewritten, and whose names may
        // each collide. That is a different operation, and half of it would be worse than none.
        Some(format!("{name} is a folder — paste copies files"))
    } else if !source.is_file() {
        Some(format!("{name} is no longer there"))
    } else {
        None
    };
    PasteItem {
        source: slash(source),
        type_name: java.then(|| stem(&name)).unwrap_or_default(),
        name,
        target: slash(&landing),
        java,
        package: package.to_string(),
        collides: landing.exists(),
        refused,
    }
}

/// Args for [`bennu_paste_files`].
#[derive(Deserialize)]
pub struct PasteArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The files being pasted, absolute.
    pub sources: Vec<String>,
    /// The directory they are being pasted into, absolute.
    pub target_dir: String,
    /// The name the **single** pasted file should take, extension included. Ignored when more than
    /// one file is being pasted: a rename only means something for one.
    #[serde(default)]
    pub new_name: String,
}

/// What a paste wrote.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PasteResult {
    /// The files created, forward-slashed — the first is what the caller opens.
    pub written: Vec<String>,
    /// The imports added, as `fqcn` strings, so the result can say what it had to fix.
    pub imports_added: Vec<String>,
}

/// Perform the paste.
///
/// Each file is written only if nothing is at its path — an existing file is an error, not an
/// overwrite. Files are written one at a time and the ones already done are kept: a paste of five
/// classes that fails on the fourth has copied three, and reporting that is more useful than
/// pretending it copied none.
#[arbor_rpc::handler]
fn bennu_paste_files(_ctx: &BennuState, args: PasteArgs) -> Result<PasteResult, String> {
    let target_dir = PathBuf::from(&args.target_dir);
    if !target_dir.is_dir() {
        return Err(format!("{} is not a folder", args.target_dir));
    }
    let package = infer_package(&target_dir).unwrap_or_default();
    let rename = (args.sources.len() == 1 && !args.new_name.trim().is_empty())
        .then(|| args.new_name.trim().to_string());

    let mut out = PasteResult::default();
    for source in &args.sources {
        let source = PathBuf::from(source);
        let name = rename.clone().unwrap_or_else(|| {
            source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
        });
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(format!("`{name}` is not a file name"));
        }
        let landing = target_dir.join(&name);
        if landing.exists() {
            return Err(format!("{} already exists", slash(&landing)));
        }

        let java = source.extension().is_some_and(|e| e.eq_ignore_ascii_case("java"));
        if java {
            let (text, imports) = java_copy(&args.root, &source, &name, &package)?;
            std::fs::write(&landing, text)
                .map_err(|e| format!("could not write {}: {e}", slash(&landing)))?;
            out.imports_added.extend(imports);
        } else {
            std::fs::copy(&source, &landing)
                .map_err(|e| format!("could not copy to {}: {e}", slash(&landing)))?;
        }
        out.written.push(slash(&landing));
    }
    Ok(out)
}

/// The text of a copied Java file, and the imports that had to be added for it to still resolve.
///
/// Falls back to the bytes as they are whenever the file does not parse or declares no type: a
/// paste that refuses because a file is mid-edit would be a paste that refuses on the file you were
/// most likely copying *from*.
fn java_copy(
    root: &str,
    source: &Path,
    new_name: &str,
    package: &str,
) -> Result<(String, Vec<String>), String> {
    let text = std::fs::read_to_string(source)
        .map_err(|e| format!("could not read {}: {e}", slash(source)))?;
    let Some(tree) = bennu_java::prelude::parse_java(&text) else {
        return Ok((text, Vec::new()));
    };
    // Outside a source root there is no package to infer, and rewriting the declaration to nothing
    // would break a file that was fine — so an empty target means "keep what it had", which
    // `copy_class` expresses as copying into the package it is already in.
    let keep = package.is_empty();
    let probe = if keep { String::new() } else { package.to_string() };
    let Some(mut plan) = copy_class(tree.root_node(), &text, &stem(new_name), &probe) else {
        return Ok((text, Vec::new()));
    };
    if keep && !plan.from_package.is_empty() {
        // Re-plan against its own package: the first pass was only how we learn what that is.
        plan = copy_class(tree.root_node(), &text, &stem(new_name), &plan.from_package.clone())
            .ok_or("this file stopped parsing")?;
    }
    let old_package = plan.from_package.clone();
    let target_package = plan.package.clone();

    if old_package == target_package {
        return Ok((plan.source, Vec::new()));
    }

    // ── the neighbours the copy can no longer see by simple name ─────────────
    let classes = IndexService::global().class_index(root).unwrap_or_default();
    let declares = |pkg: &str, simple: &str| -> Option<String> {
        classes
            .iter()
            .find(|c| c.simple == simple && package_of_fqcn(&c.fqcn) == pkg)
            .map(|c| c.fqcn.clone())
    };

    let mut source_text = plan.source;
    let mut added = Vec::new();
    for mention in &plan.mentions {
        let Some(fqcn) = declares(&old_package, mention) else { continue };
        // Both packages declare the name: the copy resolves to its new neighbour, and choosing the
        // old one for it would be a guess about which class was meant.
        if declares(&target_package, mention).is_some() {
            continue;
        }
        if let Some(edit) = insert_import_edit(&source_text, &fqcn) {
            source_text.replace_range(edit.start..edit.end, &edit.replacement);
            added.push(fqcn);
        }
    }
    Ok((source_text, added))
}

/// The package part of a dotted fully-qualified name — empty for a type in the default package.
fn package_of_fqcn(fqcn: &str) -> &str {
    fqcn.rsplit_once('.').map(|(pkg, _)| pkg).unwrap_or("")
}

/// A file name without its extension.
fn stem(name: &str) -> String {
    name.rsplit_once('.').map(|(base, _)| base.to_string()).unwrap_or_else(|| name.to_string())
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_of_a_qualified_name_is_everything_before_the_type() {
        assert_eq!(package_of_fqcn("com.acme.order.Order"), "com.acme.order");
        assert_eq!(package_of_fqcn("Order"), "", "the default package");
    }

    #[test]
    fn a_file_name_loses_only_its_last_extension() {
        assert_eq!(stem("Order.java"), "Order");
        assert_eq!(stem("archive.tar.gz"), "archive.tar");
        assert_eq!(stem("Makefile"), "Makefile");
    }
}
