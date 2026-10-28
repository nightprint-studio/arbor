//! ADD PARAMETER refactoring — preview-first, in the shape a rename already has.
//!
//! [`add_parameter_plan`] adds one parameter to a method: the declaration, every declaration that
//! overrides or implements it, and every call that has to supply a value for it. The answer is a
//! [`RenamePlan`], so the preview, the blocked/inferred vocabulary and the apply path are the ones
//! rename's are — the only difference is that every edit here is an **insertion**.
//!
//! ## What it borrows from rename
//!
//! The call sites are the reference index's (`usages_of`), the override family is rename's
//! ([`method_family`]) and so is the refusal when the family reaches a library type: a jar cannot be
//! edited to follow, so adding a parameter here would leave a class that no longer overrides what it
//! declares. The preview ([`plan_from_edits`]) and the refusal over uses the index could not type
//! ([`unseen_uses`]) are shared the same way — one place to fix each.
//!
//! ## What rename never had to decide
//!
//! A rename rewrites a **name**, and an overloaded method's calls all carry the same one — which is
//! why the index keys overloads together and why rename flags those edits `inferred` and moves on. An
//! added parameter rewrites the **argument list**, so "which overload is this call?" is now the
//! question. It is answered by arity: a call is edited only when it passes exactly as many arguments
//! as the declaration being changed has parameters. Two overloads of the same arity cannot be told
//! apart that way, so their calls are flagged `inferred` (and, from a call site, the plan refuses and
//! asks for the caret on the declaration).
//!
//! ## What it refuses
//!
//! * a method reference (`this::total`) — it has no argument list to add a value to;
//! * a `varargs` method — where the new parameter goes relative to the `T...` is not a question with
//!   one answer;
//! * a name the declaration already uses for a parameter or a local;
//! * a method some of whose calls the index could not see, for the reason rename refuses it.

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::TypeResolver;
use bennu_query::prelude::PlanFile;
use tree_sitter::Node;

use crate::refs::{classify_target, DeclKey, LangLevel, ReferenceIndex, RenameTarget};
use crate::rename::{
    find_member_name_spans, method_family, plan_from_edits, project_source, unseen_uses, unseen_uses_note, Edit,
    EditReason, RenamePlan, SubtypeMap,
};

/// The parameter to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewParameter {
    /// The type as written in the declaration — `String`, `List<Order>`.
    pub type_text: String,
    /// The parameter's name.
    pub name: String,
    /// What every existing call passes for it — an expression, written as it should appear.
    pub default_value: String,
    /// The position it takes: the index of the parameter it goes **before**, or `None` for last.
    pub before: Option<usize>,
}

/// Where the caret's call stands in relation to the parameter being added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    /// An ordinary caret: on the declaration, or on a call of the method.
    Caret,
    /// The caret is on a call that already passes one argument too many — the *wrong number of
    /// arguments* error. That argument is the new parameter's value, so this call is not edited, and
    /// the method being changed is the one with one parameter fewer than the call has arguments.
    ExtraArgument,
}

/// The plan, and what a caller needs besides the edits.
#[derive(Debug, Clone)]
pub struct AddParameterPlan {
    pub plan: RenamePlan,
    /// The parameter as it is written in the declaration — `String label`. What an offer to add it says.
    pub added: String,
    /// The parameters the changed method has today, each written `Type name` — what the dialog offers
    /// as positions.
    pub parameters: Vec<String>,
    /// The files whose declaration gained the parameter, so a caller can add the imports its type needs
    /// where it is written.
    pub declaration_files: Vec<String>,
}

/// What one declaration of the method looks like.
struct DeclSite {
    owner: String,
    file: String,
    name_start: usize,
    name_end: usize,
    params: Vec<ParamInfo>,
    /// The byte of the declaration's closing `)`.
    close: usize,
    varargs: bool,
    /// Every name a new parameter would collide with: the parameters and the locals of the body.
    taken: HashSet<String>,
}

struct ParamInfo {
    start: usize,
    end: usize,
    ty: String,
    name: String,
}

/// How a use of the method's name is written.
enum CallSite {
    Invocation { argc: usize, close: usize, args: Vec<(usize, usize)> },
    Reference,
    Other,
}

/// The call the caret is on: where its name starts and how many arguments it passes.
struct CallAt {
    name_start: usize,
    argc: usize,
}

