//! `completion picked` domain — `bennu_completion_accepted`.
//!
//! One verb, and the whole of the completion memory's input: the editor says which candidate was
//! accepted, and the next list in the same place offers it first. See `bennu-query`'s `picked`
//! module for what is remembered and why it is not persisted.
//!
//! Fire-and-forget by design. A ranking hint that fails is a list in a slightly worse order, and
//! nothing about accepting a completion should be able to fail visibly because of it.

use bennu_core::prelude::BennuState;
use serde::Deserialize;

/// Args for [`bennu_completion_accepted`].
#[derive(Deserialize)]
pub struct AcceptedArgs {
    /// The `owner` the accepted item carried — the binary name of the type that declares it. The
    /// memory is keyed by it, so what is learned is "reaching into this type, you pick this".
    /// Absent for a candidate that has no owner (a local, a keyword), which is remembered under
    /// the empty context rather than not at all.
    #[serde(default)]
    pub owner: Option<String>,
    /// The label that was accepted.
    pub label: String,
    /// The item's kind slug, when the editor knows it.
    ///
    /// A member is remembered under the type that declares it, which is the context the next list
    /// in the same place is built from. A **type name** has no such context: its `owner` is the
    /// type itself, so filing it there writes a fact nobody ever reads back. Those are remembered
    /// under the kind of hole they came out of — `bennu_query`'s `ANNOTATION_CONTEXT` for an `@`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Where the list was asked, when the editor says: the file, the buffer the list was built from and the byte offset
    /// where the replaced word starts. With them the pick is also counted per receiver type or argument slot and kept
    /// on disk (`flc_choices`); without them only the session memory learns it.
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// The parameter list the accepted method row showed (`(int, int)`, `(String name, List<Item> items)`): which
    /// overload was chosen, for the argument list drawn right after it.
    #[serde(default)]
    pub signature: Option<String>,
}

/// The overload the user last chose in the completion list.
struct PickedOverload {
    file: String,
    method: String,
    params: Vec<String>,
    at: std::time::Instant,
}

/// A choice older than this is forgotten: the call it was for has been written or abandoned.
const PICK_TTL: std::time::Duration = std::time::Duration::from_secs(300);

fn last_overload() -> &'static std::sync::Mutex<Option<PickedOverload>> {
    static LAST: std::sync::OnceLock<std::sync::Mutex<Option<PickedOverload>>> = std::sync::OnceLock::new();
    LAST.get_or_init(|| std::sync::Mutex::new(None))
}

/// The simple type name of each parameter in a row's signature: `(String name, Map<K, V> m, int... xs)` →
/// `["String", "Map", "int[]"]`. Names are optional (`(long, int)`).
pub(crate) fn signature_types(signature: &str) -> Vec<String> {
    let inner = signature.trim().trim_start_matches('(').trim_end_matches(')');
    let (mut parts, mut depth, mut start) = (Vec::new(), 0i32, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            // the type is everything before the parameter's name; a name is the last word when there are two
            let mut ty = p;
            let mut depth = 0i32;
            for (i, c) in p.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    c if c.is_whitespace() && depth == 0 => ty = &p[..i],
                    _ => {}
                }
                if depth == 0 && c.is_whitespace() {
                    break;
                }
            }
            let (base, varargs) = ty.strip_suffix("...").map_or((ty, false), |b| (b, true));
            let generic_at = base.find('<').unwrap_or(base.len());
            let head = &base[..generic_at];
            let tail = &base[base.rfind('>').map_or(generic_at, |g| g + 1)..];
            let simple = head.rsplit('.').next().unwrap_or(head);
            format!("{simple}{tail}{}", if varargs { "[]" } else { "" })
        })
        .collect()
}

/// The parameter types of the overload chosen for `method` in `file`, when that was a moment ago.
pub(crate) fn picked_overload(file: &str, method: &str) -> Option<Vec<String>> {
    let guard = last_overload().lock().unwrap_or_else(|p| p.into_inner());
    let last = guard.as_ref()?;
    (last.file == file && last.method == method && last.at.elapsed() < PICK_TTL && !last.params.is_empty()).then(|| last.params.clone())
}

/// Record an accepted completion. Always `Ok(())` — see the module doc.
#[arbor_rpc::handler]
fn bennu_completion_accepted(_ctx: &BennuState, args: AcceptedArgs) -> Result<(), String> {
    let context = match args.kind.as_deref() {
        Some("annotation") => bennu_query::prelude::ANNOTATION_CONTEXT,
        _ => args.owner.as_deref().unwrap_or(""),
    };
    bennu_query::prelude::record_pick(context, &args.label);
    if let (Some(file), Some(signature)) = (&args.file, &args.signature) {
        *last_overload().lock().unwrap_or_else(|p| p.into_inner()) =
            Some(PickedOverload { file: file.clone(), method: args.label.clone(), params: signature_types(signature), at: std::time::Instant::now() });
    }
    if let (Some(file), Some(source), Some(offset)) = (&args.file, &args.source, args.offset) {
        crate::flc_choices::record_pick(file, source, offset, &args.label);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_gives_the_simple_type_of_each_parameter() {
        assert_eq!(signature_types("(long, int)"), ["long", "int"]);
        assert_eq!(signature_types("(String name, Map<K, V> m, int... xs)"), ["String", "Map", "int[]"]);
        assert_eq!(signature_types("(java.util.List<String> items, byte[] data)"), ["List", "byte[]"]);
        assert!(signature_types("()").is_empty());
    }
}
