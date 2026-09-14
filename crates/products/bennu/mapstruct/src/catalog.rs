//! The Mappers panel: one row per mapper, one child per mapping method.
//!
//! The unmapped properties ride along as tags, because the panel is where somebody goes to ask
//! "which of our mappers silently drop a field" — a question no single file answers.

use bennu_ext::prelude::ExtEntry;

use crate::checks::unmapped;
use crate::mapper::{simple_type, Mapper, MappingMethod};
use crate::table::TypeTable;

const UNMAPPED_TAG: &str = "unmapped: ";

pub fn rows(mappers: &[Mapper], table: &TypeTable, file: &str, text: &str) -> Vec<ExtEntry> {
    mappers.iter().map(|m| row(m, table, file, text)).collect()
}

fn row(mapper: &Mapper, table: &TypeTable, file: &str, text: &str) -> ExtEntry {
    let children: Vec<ExtEntry> = mapper.methods.iter().map(|m| child(mapper, m, table, file, text)).collect();
    let unmapped_total: usize =
        children.iter().map(|c| c.tags.iter().filter(|t| t.starts_with(UNMAPPED_TAG)).count()).sum();
    let mut tags = Vec::new();
    if let Some(model) = &mapper.component_model {
        tags.push(model.clone());
    }
    if unmapped_total > 0 {
        tags.push(format!("{unmapped_total} unmapped"));
    }
    ExtEntry {
        id: mapper.fqcn.clone(),
        primary: mapper.name.clone(),
        secondary: mapper.fqcn.clone(),
        kind: "@Mapper".to_string(),
        file: Some(file.to_string()),
        offset: Some(mapper.name_offset),
        line: Some(line_of(text, mapper.name_offset)),
        tags,
        children,
    }
}

fn child(mapper: &Mapper, method: &MappingMethod, table: &TypeTable, file: &str, text: &str) -> ExtEntry {
    let sources: Vec<String> = method.sources.iter().map(|s| simple_type(&s.type_text)).collect();
    ExtEntry {
        id: format!("{}#{}", mapper.fqcn, method.name),
        primary: format!("{}({})", method.name, method.param_types.join(", ")),
        secondary: format!("{} → {}", sources.join(", "), simple_type(&method.target_text)),
        kind: method.kind.label().to_string(),
        file: Some(file.to_string()),
        offset: Some(method.name_offset),
        line: Some(line_of(text, method.name_offset)),
        tags: unmapped(method, table)
            .unwrap_or_default()
            .into_iter()
            .map(|p| format!("{UNMAPPED_TAG}{p}"))
            .collect(),
        children: Vec::new(),
    }
}

/// 1-based line of a byte offset.
pub fn line_of(text: &str, offset: usize) -> u32 {
    text.as_bytes()[..offset.min(text.len())].iter().filter(|&&b| b == b'\n').count() as u32 + 1
}
