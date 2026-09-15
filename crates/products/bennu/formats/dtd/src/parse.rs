//! Reading a DTD.
//!
//! ## Shape of the parser
//!
//! One scan for markup declarations (`<!…>`), skipping comments and processing instructions,
//! then one small parser per declaration form. There is no tokenizer: a DTD's lexical structure
//! is shallow enough that finding the extent of a declaration and then reading inside it is both
//! shorter and easier to keep span-accurate than a token stream would be.
//!
//! ## Parameter entities are expanded first
//!
//! Not an optimisation — a correctness requirement. A real-world DTD is written almost entirely
//! in parameter entities:
//!
//! ```text
//! <!ENTITY % common "id ID #IMPLIED  name CDATA #IMPLIED">
//! <!ATTLIST action %common; class CDATA #IMPLIED>
//! ```
//!
//! A parser that skips expansion sees an `<!ATTLIST>` with one attribute and reports the other
//! two as unknown — which is worse than not parsing the file at all, because it is confidently
//! wrong. Expansion runs to a fixed point with a bounded number of rounds, so a self-referential
//! entity stops rather than hanging.
//!
//! ## Offsets after expansion
//!
//! Expansion rewrites the text, so an offset into the expanded copy is not an offset into the
//! file. Declarations therefore carry the offset of the **original** text where one can be
//! recovered, and 0 where it cannot — a go-to that lands at the top of the right file is a fair
//! answer; one that lands in the middle of an unrelated declaration is not.
//!
//! In practice the common case recovers exactly: the `<!ELEMENT` / `<!ATTLIST` keyword and the
//! name that follows it are almost never themselves written as an entity, so the declaration is
//! found in the original by searching for that name.

use crate::model::*;

/// Parse a DTD. Never fails: a malformed declaration is skipped rather than aborting the file,
/// because a DTD is usually being read *because* an editor is open on something and half a
/// grammar is worth more than none.
pub fn parse(source: &str) -> Dtd {
    let entities = collect_entities(source);
    let expanded = expand(source, &entities);

    let mut dtd = Dtd { entities, ..Dtd::default() };
    for (decl, at) in declarations(&expanded) {
        let body = decl.trim();
        if let Some(rest) = keyword(body, "ELEMENT") {
            if let Some(e) = element_decl(rest, source, &expanded, at) {
                dtd.elements.push(e);
            }
        } else if let Some(rest) = keyword(body, "ATTLIST") {
            if let Some(a) = attlist_decl(rest, source, &expanded, at) {
                dtd.attlists.push(a);
            }
        }
    }
    dtd
}

/// `<!KEYWORD rest` → `rest`, case-sensitively (DTD keywords are upper-case by definition).
fn keyword<'a>(body: &'a str, word: &str) -> Option<&'a str> {
    let rest = body.strip_prefix("<!")?.strip_prefix(word)?;
    rest.starts_with(|c: char| c.is_whitespace()).then(|| rest.trim_start())
}

/// Every `<!…>` declaration in the text, with the offset it starts at.
///
/// Comments are skipped whole (a DTD's comments routinely contain `>` and even markup examples),
/// and so is any quoted string, so a `>` inside a default value does not end a declaration early.
fn declarations(source: &str) -> Vec<(&str, usize)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if source[i..].starts_with("<!--") {
            i = source[i + 4..].find("-->").map(|n| i + 4 + n + 3).unwrap_or(bytes.len());
            continue;
        }
        if source[i..].starts_with("<?") {
            i = source[i + 2..].find("?>").map(|n| i + 2 + n + 2).unwrap_or(bytes.len());
            continue;
        }
        if !source[i..].starts_with("<!") {
            i += 1;
            continue;
        }
        let Some(end) = declaration_end(source, i) else { break };
        out.push((&source[i..end], i));
        i = end;
    }
    out
}

/// The offset just past the `>` that closes the declaration starting at `from`.
fn declaration_end(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = from + 2;
    let mut quote: Option<u8> = None;
    let mut depth = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' => depth += 1,
                b')' => depth = depth.saturating_sub(1),
                b'>' if depth == 0 => return Some(i + 1),
                _ => {}
            },
        }
        i += 1;
    }
    None
}

// ── Entities ─────────────────────────────────────────────────────────────────