/// The default value a call can pass for a parameter of `type_text` — the one that compiles for the
/// type, not one that means anything. The dialog lets the user write another.
pub fn default_value_for(type_text: &str) -> String {
    match type_text.trim() {
        "boolean" => "false",
        "char" => "'\\0'",
        "byte" => "(byte) 0",
        "short" => "(short) 0",
        "int" | "long" | "float" | "double" => "0",
        _ => "null",
    }
    .to_string()
}

/// Whether `text` is a plausible Java type: an identifier path with optional type arguments and
/// array brackets. Deliberately loose — the compiler is the judge — but it keeps `a b` and `)` out.
pub fn is_type_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    let mut depth = 0i32;
    let mut last_ident = false;
    for ch in t.chars() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            c if c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '[' | ']' | ',' | ' ' | '?' | '&') => {
                if c.is_alphanumeric() {
                    last_ident = true;
                }
            }
            _ => return false,
        }
    }
    depth == 0 && last_ident && !t.starts_with('.') && !t.ends_with('.')
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

/// Plan adding `param` to the method at `file`:`offset`. `None` when the caret is not on a method this
/// project declares; a plan with `blocked` set when it is, and the change cannot be made safely.
#[allow(clippy::too_many_arguments)]
pub fn add_parameter_plan(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    param: &NewParameter,
    anchor: Anchor,
    resolver: &dyn TypeResolver,
    policy: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    subtypes: &SubtypeMap,
    java_files: &[PlanFile],
    level: LangLevel,
) -> Option<AddParameterPlan> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
    let RenameTarget::Member { key: DeclKey::Method { owner, name } } = &target else {
        return None;
    };

    let (family, library) = method_family(resolver, policy, subtypes, owner, name);
    let mut blocked: Option<String> = library.map(|lib| {
        format!(
            "`{name}` overrides {} — a library type, which cannot take the new parameter. \
             Adding it here would leave the class no longer overriding what it declares.",
            lib.replace('/', ".")
        )
    });

    let mut sites: Vec<DeclSite> = Vec::new();
    for o in &family {
        let decl_file = index.file_declaring(o).unwrap_or(file).to_string();
        let Some(text) = text_of(&decl_file, file, source, java_files) else { continue };
        let key = DeclKey::Method { owner: o.clone(), name: name.clone() };
        let spans = find_member_name_spans(text, &key);
        for site in read_sites(o, &decl_file, text, &spans) {
            if !sites.iter().any(|s| s.file == site.file && s.name_start == site.name_start) {
                sites.push(site);
            }
        }
    }

    // Which declaration is being changed, as an arity (and, from the declaration, a signature).
    let on_declaration =
        sites.iter().find(|s| s.file == file && s.name_start <= offset && offset <= s.name_end);
    let call = if on_declaration.is_none() { call_at(source, offset) } else { None };
    let (arity, signature): (usize, Option<Vec<String>>) = match (on_declaration, &call, anchor) {
        (Some(d), _, _) => (d.params.len(), Some(d.params.iter().map(|p| p.ty.clone()).collect())),
        (None, Some(c), Anchor::Caret) => (c.argc, None),
        (None, Some(c), Anchor::ExtraArgument) => (c.argc.checked_sub(1)?, None),
        _ => return None,
    };

    // Per owner, the declarations of that arity — narrowed to the one with this signature where the
    // declaration was the starting point and an owner has several.
    let mut matched: Vec<&DeclSite> = Vec::new();
    let mut ambiguous = false;
    let mut owners_seen: Vec<&str> = Vec::new();
    for s in sites.iter().filter(|s| s.params.len() == arity) {
        if owners_seen.contains(&s.owner.as_str()) {
            continue;
        }
        owners_seen.push(&s.owner);
        let group: Vec<&DeclSite> =
            sites.iter().filter(|x| x.owner == s.owner && x.params.len() == arity).collect();
        // Several of one arity make every call ambiguous, whichever declaration is changed: a call
        // cannot be told from the other overload's by counting arguments.
        ambiguous |= group.len() > 1;
        let chosen: Vec<&DeclSite> = match &signature {
            Some(sig) => {
                let exact: Vec<&DeclSite> = group
                    .iter()
                    .copied()
                    .filter(|x| x.params.iter().map(|p| p.ty.as_str()).eq(sig.iter().map(String::as_str)))
                    .collect();
                if exact.is_empty() { group } else { exact }
            }
            None => group,
        };
        matched.extend(chosen);
    }

    if blocked.is_none() {
        blocked = refusal_before_edits(&matched, ambiguous, signature.is_some(), name, arity, param);
    }

    let mut edits: Vec<Edit> = Vec::new();
    let mut covered: HashSet<(String, usize)> = HashSet::new();
    for s in &sites {
        covered.insert((s.file.clone(), s.name_start));
    }
    let insert = param.before.map_or(arity, |b| b.min(arity));
    let decl_text = format!("{} {}", param.type_text.trim(), param.name.trim());

    if blocked.is_none() {
        for s in &matched {
            let (at, text) = declaration_insertion(s, insert, &decl_text);
            edits.push(insertion(&s.file, at, text, EditReason::Declaration, false));
        }
        blocked = call_edits(
            index,
            &family,
            name,
            arity,
            insert,
            &param.default_value,
            ambiguous,
            anchor,
            (file, call.as_ref().map(|c| c.name_start)),
            |f| text_of(f, file, source, java_files),
            &mut edits,
            &mut covered,
        );
    }

    // A use the index could not place is a call this plan cannot give a value to — and an extra
    // argument-less call left behind does not compile. The same refusal rename makes, for the same reason.
    if blocked.is_none() {
        let unseen = unseen_uses(index, std::slice::from_ref(name), |u| {
            covered.contains(&(u.file.clone(), u.start))
        });
        blocked = unseen_uses_note(name, &unseen)
            .map(|note| format!("{note} Those calls would be left without the new argument."));
    }

    let parameters: Vec<String> = matched
        .first()
        .map(|s| s.params.iter().map(|p| format!("{} {}", p.ty, p.name)).collect())
        .unwrap_or_default();
    let mut declaration_files: Vec<String> = matched.iter().map(|s| s.file.clone()).collect();
    declaration_files.dedup();

    let simple_owner = owner.rsplit(['/', '$']).next().unwrap_or(owner);
    Some(AddParameterPlan {
        added: decl_text.clone(),
        plan: plan_from_edits(
            name.clone(),
            name,
            format!("add parameter `{decl_text}` to method {simple_owner}.{name}()"),
            edits,
            None,
            blocked,
        ),
        parameters,
        declaration_files,
    })
}

