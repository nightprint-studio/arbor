//! Hover and go-to for a pom.
//!
//! ## The question both answer
//!
//! *"Where does this come from?"* — which for a pom is rarely the file in front of you. The version
//! is in a parent three directories up, or in a BOM the parent imports, or in a property defined in
//! neither; the artifact is a jar in `~/.m2` or a module of this same reactor. All of it is written
//! down somewhere, and none of it is written down **here**, which is why reading a pom means
//! opening four files.
//!
//! So the hover card says where each answer came from, and every one of those origins is a place
//! the editor can go: the parent's `<dependencyManagement>` entry, the `<properties>` line that
//! defines the placeholder, the module's own pom, the artifact's `.pom` in the repository. A pom in
//! the local repository is a real file — reading one is how you find out what a library drags in —
//! so jumping into it is a jump like any other.

use bennu_ext::prelude::{ExtHover, ExtTarget};

use crate::blocks::{block_at, BlockKind};
use crate::doc::Doc;
use crate::env::{property_references, PomEnv};
use crate::known;
use crate::repo::Coord;

/// The card for the caret, when there is something to say.
pub fn hover(env: &PomEnv<'_>, doc: &Doc<'_>, offset: usize) -> Option<ExtHover> {
    if let Some(card) = property_hover(env, doc, offset) {
        return Some(card);
    }
    let (block, leaf) = block_at(doc, offset)?;
    if !matches!(doc.name(leaf), "groupId" | "artifactId" | "version" | "scope") {
        return None;
    }
    let coord = env.resolve_coord(&block.raw_coord());
    if coord.artifact_id.is_empty() {
        return None;
    }

    let signature = match env.reactor_pom(&coord) {
        Some(_) => "a module of this project — built from source".to_string(),
        None => match env.repo.resolve(&coord) {
            Some(path) => path.to_string_lossy().replace('\\', "/"),
            None if coord.version.is_empty() => "no version — nothing supplies one".to_string(),
            None => "not in the local repository".to_string(),
        },
    };

    let mut lines: Vec<String> = Vec::new();
    if let Some(doc_line) = known::describe(&coord.group_id, &coord.artifact_id) {
        lines.push(doc_line.to_string());
    }
    if block.version.text.is_empty() {
        if let Some(pin) = env.managed(&Coord { version: String::new(), ..coord.clone() }) {
            lines.push(format!("Version `{}` managed by `{}`.", pin.version, pin.from));
        }
    } else if block.version.text.contains("${") {
        lines.push(format!("`{}` expands to `{}`.", block.version.text, coord.version));
    }
    let scope = block.scope.text.trim();
    if !scope.is_empty() {
        lines.push(format!("Scope `{scope}`."));
    }
    if !block.profile.is_empty() {
        lines.push(format!("Only under profile `{}`.", block.profile));
    }
    let installed = env.repo.versions(&coord.group_id, &coord.artifact_id);
    if !installed.is_empty() {
        lines.push(format!(
            "Installed locally: {}{}",
            installed.iter().take(6).cloned().collect::<Vec<_>>().join(", "),
            if installed.len() > 6 { format!(", +{} more", installed.len() - 6) } else { String::new() }
        ));
    }
    lines.push(format!("Declared as a {}.", block.kind.label()));

    Some(ExtHover { title: coord.gav(), signature, doc: lines.join("\n") })
}

/// The card for a `${property}` under the caret: what it expands to, and which pom said so.
fn property_hover(env: &PomEnv<'_>, doc: &Doc<'_>, offset: usize) -> Option<ExtHover> {
    let (name, _) = property_at(doc, offset)?;
    let value = env.effective.properties.get(&name)?;
    let site = env
        .effective
        .property_sites
        .get(&name)
        .map(|p| format!("Defined in {}", short(p)))
        .unwrap_or_default();
    Some(ExtHover { title: format!("${{{name}}}"), signature: value.clone(), doc: site })
}

