//! **The caret is writing an annotation** — and what that annotation would be attached to.
//!
//! Two questions with one answer, because they are asked at the same instant and by the same
//! caller: a completion popup that has just seen an `@`.
//!
//! ## Why the `@` is worth reading at all
//!
//! Typing `@` narrows the set of legal names from *every type on the classpath* to *the annotation
//! types on it* — from tens of thousands to a few hundred. Nothing else in Java narrows that hard
//! from a single character. Offering the unnarrowed list there is offering, at the one moment the
//! answer is nearly knowable, a list in which the answer is buried.
//!
//! And of those few hundred, most are illegal where the caret is: an annotation declares what it
//! may be attached to (`@Target`), so above a field the ones that only go on a method are not
//! candidates at all. That is what [`ElementTarget`] is for.
//!
//! ## Text and not the tree
//!
//! Deliberately. The buffer at this instant is `@Ser` with **nothing after it yet** — half a
//! modifier list, no declaration, and a parse that recovers by inventing something. Reading the
//! tree there means reading a guess about a guess. What follows the caret, on the other hand, is
//! whatever the user is annotating: it is already written, because you annotate something that
//! exists.
//!
//! Every step below is therefore about being *sure* rather than about being clever, and
//! [`ElementTarget::Unknown`] is a first-class answer. It means "offer everything", which is what
//! this module owed the caller before it existed anyway.

/// What an annotation at the caret would be attached to — Java's `ElementType`, reduced to the
/// cases a caret can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementTarget {
    /// A class / interface / enum / record / annotation declaration.
    Type,
    /// A method or a constructor.
    Method,
    /// A field of a type.
    Field,
    /// A method or constructor parameter.
    Parameter,
    /// A local variable inside a body.
    LocalVariable,
    /// A `package-info.java`'s package declaration.
    Package,
    /// Not determinable from what is written — a caret in an empty class body, a file being
    /// typed top-down. Admits everything, which is the honest answer and not a failure.
    Unknown,
}

impl ElementTarget {
    /// The `java.lang.annotation.ElementType` constant this is, for matching a `@Target`.
    fn element_type(self) -> Option<&'static str> {
        match self {
            ElementTarget::Type => Some("TYPE"),
            ElementTarget::Method => Some("METHOD"),
            ElementTarget::Field => Some("FIELD"),
            ElementTarget::Parameter => Some("PARAMETER"),
            ElementTarget::LocalVariable => Some("LOCAL_VARIABLE"),
            ElementTarget::Package => Some("PACKAGE"),
            ElementTarget::Unknown => None,
        }
    }
}

/// An annotation being written at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotationSite {
    /// Byte offset of the `@`.
    pub at: usize,
    /// Byte offset where the name begins — one past the `@`, and where a replacement starts.
    pub name_start: usize,
    /// What has been typed of the name so far. Empty right after the `@`.
    pub prefix: String,
    /// What the annotation would be attached to.
    pub target: ElementTarget,
}

/// The annotation being written at `offset`, or `None` when the caret is not in one.
///
/// The test is narrow on purpose: the identifier under the caret must be preceded **immediately**
/// by an `@`, with no space, because that is Java's own rule — `@ Override` is not an annotation.
/// A caret inside an annotation's *arguments* is not here either: `@Column(name = |)` is a value
/// being written, and the vocabulary there is not annotation names.
pub fn annotation_site(source: &str, offset: usize) -> Option<AnnotationSite> {
    if !source.is_char_boundary(offset) || offset > source.len() {
        return None;
    }
    // Walk back over what has been typed of the name. Java identifiers only, so the scan stops at
    // the `@` rather than running through it into the previous token.
    let bytes = source.as_bytes();
    let mut name_start = offset;
    while name_start > 0 {
        let b = bytes[name_start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' {
            name_start -= 1;
        } else {
            break;
        }
    }
    if name_start == 0 || bytes[name_start - 1] != b'@' {
        return None;
    }
    let at = name_start - 1;
    // `a@b` is not an annotation, and neither is an email address in a comment. What may precede
    // an `@` is whitespace, an opening delimiter, or nothing at all.
    if at > 0 {
        let before = bytes[at - 1];
        if !before.is_ascii_whitespace() && !matches!(before, b'(' | b'{' | b'}' | b';' | b',') {
            return None;
        }
    }
    Some(AnnotationSite {
        at,
        name_start,
        prefix: source[name_start..offset].to_string(),
        target: target_after(source, offset),
    })
}

/// The modifiers that may sit between an annotation and the thing it annotates.
const MODIFIERS: [&str; 12] = [
    "public", "protected", "private", "static", "final", "abstract", "synchronized", "native",
    "transient", "volatile", "default", "strictfp",
];