/// The refusals that do not need a single call site looked at.
fn refusal_before_edits(
    matched: &[&DeclSite],
    ambiguous: bool,
    from_declaration: bool,
    name: &str,
    arity: usize,
    param: &NewParameter,
) -> Option<String> {
    if matched.is_empty() {
        return Some(format!(
            "No declaration of `{name}` in this project takes {arity} parameter{}.",
            if arity == 1 { "" } else { "s" }
        ));
    }
    if ambiguous && !from_declaration {
        return Some(format!(
            "Several overloads of `{name}` take {arity} parameter{} — put the caret on the declaration \
             of the one to change.",
            if arity == 1 { "" } else { "s" }
        ));
    }
    if !is_type_text(&param.type_text) {
        return Some("The parameter's type is not a valid Java type.".to_string());
    }
    if !is_identifier(param.name.trim()) {
        return Some("The parameter's name is not a valid Java identifier.".to_string());
    }
    if param.default_value.trim().is_empty() {
        return Some("Every existing call needs a value to pass — write one.".to_string());
    }
    if matched.iter().any(|s| s.varargs) {
        return Some(format!(
            "`{name}` takes a varargs parameter — where a new one goes relative to `...` has no single \
             answer, so Bennu does not guess."
        ));
    }
    matched
        .iter()
        .find(|s| s.taken.contains(param.name.trim()))
        .map(|s| {
            format!(
                "`{}` already names a parameter or a local in the {} declaration in {}.",
                param.name.trim(),
                name,
                s.file.rsplit('/').next().unwrap_or(&s.file)
            )
        })
}

/// Where the declaration gets the new parameter, and what is written there.
fn declaration_insertion(site: &DeclSite, insert: usize, decl_text: &str) -> (usize, String) {
    match site.params.len() {
        0 => (site.close, decl_text.to_string()),
        n if insert >= n => (site.params[n - 1].end, format!(", {decl_text}")),
        _ => (site.params[insert].start, format!("{decl_text}, ")),
    }
}