fn collect_entities(source: &str) -> Vec<EntityDecl> {
    let mut out = Vec::new();
    for (decl, at) in declarations(source) {
        let Some(rest) = keyword(decl.trim(), "ENTITY") else { continue };
        let (parameter, rest) = match rest.strip_prefix('%') {
            Some(r) => (true, r.trim_start()),
            None => (false, rest),
        };
        let Some((name, rest)) = split_name(rest) else { continue };
        // Only internal entities have a value here. An external one (`SYSTEM "…"`) would need a
        // second file, which this crate deliberately cannot fetch — it contributes nothing rather
        // than a wrong expansion.
        let Some(value) = quoted(rest) else { continue };
        out.push(EntityDecl { name: name.to_string(), parameter, value, offset: at });
    }
    out
}

/// How many substitution rounds to run. A DTD nests parameter entities a few levels deep at
/// most; the bound is what makes a self-referential one terminate.
const MAX_ROUNDS: usize = 8;

/// Replace `%name;` with its value, repeatedly, until nothing changes.
fn expand(source: &str, entities: &[EntityDecl]) -> String {
    let params: Vec<&EntityDecl> = entities.iter().filter(|e| e.parameter).collect();
    if params.is_empty() {
        return source.to_string();
    }
    let mut text = source.to_string();
    for _ in 0..MAX_ROUNDS {
        let mut next = String::with_capacity(text.len());
        let mut changed = false;
        let mut rest = text.as_str();
        while let Some(i) = rest.find('%') {
            next.push_str(&rest[..i]);
            let after = &rest[i + 1..];
            let end = after.find(';');
            let name = end.map(|e| &after[..e]).unwrap_or("");
            match params.iter().find(|p| p.name == name).filter(|_| !name.is_empty()) {
                Some(e) => {
                    next.push_str(&e.value);
                    rest = &after[end.unwrap_or(0) + 1..];
                    changed = true;
                }
                None => {
                    next.push('%');
                    rest = after;
                }
            }
        }
        next.push_str(rest);
        text = next;
        if !changed {
            break;
        }
    }
    text
}

// ── Declarations ─────────────────────────────────────────────────────────────

fn element_decl(rest: &str, original: &str, expanded: &str, at: usize) -> Option<ElementDecl> {
    let (name, rest) = split_name(rest)?;
    let content = content_model(rest.trim_end_matches(['>', ' ', '\t', '\r', '\n']).trim());
    let offset = original_offset(original, expanded, at, "ELEMENT", name);
    Some(ElementDecl {
        name: name.to_string(),
        content,
        line: line_of(original, offset),
        doc: comment_above(original, offset),
        offset,
    })
}

fn content_model(text: &str) -> Content {
    let text = text.trim();
    if text == "EMPTY" {
        return Content::Empty;
    }
    if text == "ANY" {
        return Content::Any;
    }
    let inner = text.strip_prefix('(').and_then(|t| t.rfind(')').map(|i| &t[..i])).unwrap_or(text);
    if inner.trim_start().starts_with("#PCDATA") {
        let names: Vec<String> = inner
            .split('|')
            .skip(1)
            .map(|s| s.trim().trim_end_matches(['*', '+', '?']).trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return if names.is_empty() { Content::PcData } else { Content::Mixed(names) };
    }
    match particle(text) {
        Some(p) => Content::Children(p),
        None => Content::Any,
    }
}

/// Parse `(a, b?, (c | d)*)+` into a tree. `None` when it is not a particle at all.
fn particle(text: &str) -> Option<Particle> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // A trailing occurrence indicator binds to everything before it — but only when what comes
    // before is a complete group or name, which is what `balanced` checks.
    let last = text.chars().last()?;
    let occurs = match last {
        '?' => Occurs::Opt,
        '*' => Occurs::Star,
        '+' => Occurs::Plus,
        _ => Occurs::One,
    };
    if occurs != Occurs::One {
        let head = text[..text.len() - 1].trim();
        return particle(head).map(|p| Particle::Repeat(Box::new(p), occurs));
    }
    if text.starts_with('(') && text.ends_with(')') && balanced(&text[1..text.len() - 1]) {
        let inner = &text[1..text.len() - 1];
        let seq = split_top(inner, ',');
        if seq.len() > 1 {
            return Some(Particle::Seq(seq.iter().filter_map(|p| particle(p)).collect()));
        }
        let choice = split_top(inner, '|');
        if choice.len() > 1 {
            return Some(Particle::Choice(choice.iter().filter_map(|p| particle(p)).collect()));
        }
        return particle(inner);
    }
    // A bare name. Anything with whitespace or punctuation in it is not one, and inventing a
    // child called `a,b` would be worse than admitting the model was not understood.
    (!text.is_empty() && text.chars().all(is_name_char)).then(|| Particle::Name(text.to_string()))
}