/// What follows the caret, read as the declaration the annotation is on.
///
/// Skips whatever may legally stand between: whitespace, further annotations, and modifiers. Then
/// the first word decides — and when it does not, the shape of the rest of the line does.
fn target_after(source: &str, offset: usize) -> ElementTarget {
    let mut rest = &source[offset..];
    loop {
        rest = rest.trim_start();
        // Another annotation on the same declaration. Its arguments are skipped as a balanced
        // group, so `@Column(name = "class")` cannot be read as a class declaration.
        if let Some(after) = rest.strip_prefix('@') {
            let after = after.trim_start();
            let end = after
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$' || c == '.'))
                .unwrap_or(after.len());
            rest = &after[end..];
            rest = rest.trim_start();
            if rest.starts_with('(') {
                let Some(close) = balanced_end(rest) else { return ElementTarget::Unknown };
                rest = &rest[close..];
            }
            continue;
        }
        let word = leading_word(rest);
        if word.is_empty() {
            return ElementTarget::Unknown;
        }
        if MODIFIERS.contains(&word) {
            rest = &rest[word.len()..];
            continue;
        }
        return match word {
            "class" | "interface" | "enum" | "record" => ElementTarget::Type,
            "package" => ElementTarget::Package,
            // Anything else is a type name, so what is being declared is a method, a field, a
            // parameter or a local — told apart by what comes after the name it declares.
            _ => declarator_shape(rest),
        };
    }
}

/// A `Type name …` — which of the four value declarations it is.
///
/// `(` before anything else is a method. Otherwise the terminator says it: a `;` or an `=` ends a
/// field or a local, and a `,` or a `)` ends a parameter, because those are the only things that
/// can follow a name inside a parameter list.
///
/// Field or local is a distinction this cannot make from what follows — both are `Type name;` —
/// and it does not try. It reports [`ElementTarget::Field`], which is the far commoner place to
/// find an annotation, and the cost of being wrong is a `@Target(LOCAL_VARIABLE)`-only annotation
/// ranked lower than it deserved rather than a wrong list.
fn declarator_shape(rest: &str) -> ElementTarget {
    for c in rest.chars().take(400) {
        match c {
            '(' => return ElementTarget::Method,
            ';' | '=' => return ElementTarget::Field,
            ',' | ')' => return ElementTarget::Parameter,
            '{' | '}' => return ElementTarget::Unknown,
            _ => {}
        }
    }
    ElementTarget::Unknown
}

/// The leading identifier word of `s`, empty when it does not start with one.
fn leading_word(s: &str) -> &str {
    let end = s
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .unwrap_or(s.len());
    &s[..end]
}