/// The call-site insertions, appended to `edits`. Returns the refusal when a use cannot take one.
#[allow(clippy::too_many_arguments)]
fn call_edits<'a>(
    index: &ReferenceIndex,
    family: &[String],
    name: &str,
    arity: usize,
    insert: usize,
    default_value: &str,
    ambiguous: bool,
    anchor: Anchor,
    caret_call: (&str, Option<usize>),
    text_of: impl Fn(&str) -> Option<&'a str>,
    edits: &mut Vec<Edit>,
    covered: &mut HashSet<(String, usize)>,
) -> Option<String> {
    let mut by_file: Vec<(String, Vec<(usize, usize, usize)>)> = Vec::new();
    let mut seen: HashSet<(String, usize)> = HashSet::new();
    for o in family {
        let key = DeclKey::Method { owner: o.clone(), name: name.to_string() };
        for u in index.usages_of(&key) {
            if !seen.insert((u.file.clone(), u.start)) {
                continue;
            }
            covered.insert((u.file.clone(), u.start));
            match by_file.iter_mut().find(|(f, _)| *f == u.file) {
                Some((_, v)) => v.push((u.start, u.end, u.line)),
                None => by_file.push((u.file.clone(), vec![(u.start, u.end, u.line)])),
            }
        }
    }

    let value = default_value.trim();
    let mut references: Vec<(String, usize)> = Vec::new();
    for (file, uses) in by_file {
        let Some(text) = text_of(&file) else { continue };
        let Some(tree) = bennu_java::prelude::parse_java(text) else { continue };
        let root = tree.root_node();
        for (start, end, line) in uses {
            match call_site_at(root, start, end) {
                CallSite::Reference => references.push((file.clone(), line)),
                CallSite::Other => {}
                CallSite::Invocation { argc, close, args } => {
                    // Another overload's call, or the call that already passes the value.
                    if argc != arity {
                        continue;
                    }
                    if anchor == Anchor::ExtraArgument && caret_call == (file.as_str(), Some(start)) {
                        continue;
                    }
                    let (at, written) = match args.len() {
                        0 => (close, value.to_string()),
                        n if insert >= n => (args[n - 1].1, format!(", {value}")),
                        _ => (args[insert].0, format!("{value}, ")),
                    };
                    edits.push(insertion(&file, at, written, EditReason::Reference, ambiguous));
                }
            }
        }
    }
    references.first().map(|(file, line)| {
        format!(
            "`{name}` is used as a method reference at {} site(s) — the first is {}:{line}. A reference \
             has no argument list to add a value to; change those to lambdas first.",
            references.len(),
            file.rsplit('/').next().unwrap_or(file)
        )
    })
}

fn insertion(file: &str, at: usize, text: String, reason: EditReason, inferred: bool) -> Edit {
    Edit { file: file.to_string(), start: at, end: at, new_text: text, old: String::new(), reason, inferred }
}

fn text_of<'a>(
    file: &str,
    caret_file: &str,
    caret_source: &'a str,
    java_files: &'a [PlanFile],
) -> Option<&'a str> {
    if file == caret_file {
        Some(caret_source)
    } else {
        project_source(java_files, file)
    }
}

// ── reading the tree ─────────────────────────────────────────────────────────────────────────────

fn is_comment(n: &Node) -> bool {
    matches!(n.kind(), "line_comment" | "block_comment")
}

fn read_sites(owner: &str, file: &str, text: &str, spans: &[(usize, usize)]) -> Vec<DeclSite> {
    let Some(tree) = bennu_java::prelude::parse_java(text) else { return Vec::new() };
    let root = tree.root_node();
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for &(start, end) in spans {
        let Some(name) = root.descendant_for_byte_range(start, end) else { continue };
        let Some(method) = name.parent().filter(|p| p.kind() == "method_declaration") else { continue };
        let Some(params) = method.child_by_field_name("parameters") else { continue };
        let mut infos = Vec::new();
        let mut varargs = false;
        let mut cursor = params.walk();
        for p in params.named_children(&mut cursor) {
            match p.kind() {
                "formal_parameter" => {
                    let ty = p.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok());
                    let pname = p.child_by_field_name("name").and_then(|t| t.utf8_text(bytes).ok());
                    if let (Some(ty), Some(pname)) = (ty, pname) {
                        infos.push(ParamInfo {
                            start: p.start_byte(),
                            end: p.end_byte(),
                            ty: squash(ty),
                            name: pname.to_string(),
                        });
                    }
                }
                "spread_parameter" => {
                    varargs = true;
                    infos.push(ParamInfo {
                        start: p.start_byte(),
                        end: p.end_byte(),
                        ty: "...".to_string(),
                        name: String::new(),
                    });
                }
                _ => {}
            }
        }
        let mut taken: HashSet<String> = infos.iter().map(|p| p.name.clone()).collect();
        collect_declared_names(method, bytes, &mut taken);
        out.push(DeclSite {
            owner: owner.to_string(),
            file: file.to_string(),
            name_start: start,
            name_end: end,
            params: infos,
            close: params.end_byte().saturating_sub(1),
            varargs,
            taken,
        });
    }
    out
}

fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join("")
}

/// Every name a declaration inside `method` introduces — locals, lambda and catch parameters,
/// enhanced-for variables — which is what a new parameter's name must not repeat.
fn collect_declared_names(method: Node, bytes: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![method];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_declarator" | "formal_parameter" | "catch_formal_parameter" | "enhanced_for_statement" => {
                if let Some(name) = n.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) {
                    out.insert(name.to_string());
                }
            }
            "inferred_parameters" => {
                let mut c = n.walk();
                for id in n.named_children(&mut c).filter(|x| x.kind() == "identifier") {
                    if let Ok(t) = id.utf8_text(bytes) {
                        out.insert(t.to_string());
                    }
                }
            }
            _ => {}
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
}

/// The method call the caret is in: its name's start and how many arguments it passes.
fn call_at(source: &str, offset: usize) -> Option<CallAt> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let at = offset.min(source.len());
    let mut cur = tree.root_node().descendant_for_byte_range(at, at);
    while let Some(n) = cur {
        if n.kind() == "method_invocation" {
            let name = n.child_by_field_name("name")?;
            let args = n.child_by_field_name("arguments")?;
            let argc = {
                let mut c = args.walk();
                args.named_children(&mut c).filter(|a| !is_comment(a)).count()
            };
            return Some(CallAt { name_start: name.start_byte(), argc });
        }
        cur = n.parent();
    }
    None
}

