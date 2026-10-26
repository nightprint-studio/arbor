//! `implementations` domain — `bennu_implementations` ("go to implementation").
//!
//! What implements the interface / abstract type under the caret, or overrides the method under
//! it. A language with a server is answered by its server; Java by Bennu's own engine over the
//! project's subtype map — the same one the type hierarchy descends, so the two never disagree
//! about what an implementor is.
//!
//! On a **method**, the answer also carries the MyBatis `<select|insert|update|delete>` statement
//! the method is implemented by when its owner (or one of its implementors) is a mapper
//! interface: in a mapper the XML *is* the implementation, and it is the one a Java-side search
//! can never find.
//!
//! Returns the same wire shape as find-usages so the editor's results popover renders it as is.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{DeclKey, HierarchyHandle, HierarchyItem};
use bennu_proto::prelude::{UsageHit, UsagesResult};
use serde::Deserialize;

use crate::index_service::IndexService;
use crate::mybatis_nav::mapper_statement;

/// Args for [`bennu_implementations`].
#[derive(Deserialize)]
pub struct ImplementationsArgs {
    /// Absolute path (forward slashes) of the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer — the caret is classified against it.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// The implementations of the symbol at `file`:`offset`. `None` when no engine owns the file, the
/// index is still building, or the caret isn't on a type or method; an empty list when it is but
/// nothing builds on it.
#[arbor_rpc::handler]
fn bennu_implementations(
    _ctx: &BennuState,
    args: ImplementationsArgs,
) -> Result<Option<UsagesResult>, String> {
    if let Some(found) = crate::lsp_route::implementations(&args.file, &args.source, args.offset) {
        return Ok(Some(found));
    }
    if !crate::intel::is_java_file(&args.file) {
        return Ok(None);
    }
    let Some((key, items)) =
        IndexService::global().implementations(&args.file, &args.source, args.offset)
    else {
        return Ok(None);
    };

    let mut usages: Vec<UsageHit> = Vec::new();
    if let DeclKey::Method { owner, name } = &key {
        usages.extend(statement_hits(&args.file, owner, name, &items));
    }
    usages.extend(items.into_iter().filter_map(usage_of));
    Ok(Some(UsagesResult { target_label: format!("implementations of {}", key.label()), usages }))
}

/// The mapper statements implementing method `name`: one per mapper interface among the method's
/// owner and its implementors (a shared base mapper's method is implemented by each sub-mapper's
/// own XML).
fn statement_hits(file: &str, owner: &str, name: &str, items: &[HierarchyItem]) -> Vec<UsageHit> {
    let implementors = items.iter().filter_map(|i| match &i.handle {
        HierarchyHandle::Method { owner, .. } => Some(owner.as_str()),
        HierarchyHandle::Type { .. } => None,
    });
    std::iter::once(owner)
        .chain(implementors)
        .filter_map(|o| mapper_statement(file, o, name))
        .filter_map(|d| statement_hit(&d.config_file, d.offset, name))
        .collect()
}

/// A hit at `offset` in `file` (a mapper XML), with the line it sits on read from disk. `None`
/// when the file can't be read — a hit that cannot be previewed is better dropped than shown
/// blank.
fn statement_hit(file: &str, offset: usize, id: &str) -> Option<UsageHit> {
    let text = std::fs::read_to_string(file).ok()?;
    let head = text.get(..offset)?;
    let line_start = head.rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[line_start..].find('\n').map_or(text.len(), |i| line_start + i);
    Some(UsageHit {
        file: file.to_string(),
        start: offset,
        end: offset + id.len(),
        line: head.bytes().filter(|b| *b == b'\n').count() + 1,
        col: offset - line_start + 1,
        preview: text[line_start..line_end].trim().to_string(),
        via: None,
    })
}

/// An implementor as a result row. A library type has no file to open, so it has no row.
fn usage_of(item: HierarchyItem) -> Option<UsageHit> {
    if item.file.is_empty() {
        return None;
    }
    Some(UsageHit {
        file: item.file,
        start: item.start,
        end: item.end,
        line: item.line,
        col: item.col,
        preview: item.preview,
        via: None,
    })
}