/// One past the `)` that closes the `(` at the start of `s`, or `None` when it is unbalanced.
fn balanced_end(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether an annotation whose own `@Target` reads `target_text` may be attached to `where_`.
///
/// `target_text` is the raw source of the `@Target(...)` argument — `{ElementType.METHOD,
/// ElementType.FIELD}`, or `ElementType.TYPE`, or `TYPE` when the constants were statically
/// imported. Matched on the constant NAMES rather than parsed, because all three spellings are
/// common and the names are unambiguous.
///
/// **An empty `target_text` admits everything.** An annotation with no `@Target` really does apply
/// almost anywhere in Java — and, more to the point here, "no `@Target`" and "the resolver did not
/// read one" are the same empty string, and treating the second as a refusal would silently hide
/// every annotation from a jar whose annotations were not decoded.
pub fn target_admits(target_text: &str, where_: ElementTarget) -> bool {
    let Some(wanted) = where_.element_type() else { return true };
    if target_text.trim().is_empty() {
        return true;
    }
    // `TYPE_USE` admits an annotation anywhere a type is written, which includes every declaration
    // whose type is spelled out — so it is treated as admitting all of these.
    if contains_constant(target_text, "TYPE_USE") {
        return true;
    }
    if contains_constant(target_text, wanted) {
        return true;
    }
    // A constructor is a method as far as a caret can tell, and `@Target(CONSTRUCTOR)` is common.
    where_ == ElementTarget::Method && contains_constant(target_text, "CONSTRUCTOR")
}

/// Whether `text` names the constant `name` — as a whole word, so `TYPE` does not match inside
/// `TYPE_USE` or `TYPE_PARAMETER`.
fn contains_constant(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(i) = text[from..].find(name) {
        let start = from + i;
        let end = start + name.len();
        let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
        let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(src: &str) -> Option<AnnotationSite> {
        let offset = src.find('|').expect("mark the caret with |");
        let text = src.replace('|', "");
        annotation_site(&text, offset)
    }

    fn target(src: &str) -> ElementTarget {
        site(src).expect("an annotation site").target
    }

    #[test]
    fn the_prefix_is_what_has_been_typed_after_the_at() {
        let s = site("class A {\n    @Ove|\n    void m() {}\n}").expect("a site");
        assert_eq!(s.prefix, "Ove");
        assert_eq!(s.name_start, s.at + 1);
    }

    #[test]
    fn a_bare_at_is_a_site_with_nothing_typed() {
        assert_eq!(site("class A {\n    @|\n    void m() {}\n}").unwrap().prefix, "");
    }

    #[test]
    fn a_plain_identifier_is_not_one() {
        assert!(site("class A { Ove| }").is_none());
    }

    #[test]
    fn a_space_after_the_at_ends_it() {
        // `@ Override` is not an annotation in Java, and offering annotations there would be
        // offering something that cannot compile.
        assert!(site("class A {\n    @ Ove|\n}").is_none());
    }

    #[test]
    fn an_at_in_the_middle_of_a_word_is_not_one() {
        assert!(site("String s = \"me@exa|\";").is_none());
    }

    #[test]
    fn what_it_is_attached_to_is_read_from_what_follows() {
        assert_eq!(target("@Ent|\npublic class Order {}"), ElementTarget::Type);
        assert_eq!(target("class A {\n  @Ov|\n  public void run() {}\n}"), ElementTarget::Method);
        assert_eq!(target("class A {\n  @Au|\n  private Repo repo;\n}"), ElementTarget::Field);
        assert_eq!(target("void m(@No| String name) {}"), ElementTarget::Parameter);
        assert_eq!(target("@Non|\npackage app;"), ElementTarget::Package);
    }

    #[test]
    fn an_interface_and_a_record_are_types_too() {
        assert_eq!(target("@Fun|\npublic interface Job {}"), ElementTarget::Type);
        assert_eq!(target("@Ann|\nrecord Point(int x) {}"), ElementTarget::Type);
        assert_eq!(target("@Ann|\nenum Colour { RED }"), ElementTarget::Type);
    }

    #[test]
    fn modifiers_between_the_annotation_and_the_declaration_are_skipped() {
        assert_eq!(
            target("class A {\n  @Tx|\n  public static final synchronized void go() {}\n}"),
            ElementTarget::Method
        );
    }

    #[test]
    fn another_annotation_in_between_is_skipped_arguments_and_all() {
        // The argument had to be skipped as a group: `"class"` inside it would otherwise read as a
        // class declaration and put every type annotation at the top of the list.
        assert_eq!(
            target("@Ne|\n@Column(name = \"class\")\nprivate String kind;"),
            ElementTarget::Field
        );
    }

    #[test]
    fn nothing_written_yet_is_unknown_rather_than_a_guess() {
        assert_eq!(target("class A {\n    @Ov|\n}"), ElementTarget::Unknown);
        assert_eq!(target("@Ov|"), ElementTarget::Unknown);
    }

    // ── @Target ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn a_target_list_is_matched_however_the_constants_are_spelled() {
        let braced = "{ElementType.METHOD, ElementType.FIELD}";
        assert!(target_admits(braced, ElementTarget::Method));
        assert!(target_admits(braced, ElementTarget::Field));
        assert!(!target_admits(braced, ElementTarget::Type));
        assert!(target_admits("ElementType.TYPE", ElementTarget::Type));
        assert!(target_admits("METHOD", ElementTarget::Method));
    }

    #[test]
    fn type_does_not_match_inside_type_use_or_type_parameter() {
        assert!(!target_admits("{ElementType.TYPE_PARAMETER}", ElementTarget::Type));
    }

    #[test]
    fn type_use_goes_anywhere_a_type_is_written() {
        assert!(target_admits("ElementType.TYPE_USE", ElementTarget::Field));
        assert!(target_admits("ElementType.TYPE_USE", ElementTarget::Parameter));
    }

    #[test]
    fn a_constructor_target_admits_a_caret_that_reads_as_a_method() {
        // A caret cannot tell `public Order(` from `public void run(` before the name is read, and
        // refusing `@Inject` above a constructor would be refusing its commonest use.
        assert!(target_admits("{ElementType.CONSTRUCTOR}", ElementTarget::Method));
    }

    #[test]
    fn no_target_read_admits_everything() {
        // "no `@Target`" and "the resolver did not read one" are the same empty string. Reading the
        // second as a refusal would hide every annotation from a jar whose bytes were not decoded.
        assert!(target_admits("", ElementTarget::Field));
        assert!(target_admits("   ", ElementTarget::Type));
    }

    #[test]
    fn an_unknown_position_admits_everything() {
        assert!(target_admits("{ElementType.METHOD}", ElementTarget::Unknown));
    }
}