/// How the use at `[start, end)` is written.
fn call_site_at(root: Node, start: usize, end: usize) -> CallSite {
    let Some(node) = root.descendant_for_byte_range(start, end) else { return CallSite::Other };
    let Some(parent) = node.parent() else { return CallSite::Other };
    match parent.kind() {
        "method_reference" => CallSite::Reference,
        "method_invocation" if parent.child_by_field_name("name").is_some_and(|n| n.id() == node.id()) => {
            let Some(args) = parent.child_by_field_name("arguments") else { return CallSite::Other };
            let mut c = args.walk();
            let ranges: Vec<(usize, usize)> = args
                .named_children(&mut c)
                .filter(|a| !is_comment(a))
                .map(|a| (a.start_byte(), a.end_byte()))
                .collect();
            CallSite::Invocation {
                argc: ranges.len(),
                close: args.end_byte().saturating_sub(1),
                args: ranges,
            }
        }
        _ => CallSite::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::{build_reference_index, SourceFile};
    use crate::rename::tests::build_resolver;

    fn param(ty: &str, name: &str, default: &str) -> NewParameter {
        NewParameter {
            type_text: ty.to_string(),
            name: name.to_string(),
            default_value: default.to_string(),
            before: None,
        }
    }

    fn plan_with(
        files: &[(&str, &str)],
        target_file: &str,
        offset: usize,
        param: &NewParameter,
        anchor: Anchor,
    ) -> Option<AddParameterPlan> {
        let (resolver, project_types) = build_resolver(files);
        let src: Vec<SourceFile> =
            files.iter().map(|(p, s)| SourceFile { path: p.to_string(), source: s.to_string() }).collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        let java_files: Vec<PlanFile> =
            files.iter().map(|(p, s)| PlanFile { path: p.to_string(), source: s.to_string() }).collect();
        let source = files.iter().find(|(p, _)| *p == target_file).unwrap().1;
        let subtypes = SubtypeMap::build(&index, &resolver);
        add_parameter_plan(
            &index, target_file, source, offset, param, anchor, &resolver, &resolver, &project_types,
            &subtypes, &java_files, LangLevel(0),
        )
    }

    /// The text after every edit of the plan is applied, per file.
    fn applied(files: &[(&str, &str)], plan: &AddParameterPlan) -> HashMap<String, String> {
        let mut out: HashMap<String, String> =
            files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        for f in &plan.plan.files {
            let text = out.get_mut(&f.file).unwrap();
            for e in f.edits.iter().rev() {
                text.replace_range(e.start..e.end, &e.new_text);
            }
        }
        out
    }

    const A: &str = "package p;\npublic class A {\n  int total(int a) { return a; }\n  int use() { return total(1) + total(2); }\n}\n";

    #[test]
    fn the_declaration_and_every_call_gain_the_parameter() {
        let files = [("A.java", A)];
        let plan = plan_with(&files, "A.java", A.find("total(int").unwrap() + 1, &param("String", "label", "null"), Anchor::Caret)
            .expect("a plan");
        assert!(plan.plan.blocked.is_none(), "{:?}", plan.plan.blocked);
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("int total(int a, String label)"), "{text}");
        assert!(text.contains("total(1, null) + total(2, null)"), "{text}");
    }

    #[test]
    fn it_can_start_from_a_call() {
        let files = [("A.java", A)];
        let at = A.find("total(1)").unwrap() + 2;
        let plan = plan_with(&files, "A.java", at, &param("int", "scale", "10"), Anchor::Caret).expect("a plan");
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("total(int a, int scale)"), "{text}");
        assert!(text.contains("total(1, 10)"), "{text}");
    }

    #[test]
    fn a_method_with_no_parameters_takes_the_first_one() {
        let src = "package p;\npublic class A {\n  int n() { return 1; }\n  int use() { return n(); }\n}\n";
        let files = [("A.java", src)];
        let plan = plan_with(&files, "A.java", src.find("n()").unwrap() + 1, &param("int", "x", "0"), Anchor::Caret)
            .expect("a plan");
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("int n(int x)"), "{text}");
        assert!(text.contains("return n(0);"), "{text}");
    }

    #[test]
    fn it_goes_before_the_parameter_asked_for() {
        let files = [("A.java", A)];
        let mut p = param("String", "label", "\"x\"");
        p.before = Some(0);
        let plan = plan_with(&files, "A.java", A.find("total(int").unwrap() + 1, &p, Anchor::Caret).unwrap();
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("int total(String label, int a)"), "{text}");
        assert!(text.contains("total(\"x\", 1)"), "{text}");
    }

    #[test]
    fn calls_in_other_files_are_edited_too() {
        let b = "package p;\npublic class B {\n  int go(A a) { return a.total(5); }\n}\n";
        let files = [("A.java", A), ("B.java", b)];
        let plan = plan_with(&files, "A.java", A.find("total(int").unwrap() + 1, &param("int", "k", "1"), Anchor::Caret)
            .expect("a plan");
        let out = applied(&files, &plan);
        assert!(out["B.java"].contains("a.total(5, 1)"), "{}", out["B.java"]);
    }

    #[test]
    fn an_overload_of_another_arity_is_left_alone() {
        let src = "package p;\npublic class A {\n  int total(int a) { return a; }\n  int total(int a, int b) { return a + b; }\n  int use() { return total(1) + total(1, 2); }\n}\n";
        let files = [("A.java", src)];
        let at = src.find("total(int a)").unwrap() + 1;
        let plan = plan_with(&files, "A.java", at, &param("int", "c", "0"), Anchor::Caret).unwrap();
        assert!(plan.plan.blocked.is_none(), "{:?}", plan.plan.blocked);
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("total(int a, int c)"), "{text}");
        assert!(text.contains("total(int a, int b)"), "{text}");
        assert!(text.contains("total(1, 0) + total(1, 2)"), "{text}");
    }

    #[test]
    fn overloads_of_one_arity_make_the_calls_inferred() {
        let src = "package p;\npublic class A {\n  int f(int a) { return a; }\n  int f(String a) { return 0; }\n  int use() { return f(1); }\n}\n";
        let files = [("A.java", src)];
        let at = src.find("f(int a)").unwrap();
        let plan = plan_with(&files, "A.java", at, &param("int", "c", "0"), Anchor::Caret).unwrap();
        assert!(plan.plan.blocked.is_none(), "{:?}", plan.plan.blocked);
        assert!(plan.plan.has_inferred, "the call cannot be told apart from f(String)'s");
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("int f(int a, int c)"), "{text}");
        assert!(text.contains("int f(String a) {"), "{text}");
    }

    #[test]
    fn from_a_call_an_ambiguous_overload_is_refused() {
        let src = "package p;\npublic class A {\n  int f(int a) { return a; }\n  int f(String a) { return 0; }\n  int use() { return f(1); }\n}\n";
        let files = [("A.java", src)];
        let at = src.rfind("f(1)").unwrap();
        let plan = plan_with(&files, "A.java", at, &param("int", "c", "0"), Anchor::Caret).unwrap();
        assert!(plan.plan.blocked.is_some(), "which f is that call?");
    }

    #[test]
    fn a_method_reference_blocks_the_change() {
        let src = "package p;\npublic class A {\n  int total(int a) { return a; }\n  java.util.function.IntUnaryOperator op() { return this::total; }\n}\n";
        let files = [("A.java", src)];
        let plan = plan_with(&files, "A.java", src.find("total(int").unwrap() + 1, &param("int", "k", "0"), Anchor::Caret).unwrap();
        let why = plan.plan.blocked.expect("blocked");
        assert!(why.contains("method reference"), "{why}");
    }

    #[test]
    fn a_name_already_taken_blocks_the_change() {
        let src = "package p;\npublic class A {\n  int total(int a) { int tmp = a; return tmp; }\n}\n";
        let files = [("A.java", src)];
        let at = src.find("total(int").unwrap() + 1;
        assert!(plan_with(&files, "A.java", at, &param("int", "a", "0"), Anchor::Caret).unwrap().plan.blocked.is_some());
        assert!(
            plan_with(&files, "A.java", at, &param("int", "tmp", "0"), Anchor::Caret).unwrap().plan.blocked.is_some(),
            "a local of the body collides too"
        );
    }

    #[test]
    fn a_varargs_method_is_refused() {
        let src = "package p;\npublic class A {\n  int sum(int... xs) { return 0; }\n}\n";
        let files = [("A.java", src)];
        let plan = plan_with(&files, "A.java", src.find("sum").unwrap() + 1, &param("int", "k", "0"), Anchor::Caret).unwrap();
        assert!(plan.plan.blocked.is_some());
    }

    #[test]
    fn an_invalid_type_or_name_or_empty_value_is_refused() {
        let files = [("A.java", A)];
        let at = A.find("total(int").unwrap() + 1;
        for bad in [param("", "x", "0"), param("int", "1x", "0"), param("int", "x", "  "), param("a b", "x", "0")] {
            let plan = plan_with(&files, "A.java", at, &bad, Anchor::Caret).unwrap();
            assert!(plan.plan.blocked.is_some(), "{bad:?}");
        }
    }

    #[test]
    fn the_call_that_already_passes_the_value_is_not_edited() {
        // `total(1, "x")` is the wrong-argument-count call: its second argument IS the new parameter.
        let src = "package p;\npublic class A {\n  int total(int a) { return a; }\n  int one() { return total(1); }\n  int two() { return total(2, \"x\"); }\n}\n";
        let files = [("A.java", src)];
        let at = src.find("total(2").unwrap();
        let plan = plan_with(&files, "A.java", at, &param("String", "label", "null"), Anchor::ExtraArgument)
            .expect("a plan");
        assert!(plan.plan.blocked.is_none(), "{:?}", plan.plan.blocked);
        let text = &applied(&files, &plan)["A.java"];
        assert!(text.contains("int total(int a, String label)"), "{text}");
        assert!(text.contains("total(1, null)"), "{text}");
        assert!(text.contains("total(2, \"x\")"), "the caller's own argument stays: {text}");
    }

    #[test]
    fn a_caret_that_is_not_on_a_method_gives_nothing() {
        let files = [("A.java", A)];
        assert!(plan_with(&files, "A.java", A.find("class A").unwrap() + 7, &param("int", "x", "0"), Anchor::Caret).is_none());
    }

    #[test]
    fn defaults_compile_for_the_type() {
        assert_eq!(default_value_for("boolean"), "false");
        assert_eq!(default_value_for("int"), "0");
        assert_eq!(default_value_for("byte"), "(byte) 0");
        assert_eq!(default_value_for("String"), "null");
        assert_eq!(default_value_for("List<Order>"), "null");
    }

    #[test]
    fn type_text_is_checked_loosely() {
        for ok in ["int", "String", "List<String>", "Map<String, List<Integer>>", "int[]", "java.util.Map.Entry<K, V>"] {
            assert!(is_type_text(ok), "{ok}");
        }
        for bad in ["", "a b c)", "List<", ">x", "1+1;", "."] {
            assert!(!is_type_text(bad), "{bad}");
        }
    }
}