fn balanced(text: &str) -> bool {
    let mut depth = 0i32;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split on `sep` at paren depth 0 only.
fn split_top(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if c == sep && depth == 0 => {
                out.push(text[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out
}

fn attlist_decl(rest: &str, original: &str, expanded: &str, at: usize) -> Option<AttListDecl> {
    let (element, rest) = split_name(rest)?;
    let body = rest.trim_end_matches(['>', ' ', '\t', '\r', '\n']);
    let offset = original_offset(original, expanded, at, "ATTLIST", element);

    let mut attrs = Vec::new();
    let mut cursor = body;
    while let Some((name, after_name)) = split_name(cursor.trim_start()) {
        let (kind, after_kind) = attr_kind(after_name.trim_start());
        let (default, after_default) = attr_default(after_kind.trim_start());
        // Search the ORIGINAL for the attribute name, so go-to lands on it rather than on the
        // declaration's first byte. Bounded to this declaration's neighbourhood.
        let name_at = original[offset..]
            .find(name)
            .map(|i| offset + i)
            .filter(|i| i.saturating_sub(offset) < 4096)
            .unwrap_or(offset);
        attrs.push(AttrDecl {
            name: name.to_string(),
            kind,
            default,
            line: line_of(original, name_at),
            offset: name_at,
        });
        if after_default.len() >= cursor.len() {
            break; // no progress — a malformed tail, stop rather than spin
        }
        cursor = after_default;
    }
    Some(AttListDecl {
        element: element.to_string(),
        attrs,
        line: line_of(original, offset),
        offset,
    })
}

fn attr_kind(text: &str) -> (AttrKind, &str) {
    if let Some(rest) = text.strip_prefix("NOTATION") {
        let (values, rest) = enumeration(rest.trim_start());
        return (AttrKind::Notation(values), rest);
    }
    if text.starts_with('(') {
        let (values, rest) = enumeration(text);
        return (AttrKind::Enumeration(values), rest);
    }
    // Longest first: `IDREFS` before `IDREF` before `ID`, `NMTOKENS` before `NMTOKEN`.
    for (word, kind) in [
        ("CDATA", AttrKind::CData),
        ("IDREFS", AttrKind::IdRefs),
        ("IDREF", AttrKind::IdRef),
        ("ID", AttrKind::Id),
        ("ENTITIES", AttrKind::Entities),
        ("ENTITY", AttrKind::Entity),
        ("NMTOKENS", AttrKind::NmTokens),
        ("NMTOKEN", AttrKind::NmToken),
    ] {
        if let Some(rest) = text.strip_prefix(word) {
            if !rest.starts_with(is_name_char) {
                return (kind, rest);
            }
        }
    }
    (AttrKind::CData, text)
}

fn enumeration(text: &str) -> (Vec<String>, &str) {
    let Some(close) = text.find(')') else { return (Vec::new(), text) };
    let inner = text[1..close].to_string();
    let values =
        inner.split('|').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    (values, &text[close + 1..])
}

fn attr_default(text: &str) -> (DefaultDecl, &str) {
    if let Some(rest) = text.strip_prefix("#REQUIRED") {
        return (DefaultDecl::Required, rest);
    }
    if let Some(rest) = text.strip_prefix("#IMPLIED") {
        return (DefaultDecl::Implied, rest);
    }
    if let Some(rest) = text.strip_prefix("#FIXED") {
        let rest = rest.trim_start();
        let value = quoted(rest).unwrap_or_default();
        return (DefaultDecl::Fixed(value), skip_quoted(rest));
    }
    match quoted(text) {
        Some(v) => (DefaultDecl::Value(v), skip_quoted(text)),
        None => (DefaultDecl::Implied, text),
    }
}

// ── Small readers ────────────────────────────────────────────────────────────

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')
}

/// The leading name and what follows it.
fn split_name(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let end = text.find(|c: char| !is_name_char(c)).unwrap_or(text.len());
    (end > 0).then(|| (&text[..end], &text[end..]))
}

/// The contents of the leading quoted string.
fn quoted(text: &str) -> Option<String> {
    let text = text.trim_start();
    let q = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &text[q.len_utf8()..];
    rest.find(q).map(|i| rest[..i].to_string())
}

fn skip_quoted(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(q) = trimmed.chars().next().filter(|c| *c == '"' || *c == '\'') else { return text };
    let rest = &trimmed[q.len_utf8()..];
    rest.find(q).map(|i| &rest[i + q.len_utf8()..]).unwrap_or("")
}

fn line_of(source: &str, offset: usize) -> u32 {
    source[..offset.min(source.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1
}

/// Where this declaration lives in the **original** text.
///
/// Expansion rewrote the buffer, so `at` is an offset into the expanded copy and useless for a
/// go-to. The declaration is found again by searching the original for `<!KEYWORD` followed by
/// the same name — which works whenever the keyword and the name were written literally, and
/// they nearly always are. Falls back to 0 rather than to a wrong offset: landing at the top of
/// the right file is a fair answer, landing inside an unrelated declaration is not.
fn original_offset(original: &str, expanded: &str, at: usize, word: &str, name: &str) -> usize {
    if original.len() == expanded.len() && original == expanded {
        return at;
    }
    let needle = format!("<!{word}");
    let mut from = 0usize;
    while let Some(i) = original[from..].find(&needle) {
        let start = from + i;
        let after = &original[start + needle.len()..];
        if let Some((found, _)) = split_name(after) {
            if found == name {
                return start;
            }
        }
        from = start + needle.len();
    }
    0
}

/// The comment block immediately above `offset`, as prose. A DTD has nowhere else to document
/// itself, so this is the entire content of a hover card.
fn comment_above(source: &str, offset: usize) -> String {
    let before = &source[..offset.min(source.len())];
    let trimmed = before.trim_end();
    if !trimmed.ends_with("-->") {
        return String::new();
    }
    let Some(open) = trimmed.rfind("<!--") else { return String::new() };
    // Only when it is genuinely adjacent — one blank line is fine, a page of them is not.
    if before[trimmed.len()..].bytes().filter(|&b| b == b'\n').count() > 1 {
        return String::new();
    }
    let body = &trimmed[open + 4..trimmed.len() - 3];
    body.lines()
        .map(|l| l.trim().trim_start_matches(['*', '-', '#']).trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_element_keeps_its_content_model_rather_than_a_set_of_names() {
        let d = parse("<!ELEMENT struts (package*, bean?, (constant | include)*)>");
        let e = d.element("struts").unwrap();
        assert_eq!(e.content.child_names(), ["package", "bean", "constant", "include"]);
        let Content::Children(Particle::Seq(parts)) = &e.content else {
            panic!("expected a sequence, got {:?}", e.content)
        };
        assert_eq!(parts.len(), 3);
        assert!(matches!(parts[1], Particle::Repeat(_, Occurs::Opt)));
    }

    #[test]
    fn the_three_degenerate_content_models_are_distinguished() {
        let d = parse(
            "<!ELEMENT br EMPTY>\n<!ELEMENT any ANY>\n<!ELEMENT t (#PCDATA)>\n\
             <!ELEMENT m (#PCDATA | b | i)*>",
        );
        assert_eq!(d.element("br").unwrap().content, Content::Empty);
        assert_eq!(d.element("any").unwrap().content, Content::Any);
        assert_eq!(d.element("t").unwrap().content, Content::PcData);
        assert_eq!(d.element("m").unwrap().content, Content::Mixed(vec!["b".into(), "i".into()]));
        assert!(d.element("m").unwrap().content.allows_text());
        assert!(!d.element("br").unwrap().content.allows_text());
    }

    #[test]
    fn an_attlist_reads_the_enumeration_and_the_requirement() {
        let d = parse(
            "<!ATTLIST result name CDATA \"success\" type (dispatcher|redirect) #IMPLIED \
             class CDATA #REQUIRED>",
        );
        let attrs = d.attributes_of("result");
        assert_eq!(attrs.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["name", "type", "class"]);
        assert_eq!(attrs[0].default.value(), "success");
        assert_eq!(attrs[1].values(), ["dispatcher", "redirect"]);
        assert!(attrs[2].required());
        assert!(!attrs[0].required());
    }

    /// The one that decides whether this crate is useful on a real DTD: without expansion the
    /// `<!ATTLIST>` below has one attribute instead of three.
    #[test]
    fn parameter_entities_are_expanded_before_anything_is_read() {
        let d = parse(
            "<!ENTITY % common \"id ID #IMPLIED name CDATA #IMPLIED\">\n\
             <!ATTLIST action %common; class CDATA #REQUIRED>",
        );
        let names: Vec<&str> = d.attributes_of("action").iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["id", "name", "class"]);
    }

    #[test]
    fn an_entity_that_refers_to_itself_terminates() {
        let d = parse("<!ENTITY % loop \"%loop; x\">\n<!ELEMENT a EMPTY>");
        assert!(d.element("a").is_some(), "the rest of the file still parses");
    }

    #[test]
    fn comments_and_quoted_text_do_not_end_a_declaration_early() {
        let d = parse(
            "<!-- <!ELEMENT ghost EMPTY> a > inside a comment -->\n\
             <!ATTLIST a href CDATA \"a > b\">\n\
             <!ELEMENT a EMPTY>",
        );
        assert!(d.element("ghost").is_none(), "a declaration inside a comment is not one");
        assert!(d.element("a").is_some());
        assert_eq!(d.attributes_of("a")[0].default.value(), "a > b");
    }

    /// A DTD documents itself in comments and nowhere else, so this IS the hover card.
    #[test]
    fn the_comment_above_a_declaration_becomes_its_documentation() {
        let d = parse("<!-- A unit of configuration. -->\n<!ELEMENT package (action*)>");
        assert_eq!(d.element("package").unwrap().doc, "A unit of configuration.");
        // Not one from the other end of the file.
        let d = parse("<!-- far away -->\n\n\n\n<!ELEMENT package (action*)>");
        assert_eq!(d.element("package").unwrap().doc, "");
    }

    #[test]
    fn a_declaration_keeps_an_offset_into_the_file_it_came_from() {
        let src = "<!ENTITY % c \"id ID #IMPLIED\">\n<!ELEMENT a EMPTY>\n<!ATTLIST a %c;>";
        let d = parse(src);
        let e = d.element("a").unwrap();
        assert_eq!(&src[e.offset..e.offset + 11], "<!ELEMENT a", "recovered past the expansion");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn a_malformed_declaration_is_skipped_rather_than_aborting_the_file() {
        let d = parse("<!ELEMENT>\n<!ATTLIST>\n<!ELEMENT good EMPTY>");
        assert!(d.element("good").is_some());
        assert_eq!(d.elements.len(), 1);
    }

    #[test]
    fn an_optional_particle_is_recognised_as_satisfiable_by_nothing() {
        let opt = particle("(a?, b*)").unwrap();
        assert!(opt.optional());
        assert!(!particle("(a, b?)").unwrap().optional());
        assert!(particle("(a | b?)").unwrap().optional());
    }

    /// What a content model *demands*, which is not what it permits. `struts.xml` is the case
    /// this is for: `(result-type?, interceptors?, action*)` permits four things and demands
    /// none, while an `<action>` really does demand a `<result>`.
    #[test]
    fn a_content_model_says_what_a_document_must_contain() {
        let required = |src: &str| {
            parse(&format!("<!ELEMENT e {src}>"))
                .element("e")
                .unwrap()
                .content
                .required_child_names()
        };
        assert_eq!(required("(name, value)"), ["name", "value"]);
        assert_eq!(required("(name, value?)"), ["name"]);
        assert_eq!(required("(name, value*)"), ["name"]);
        assert_eq!(required("(name, value+)"), ["name", "value"]);
        assert_eq!(required("(name)?"), Vec::<String>::new());
        // A choice is satisfied by one branch, so it demands only what every branch demands.
        assert_eq!(required("(a | b)"), Vec::<String>::new());
        assert_eq!(required("((id, a) | (id, b))"), ["id"]);
        // And the shapes that cannot demand anything by construction.
        assert!(required("ANY").is_empty());
        assert!(required("EMPTY").is_empty());
        assert!(required("(#PCDATA | a)*").is_empty());
    }
}