/// Everywhere the caret could be taken.
pub fn navigate(env: &PomEnv<'_>, doc: &Doc<'_>, offset: usize) -> Vec<ExtTarget> {
    let mut out: Vec<ExtTarget> = Vec::new();

    // A `${property}` goes to the pom that defines it, at the line that does.
    if let Some((name, _)) = property_at(doc, offset) {
        if let Some(path) = env.effective.property_sites.get(&name) {
            let at = property_site(path, &name).unwrap_or(0);
            out.push(ExtTarget {
                file: path.clone(),
                offset: at,
                label: format!("${{{name}}}"),
                detail: format!("defined in {}", short(path)),
            });
            return out;
        }
    }

    let path = doc.path_at(offset);
    let Some(&leaf) = path.last() else { return out };

    // A `<module>` goes to the module.
    if doc.name(leaf) == "module" {
        let name = doc.text(leaf);
        let pom = std::path::PathBuf::from(env.dir()).join(name.trim_end_matches('/')).join("pom.xml");
        if pom.is_file() {
            out.push(ExtTarget {
                file: pom.to_string_lossy().replace('\\', "/"),
                offset: 0,
                label: name.to_string(),
                detail: "module".to_string(),
            });
        }
        return out;
    }

    let Some((block, _)) = block_at(doc, offset) else { return out };
    if !matches!(doc.name(leaf), "groupId" | "artifactId" | "version") {
        return out;
    }
    let coord = env.resolve_coord(&block.raw_coord());
    if coord.artifact_id.is_empty() {
        return out;
    }

    // The project's own module first — it is the one with sources.
    if let Some(pom) = env.reactor_pom(&coord) {
        out.push(ExtTarget {
            file: pom.to_string(),
            offset: 0,
            label: coord.ga(),
            detail: "a module of this project".to_string(),
        });
    }

    // Where the version comes from, when it is not written here.
    if block.version.text.is_empty() || doc.name(leaf) == "version" {
        let key = Coord { version: String::new(), ..coord.clone() };
        if let Some(pin) = env.managed(&key) {
            if !pin.from_path.is_empty() && pin.from_path != env.path {
                let at = managed_site(&pin.from_path, &key).unwrap_or(0);
                out.push(ExtTarget {
                    file: pin.from_path.clone(),
                    offset: at,
                    label: format!("managed by {}", pin.from),
                    detail: format!("version {} · {}", pin.version, short(&pin.from_path)),
                });
            }
        }
    }

    // And the artifact itself. The jar is not a text file; the `.pom` beside it is, and it is the
    // one that says what this dependency drags in — which is why anybody follows the link.
    let pom = env.repo.pom_file(&coord);
    if pom.is_file() {
        out.push(ExtTarget {
            file: pom.to_string_lossy().replace('\\', "/"),
            offset: 0,
            label: format!("{}.pom", coord.artifact_id),
            detail: "in the local repository".to_string(),
        });
    }
    out
}

/// The property name under the caret, with its span, when the caret is inside a `${…}`.
fn property_at(doc: &Doc<'_>, offset: usize) -> Option<(String, (usize, usize))> {
    let leaf = *doc.path_at(offset).last()?;
    let (start, end) = doc.text_span(leaf)?;
    if offset < start || offset > end {
        return None;
    }
    let text = &doc.source[start..end];
    property_references(text)
        .into_iter()
        .find(|(_, from, to)| offset >= start + from && offset <= start + to)
        .map(|(name, from, to)| (name, (start + from, start + to)))
}

/// The offset of the `<dependency>` inside another pom's `<dependencyManagement>` that pins this
/// coordinate. Zero (the top of the file) when the entry cannot be located — a jump to the right
/// file is still most of the answer.
fn managed_site(path: &str, coord: &Coord) -> Option<usize> {
    let text = read(path)?;
    let doc = Doc::new(&text);
    for block in crate::blocks::blocks(&doc) {
        if block.kind != BlockKind::Managed {
            continue;
        }
        let written = block.raw_coord();
        if written.artifact_id == coord.artifact_id && written.group_id == coord.group_id {
            return Some(block.tag.0);
        }
    }
    None
}

/// The offset of a `<properties>` entry in another pom.
fn property_site(path: &str, name: &str) -> Option<usize> {
    let text = read(path)?;
    let doc = Doc::new(&text);
    let project = doc.root()?;
    let properties = doc.child(project, "properties")?;
    doc.children(properties).into_iter().find(|c| doc.name(*c) == name).map(|c| doc.start(c))
}

fn read(path: &str) -> Option<String> {
    std::fs::read(path).ok().map(|b| String::from_utf8_lossy(&b).to_string())
}

/// A path as a person reads it in a card: the last two segments, which is enough to tell a parent
/// from a repository pom without a line of absolute path.
fn short(path: &str) -> String {
    let parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_reads_as_its_last_two_segments() {
        assert_eq!(short("/a/b/c/pom.xml"), "c/pom.xml");
        assert_eq!(short("pom.xml"), "pom.xml");
    }
}
