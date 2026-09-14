//! Test fixtures: a project built from literal sources.

use std::path::{Path, PathBuf};

use bennu_ext::prelude::{ProjectScan, ScannedFile};

use crate::inject::InjectionPoint;
use crate::matching::{candidates, resolution, Candidate, Resolution};
use crate::model::Model;
use crate::types::TypeView;

/// The Jakarta imports a CDI file carries, on ONE line so splicing them never moves a line number.
/// No `jakarta.ejb.*`: with `jakarta.inject.*` beside it, `@Singleton` would be both.
pub const IMPORTS: &str = " import jakarta.enterprise.context.*; import jakarta.enterprise.inject.*; import jakarta.inject.*; import jakarta.servlet.annotation.*;";

pub fn scanned(path: &str, text: &str) -> ScannedFile {
    ScannedFile { path: PathBuf::from(path), text: text.to_string() }
}

/// [`IMPORTS`] spliced onto the `package` line.
pub fn with_imports(src: &str) -> String {
    match src.find(';') {
        Some(semi) if src.starts_with("package") => format!("{}{IMPORTS}{}", &src[..=semi], &src[semi + 1..]),
        _ => format!("{IMPORTS}\n{src}"),
    }
}

pub struct Project {
    pub model: Model,
    pub files: Vec<(String, String)>,
}

impl Project {
    /// Sources as written.
    pub fn new(java: &[(&str, &str)]) -> Self {
        Self::with_xml(java, &[])
    }

    /// Sources with the CDI imports spliced in.
    pub fn cdi(java: &[(&str, &str)]) -> Self {
        Self::cdi_with_xml(java, &[])
    }

    pub fn cdi_with_xml(java: &[(&str, &str)], xml: &[(&str, &str)]) -> Self {
        let spliced: Vec<(String, String)> = java.iter().map(|(p, s)| (p.to_string(), with_imports(s))).collect();
        let refs: Vec<(&str, &str)> = spliced.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect();
        Self::with_xml(&refs, xml)
    }

    pub fn with_xml(java: &[(&str, &str)], xml: &[(&str, &str)]) -> Self {
        let java_files: Vec<ScannedFile> = java.iter().map(|(p, s)| scanned(p, s)).collect();
        let xml_files: Vec<ScannedFile> = xml.iter().map(|(p, s)| scanned(p, s)).collect();
        let scan = ProjectScan { java: &java_files, xml: &xml_files, ..ProjectScan::empty(Path::new("/p")) };
        Self {
            model: Model::build(&scan),
            files: java.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect(),
        }
    }

    /// A file's text, as the model read it.
    pub fn text(&self, path: &str) -> &str {
        self.files.iter().find(|(p, _)| p == path).map(|(_, s)| s.as_str()).unwrap_or_else(|| panic!("no file {path}"))
    }

    pub fn point(&self, index: usize) -> &InjectionPoint {
        &self.model.points[index]
    }

    pub fn candidates_of(&self, index: usize) -> Vec<Candidate<'_>> {
        let view = TypeView::of(&self.model.table);
        candidates(&view, &self.model.beans, self.point(index), self.model.gates().mode)
    }

    pub fn resolve_point(&self, index: usize) -> Resolution {
        let view = TypeView::of(&self.model.table);
        resolution(&view, &self.model.beans, self.point(index), self.model.gates())
    }
}
