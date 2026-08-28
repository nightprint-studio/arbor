//! RENAME refactoring — best-effort, PREVIEW-first (docs §5 #10-12).
//!
//! [`rename_plan`] classifies the symbol under the caret ([`crate::refs::classify_target`])
//! and computes **all** edit sites, returned as a per-file preview. [`rename_apply`]
//! flattens that plan into the concrete edits the FE applies (so CodeMirror's undo works
//! and the FE — not the backend — writes the buffers).
//!
//! The three caret kinds (docs §5 #10-12):
//!   * **Local variable / parameter** → scope-exact, single file. Pure tree-sitter scope
//!     walk; NO index needed, and NO unrelated same-named symbol is touched.
//!   * **Field / method** → the declaration + every cross-file reference from the
//!     [`crate::refs::ReferenceIndex`].
//!   * **Class / interface** → the declaration + references (simple-name use sites) +
//!     `import` statements + Spring bean XML `<bean class="oldFQCN">` occurrences
//!     ([`bennu_web::prelude::bean_class_value_spans`]). A Struts `<action class="beanId">`
//!     uses a bean-**id**, not the FQCN, so it is correctly NOT edited (honest limit).
//!
//! Conservative: an edit is emitted only where we can justify it. Method use-sites are
//! flagged `inferred` (overloads collapse to one key) so the FE surfaces them for review,
//! never silently applies them as if exact.
//!
//! Everything here is a **free function over a borrowed project view** — the index, the sources,
//! the type map, the subtype map. What holds those four together and hands them out consistently is
//! [`crate::engine::SemanticEngine`], which is also where go-to, hover and the hierarchies enter.
//! The split is the point: this module is testable against an in-memory resolver with no live JDK,
//! and the engine is where a lock is taken.

use std::collections::{HashMap, HashSet};

use bennu_java::prelude::{find_type_name_span, Member, TypeResolver};
use bennu_query::prelude::PlanFile;
use bennu_web::prelude::bean_class_value_spans;
use tree_sitter::Node;

use crate::refs::{classify_target, DeclKey, LangLevel, ReferenceIndex, RenameTarget};

/// Why an edit was planned (drives the preview grouping + the honest-limits surfacing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditReason {
    Declaration,
    Reference,
    Import,
    SpringBean,
    Local,
}

impl EditReason {
    pub fn label(&self) -> &'static str {
        match self {
            EditReason::Declaration => "declaration",
            EditReason::Reference => "reference",
            EditReason::Import => "import",
            EditReason::SpringBean => "spring-bean",
            EditReason::Local => "local",
        }
    }
}

/// One concrete edit: replace `[start, end)` in `file` with `new_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub new_text: String,
    /// The exact text currently at `[start, end)` — the FE shows old→new and can assert
    /// the buffer still matches before applying (a stale-buffer guard).
    pub old: String,
    /// Why this edit exists.
    pub reason: EditReason,
    /// True when the edit is inferred/heuristic (a method use-site where an overload could
    /// collapse). The FE surfaces these for review.
    pub inferred: bool,
}

/// The edits for one file (the preview list the FE renders per file).
#[derive(Debug, Clone)]
pub struct FileEdits {
    pub file: String,
    pub edits: Vec<Edit>,
}

/// The rename PREVIEW: what the caret resolved to + the per-file edit sites.
#[derive(Debug, Clone)]
pub struct RenamePlan {
    pub old_name: String,
    pub new_name: String,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"local `x`"`, …).
    pub target_label: String,
    /// The edits, grouped by file (preserves per-file preview rendering).
    pub files: Vec<FileEdits>,
    /// Whether the plan carries any `inferred` edit (the FE nudges review).
    pub has_inferred: bool,
    /// The source file this rename also has to move, if any — see [`FileRename`].
    pub file_rename: Option<FileRename>,
    /// Why this rename must NOT be applied, when it must not be.
    ///
    /// The edits are still computed and still shown — seeing what it *would* do is how the reason
    /// makes sense — but a caller has to treat this as a refusal, not a warning to click past.
    /// The one case today is a method that overrides a member of a library type: the jar cannot be
    /// edited to follow, so renaming here produces a class that stops compiling.
    pub blocked: Option<String>,
}

/// A source file that has to be renamed along with the type it holds.
///
/// Java requires a public top-level type and its file to share a name, so renaming the type without
/// the file leaves code that does not compile. Only ever set when the file is named after the type
/// being renamed, which is exactly the top-level case: a nested type's file is named after its
/// outer type and must stay put.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRename {
    /// The file's current path (forward slashes, as the FE keys them).
    pub from: String,
    /// The path it must take — same directory, new basename.
    pub to: String,
}

impl RenamePlan {
    pub fn total_edits(&self) -> usize {
        self.files.iter().map(|f| f.edits.len()).sum()
    }
}

/// A resolved go-to-declaration target: the declaration NAME span in the owning **project**
/// file, plus 1-based line/col (computed from that file's source) and a human label. The be
/// layer maps this onto the wire `DeclarationTarget` field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationLocation {
    /// Absolute path (forward slashes) of the project file declaring the symbol.
    pub file: String,
    /// Start byte offset of the declaration NAME token in `file`.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// 1-based line of the declaration name in `file`.
    pub line: u32,
    /// 1-based column of the declaration name in `file`.
    pub col: u32,
    /// A short human label of the target (`"method com.x.Foo.bar()"`, `"class com.x.Order"`,
    /// `"local `x`"`) — the same style [`crate::refs::DeclKey::label`] uses.
    pub label: String,
}

/// Compute the rename plan for the symbol at `file`:`offset`. `java_files` are all the
/// project's `.java` sources; `xml_files` are the config fragments. Returns `None` when
/// the caret isn't on a renameable identifier.
///
/// `resolver` classifies the caret and walks the project hierarchy — it is the same (cheap) view
/// the reference walk ran with. `policy` answers the one question that needs the FULL classpath:
/// whether a supertype in a dependency jar declares this method, which decides whether the rename
/// is allowed at all. Pass the same resolver for both when there is only one.
#[allow(clippy::too_many_arguments)]
pub fn rename_plan(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    new_name: &str,
    resolver: &dyn TypeResolver,
    policy: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    subtypes: &SubtypeMap,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    level: LangLevel,
) -> Option<RenamePlan> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
    let mut file_rename: Option<FileRename> = None;
    let mut blocked: Option<String> = None;

    let (old_name, label, edits) = match &target {
        RenameTarget::Local {
            name,
            def_start,
            def_end,
        } => {
            let planned = plan_local(source, file, *def_start, *def_end, name, new_name);
            blocked = planned.capture;
            (name.clone(), format!("local `{name}`"), planned.edits)
        }
        RenameTarget::Member { key } => {
            // The declaration lives in the file that DECLARES the member's owner (walked up the
            // hierarchy), which is not necessarily the caret's file. Renaming from a use site in
            // another file must still rewrite the declaration — otherwise the plan renamed the
            // call sites but left `int foo()` untouched. Fall back to the caret file only when
            // the declaring file isn't a known project source (a JDK/dep owner).
            // A method can be declared at several levels of one hierarchy, and to every caller
            // that is ONE method — so the whole override family moves together. A field has no
            // such family (hiding a field is not overriding it, and a shadowing field of the same
            // name is a different field), so it stays alone.
            let owners: Vec<String> = match key {
                DeclKey::Method { owner, name } => {
                    // A library supertype declaring the same method makes this an override of code
                    // we cannot edit. Plan it anyway — the edit list is what makes the refusal
                    // legible — but mark it unappliable.
                    let family = override_family(resolver, subtypes, owner, name);
                    // Asked of EVERY member of the family, not just the caret's own type. A method
                    // can be declared by a project interface AND, in one of that interface's
                    // implementors, override a library class's method of the same name:
                    // `FastDateFormat extends java.text.Format implements DateParser` declares
                    // `parseObject` on both sides. Checking only the starting type let the rename
                    // through and the class stopped overriding `Format.parseObject`.
                    blocked = family.iter().find_map(|o| {
                        library_override(policy, o, name).map(|lib| {
                            format!(
                                "`{name}` overrides {} — a library type, which cannot be renamed \
                                 with it. Renaming only this side would stop the class implementing \
                                 what it declares.",
                                lib.replace('/', ".")
                            )
                        })
                    });
                    family
                }
                _ => vec![key.owner_binary().to_string()],
            };
            let mut edits = Vec::new();
            let mut also_named: Vec<String> = Vec::new();
            for owner in &owners {
                let member = match key {
                    DeclKey::Method { name, .. } => DeclKey::Method {
                        owner: owner.clone(),
                        name: name.clone(),
                    },
                    other => other.clone(),
                };
                let decl_file = index.file_declaring(owner).unwrap_or(file);
                let decl_source = project_source(java_files, decl_file).unwrap_or(source);
                // An ANONYMOUS class overriding this method lives wherever the `new` was written,
                // which is usually not the file that declares the interface — and an anonymous
                // class is not in `project_types` (every one of them is called `1`), so the family
                // above cannot reach it. The files that MENTION the owner type can: that set comes
                // straight out of the index and is small, and scanning it finds the override where
                // it actually is.
                // Only for a method the owner declares ABSTRACT. That is what an anonymous class is
                // written to implement, and the gate matters: without it this scans every file that
                // mentions the type for every method rename, which took a whole-project fix from
                // 65 seconds to 300.
                let abstract_here = resolver
                    .members_of(owner)
                    .map(|cm| {
                        cm.methods
                            .iter()
                            .any(|m| m.name == member_name(&member) && m.is_abstract)
                    })
                    .unwrap_or(false);
                let anon_hosts: &[crate::refs::UsageLocation] = if abstract_here {
                    index.usages_of(&DeclKey::Type {
                        binary: owner.clone(),
                    })
                } else {
                    &[]
                };
                let mut scanned: Vec<&str> = Vec::new();
                for u in anon_hosts {
                    if u.file == decl_file || scanned.contains(&u.file.as_str()) {
                        continue;
                    }
                    scanned.push(u.file.as_str());
                    let Some(src) = project_source(java_files, &u.file) else {
                        continue;
                    };
                    for (ds, de) in find_member_name_spans(src, &member) {
                        edits.push(Edit {
                            file: u.file.clone(),
                            start: ds,
                            end: de,
                            new_text: new_name.to_string(),
                            old: member_name(&member),
                            reason: EditReason::Declaration,
                            inferred: false,
                        });
                    }
                }
                let planned = plan_member(index, decl_file, decl_source, &member, new_name);
                for n in planned.also_named {
                    if !also_named.contains(&n) {
                        also_named.push(n);
                    }
                }
                edits.extend(planned.edits);
            }
            // Two levels of the family can land on the same bytes (a file declaring both). Keep
            // one edit per range, preferring the declaration — the same rule the type pass uses.
            edits.sort_by(|a, b| {
                (a.file.as_str(), a.start, a.end, reason_rank(&a.reason)).cmp(&(
                    b.file.as_str(),
                    b.start,
                    b.end,
                    reason_rank(&b.reason),
                ))
            });
            edits.dedup_by(|a, b| a.file == b.file && a.start == b.start && a.end == b.end);
            // A member plan with no DECLARATION edit would rewrite every caller to a name that
            // nothing declares — worse than doing nothing. It happens when the caret is on a
            // member NOBODY WROTE DOWN: a Lombok accessor has no source to edit, so renaming
            // `getCustomerName` here would leave the generated method named after the field it
            // still comes from. Refuse, and let the user rename the FIELD — the generated
            // accessors follow it (see `generated_accessors`).
            if !edits.iter().any(|e| e.reason == EditReason::Declaration) {
                return None;
            }
            // A use the walk could not SEE is a use this plan cannot rewrite — and a rename that
            // rewrites a declaration while leaving a call behind does not compile. So if this
            // project writes `.<name>` anywhere on a receiver the engine failed to type, and that
            // site is not already in the plan, refuse and say where.
            //
            // Deliberately keyed on the name alone. The engine cannot know whose member that site
            // meant — that is what "could not type it" means — so the only sound reading is that it
            // might be this one. Refusing costs a rename that was probably safe; not refusing costs
            // a build, silently, in a file nobody was looking at.
            // Whether the NEW name is free where it has to be. Asked of the policy resolver — the
            // one that can see the whole classpath — because the declaration a rename would collide
            // with is very often in a jar: an inherited field, a supertype method of the same arity.
            if blocked.is_none() {
                blocked = crate::conflict::member_conflict(policy, &owners, key, new_name);
            }
            if blocked.is_none() {
                let name = member_name(key);
                // Every name this rename moves — the member's own, plus what its generated
                // accessors are called.
                let mut names = vec![name.clone()];
                names.extend(also_named.iter().cloned());
                let unseen: Vec<&crate::refs::UsageLocation> = names
                    .iter()
                    .flat_map(|n| index.unresolved_named(n))
                    .filter(|u| !edits.iter().any(|e| e.file == u.file && e.start == u.start))
                    .collect();
                if let Some(first) = unseen.first() {
                    blocked = Some(format!(
                        "`{name}` is also written at {} site(s) whose receiver this engine cannot \
                         resolve — the first is {}:{}. Renaming would leave them calling a name \
                         that no longer exists.",
                        unseen.len(),
                        first.file.rsplit('/').next().unwrap_or(&first.file),
                        first.line
                    ));
                }
            }
            (member_name(key), key.label(), edits)
        }
        RenameTarget::Type { binary, .. } => {
            blocked = crate::conflict::type_conflict(policy, binary, new_name);
            let old = simple_of(binary);
            let edits = plan_type(
                index,
                binary,
                &old,
                new_name,
                java_files,
                xml_files,
                project_types,
            );
            file_rename = index
                .file_declaring(binary)
                .and_then(|decl| file_rename_for(decl, &old, new_name));
            (old, format!("type {}", binary.replace('/', ".")), edits)
        }
    };

    let mut order: Vec<String> = Vec::new();
    let mut by_file: HashMap<String, Vec<Edit>> = HashMap::new();
    let mut has_inferred = false;
    for e in edits {
        has_inferred |= e.inferred;
        if !by_file.contains_key(&e.file) {
            order.push(e.file.clone());
        }
        by_file.entry(e.file.clone()).or_default().push(e);
    }
    let files = order
        .into_iter()
        .map(|f| {
            let mut edits = by_file.remove(&f).unwrap_or_default();
            edits.sort_by_key(|e| e.start);
            FileEdits { file: f, edits }
        })
        .collect();

    Some(RenamePlan {
        old_name,
        new_name: new_name.to_string(),
        target_label: label,
        files,
        has_inferred,
        blocked,
        file_rename,
    })
}

/// The file move a type rename implies, or `None` when the file is not named after the type.
///
/// The name check is the whole test: Java ties a public top-level type to its filename, so a file
/// called `Order.java` holding `Order` must become `Bar.java` when `Order` becomes `Bar` — while a
/// nested type lives in a file named after its OUTER type, where the basename won't match and
/// nothing moves. `None` too when the new name would not change the path at all.
pub fn file_rename_for(decl_file: &str, old_simple: &str, new_name: &str) -> Option<FileRename> {
    let (dir, base) = match decl_file.rfind('/') {
        Some(i) => (&decl_file[..=i], &decl_file[i + 1..]),
        None => ("", decl_file),
    };
    let stem = base.strip_suffix(".java")?;
    if stem != old_simple || new_name == old_simple {
        return None;
    }
    Some(FileRename {
        from: decl_file.to_string(),
        to: format!("{dir}{new_name}.java"),
    })
}

/// Flatten a plan to the concrete edits the FE applies. Kept separate from the preview so
/// the two stages are distinct on the wire — the FE previews, the user confirms, the FE
/// applies. Sorted per file already.
pub fn rename_apply(plan: &RenamePlan) -> Vec<Edit> {
    plan.files
        .iter()
        .flat_map(|f| f.edits.iter().cloned())
        .collect()
}

/// Resolve the caret at `file`:`offset` to its DECLARATION site (go-to-declaration). Runs
/// the same caret classification find-usages / rename share, then returns the declaration
/// NAME span + owning project file (+ 1-based line/col from the declaring file's source).
/// The free-function core [`crate::engine::SemanticEngine::declaration`] wraps — kept separate so it's
/// testable with an in-memory resolver (no live JDK), like [`rename_plan`] / [`references`].
///
/// `None` when the caret isn't on a resolvable symbol, or the declaration lives in a JDK /
/// dep-jar (no project source in `java_files` declares it → nothing to open).
pub fn resolve_declaration(
    index: &ReferenceIndex,
    file: &str,
    source: &str,
    offset: usize,
    resolver: &dyn TypeResolver,
    project_types: &HashMap<String, String>,
    java_files: &[PlanFile],
    level: LangLevel,
) -> Option<DeclarationLocation> {
    let target = classify_target(index, file, source, offset, resolver, project_types, level)?;
    match target {
        RenameTarget::Local {
            name,
            def_start,
            def_end,
        } => {
            // A local/param declaration is in the CURRENT buffer (scope-exact).
            let (line, col) = line_col_1based(source, def_start);
            Some(DeclarationLocation {
                file: file.to_string(),
                start: def_start,
                end: def_end,
                line,
                col,
                label: format!("local `{name}`"),
            })
        }
        RenameTarget::Member { key } => {
            // Resolve the member in the file that DECLARES ITS OWNER — `declaring_owner`
            // already walked the hierarchy to the type that actually declares it. Scanning
            // every file for a same-named member (the old behaviour) jumped to a random other
            // class. `None` when the owner isn't project source (a JDK/dep type) or the span
            // isn't found there.
            let decl_file = index.file_declaring(key.owner_binary())?.to_string();
            let decl_src = project_source(java_files, &decl_file)?;
            if let Some((s, e)) = find_member_name_span(decl_src, &key) {
                let (line, col) = line_col_1based(decl_src, s);
                return Some(DeclarationLocation {
                    file: decl_file,
                    start: s,
                    end: e,
                    line,
                    col,
                    label: key.label(),
                });
            }
            // No source method with that name: a Lombok-generated accessor (`getId`/`setId`/
            // `isShipped`, a fluent `customer`) has no name token to open — redirect to the BACKING
            // FIELD it wraps. Several candidate names are tried because the accessor→field mapping
            // isn't injective (see `backing_field_candidates`); the first that is a real field wins,
            // and a candidate that isn't simply doesn't match.
            if let DeclKey::Method { owner, name } = &key {
                for field in crate::lombok::backing_field_candidates(name) {
                    let field_key = DeclKey::Field {
                        owner: owner.clone(),
                        name: field,
                    };
                    if let Some((s, e)) = find_member_name_span(decl_src, &field_key) {
                        let (line, col) = line_col_1based(decl_src, s);
                        return Some(DeclarationLocation {
                            file: decl_file,
                            start: s,
                            end: e,
                            line,
                            col,
                            label: field_key.label(),
                        });
                    }
                }
            }
            // An enum's `values()` / `valueOf(String)`: the type declares them (the compiler writes
            // them into the class file) but nothing in the source names them, so there is no member
            // token to open. The enum's own declaration is the honest destination — the same answer
            // an IDE gives, and better than the fall-through, which sent go-to into a decompiled
            // `java.lang.Enum` stub that does not declare a one-argument `valueOf` at all.
            if is_enum_implicit(&key, resolver) {
                let simple = simple_of(key.owner_binary());
                if let Some((s, e)) = find_type_name_span(decl_src, &simple) {
                    let (line, col) = line_col_1based(decl_src, s);
                    return Some(DeclarationLocation {
                        file: decl_file,
                        start: s,
                        end: e,
                        line,
                        col,
                        label: format!("enum {}", key.owner_binary().replace('/', ".")),
                    });
                }
            }
            None
        }
        RenameTarget::Type { binary, .. } => {
            // The declaring file is the one whose symbols carry this exact binary — not merely
            // a file with a same-simple-named type in another package.
            let decl_file = index.file_declaring(&binary)?.to_string();
            let decl_src = project_source(java_files, &decl_file)?;
            let simple = simple_of(&binary);
            let (s, e) = find_type_name_span(decl_src, &simple)?;
            let (line, col) = line_col_1based(decl_src, s);
            Some(DeclarationLocation {
                file: decl_file,
                start: s,
                end: e,
                line,
                col,
                label: format!("class {}", binary.replace('/', ".")),
            })
        }
    }
}

/// Whether `key` names one of the methods the compiler declares for every enum — `values()` /
/// `valueOf(String)` on a type whose own flags say `enum`. The owner check is what keeps a
/// hand-written `values()` on an ordinary class out of it.
fn is_enum_implicit(key: &DeclKey, resolver: &dyn TypeResolver) -> bool {
    let DeclKey::Method { owner, name } = key else {
        return false;
    };
    bennu_java::prelude::ENUM_IMPLICIT_METHODS.contains(&name.as_str())
        && resolver
            .members_of(owner)
            .is_some_and(|cm| cm.flags.is_enum)
}

/// The cached source text of a project java file by its (forward-slash) path.
pub(crate) fn project_source<'a>(java_files: &'a [PlanFile], file: &str) -> Option<&'a str> {
    java_files
        .iter()
        .find(|f| f.path == file)
        .map(|f| f.source.as_str())
}

/// The byte offset where the *declaration* of `key` begins in `source` (the start of the
/// `class`/`interface`/`enum`/method/field declaration node, NOT just its name token — so
/// a preceding Javadoc comment can be found immediately above it). `None` when `source`
/// doesn't declare `key`.
pub(crate) fn decl_site_for_key(source: &str, key: &DeclKey) -> Option<usize> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let bytes = source.as_bytes();
    let root = tree.root_node();

    match key {
        DeclKey::Type { binary } => {
            let simple = simple_of(binary);
            find_decl_node_start(
                &root,
                bytes,
                &[
                    "class_declaration",
                    "interface_declaration",
                    "enum_declaration",
                ],
                &simple,
                false,
            )
        }
        DeclKey::Method { name, .. } => {
            find_decl_node_start(&root, bytes, &["method_declaration"], name, false)
        }
        DeclKey::Field { name, .. } => {
            find_decl_node_start(&root, bytes, &["variable_declarator"], name, true)
        }
    }
}

/// Walk `root` for a declaration node of one of `kinds` whose `name` child matches `name`,
/// returning the node's start byte. When `want_field`, the `variable_declarator` must sit
/// under a `field_declaration` (not a local) and the reported start is the enclosing
/// `field_declaration` (so its leading Javadoc is found, not the declarator's).
fn find_decl_node_start(
    root: &Node,
    bytes: &[u8],
    kinds: &[&str],
    name: &str,
    want_field: bool,
) -> Option<usize> {
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if kinds.contains(&n.kind()) {
            if want_field {
                let is_field = n
                    .parent()
                    .map(|p| p.kind() == "field_declaration")
                    .unwrap_or(false);
                if !is_field {
                    continue;
                }
            }
            if let Some(nm) = n.child_by_field_name("name") {
                if nm.utf8_text(bytes).ok() == Some(name) {
                    // For a field, anchor on the enclosing `field_declaration` so the doc
                    // above `private int x;` (not above the bare declarator) is captured.
                    let anchor = if want_field {
                        n.parent().unwrap_or(n)
                    } else {
                        n
                    };
                    return Some(anchor.start_byte());
                }
            }
        }
    }
    None
}

/// Extract and clean the `/** … */` Javadoc block that ends immediately above the
/// declaration starting at `decl_start` in `source`. Returns the joined, trimmed doc text
/// (leading `*` and the `/**` / `*/` markers stripped, capped ~600 chars), or `None` when
/// the lines directly above the declaration aren't a Javadoc block.
pub(crate) fn leading_javadoc(source: &str, decl_start: usize) -> Option<String> {
    // Everything above the declaration. We look only at the whitespace/comment tail here —
    // a modifier keyword (`public`) between the comment and the node can't occur, since the
    // declaration node start already precedes modifiers.
    let head = &source[..decl_start];
    let trimmed = head.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    // Find the matching `/**` opening the block that this `*/` closes.
    let open = trimmed.rfind("/**")?;
    let close = trimmed.len() - "*/".len();
    if open + "/**".len() > close {
        return None; // malformed / `/**/`
    }
    let inner = &trimmed[open + "/**".len()..close];

    let mut lines: Vec<String> = Vec::new();
    for raw in inner.lines() {
        let mut l = raw.trim();
        // Strip a leading `*` (the Javadoc gutter) and one following space.
        if let Some(rest) = l.strip_prefix('*') {
            l = rest.strip_prefix(' ').unwrap_or(rest);
        }
        lines.push(l.to_string());
    }
    // Drop leading/trailing empty lines, then join.
    while lines.first().map(|s| s.is_empty()).unwrap_or(false) {
        lines.remove(0);
    }
    while lines.last().map(|s| s.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    let joined = lines.join("\n");
    let doc = joined.trim();
    if doc.is_empty() {
        return None;
    }
    Some(doc.chars().take(600).collect())
}

/// A resolved hover card for the symbol under the caret (the intel-level view the be layer
/// maps to the wire `HoverInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverInfo {
    /// The signature line: a member's `raw_signature` (or a synthesized `name(...)`
    /// fallback), or a type's dotted FQCN.
    pub signature: String,
    /// `"method"` | `"field"` | `"class"` (types are reported as `"class"`, best-effort —
    /// interface/enum aren't distinguished from the reference index).
    pub kind: String,
    /// The owning type's dotted FQCN for a member; `None` for a type.
    pub container: Option<String>,
    /// A best-effort leading Javadoc for a PROJECT declaration (the `/** … */` block
    /// immediately above it, markers stripped, capped ~600 chars). `None` for a JDK /
    /// dep-jar symbol (source not readable) or a declaration with no Javadoc.
    pub doc: Option<String>,
}

/// Build a [`HoverInfo`] for a classified [`DeclKey`], resolving a member's signature from
/// the resolver's [`bennu_java::prelude::ClassMembers`] (falling back to a synthesized
/// `name(...)` when the class isn't on the resolvable classpath or carries no signature).
pub(crate) fn hover_for_key(
    key: &DeclKey,
    resolver: &dyn TypeResolver,
    argc: Option<usize>,
) -> HoverInfo {
    match key {
        DeclKey::Type { binary } => {
            // What the type IS, not "class" for everything — an interface reported as a class
            // is the card stating something false about the thing you are pointing at. The
            // signature reads like the declaration; the package goes on the meta line.
            let simple = simple_of(binary).replace('$', ".");
            let kind = resolver
                .members_of(binary)
                .map(|cm| {
                    if cm.flags.is_annotation {
                        "annotation"
                    } else if cm.flags.is_interface {
                        "interface"
                    } else if cm.flags.is_enum {
                        "enum"
                    } else if cm.flags.is_record {
                        "record"
                    } else {
                        "class"
                    }
                })
                .unwrap_or("class");
            let dotted = binary.replace('/', ".");
            let package = dotted.rsplit_once('.').map(|(p, _)| p.to_string());
            HoverInfo {
                signature: format!("{kind} {simple}"),
                kind: kind.to_string(),
                container: package,
                doc: None,
            }
        }
        DeclKey::Method { owner, name } => {
            let found = member_signature(resolver, owner, name, true, argc);
            let (signature, declaring) =
                found.unwrap_or_else(|| (format!("{name}(…)"), owner.clone()));
            HoverInfo {
                signature,
                kind: "method".to_string(),
                // The type that DECLARES it, not the one it was reached through: hovering an
                // inherited method and being told the subclass owns it is a wrong answer.
                container: Some(declaring.replace('/', ".")),
                doc: None,
            }
        }
        DeclKey::Field { owner, name } => {
            let found = member_signature(resolver, owner, name, false, None);
            let (signature, declaring) = found.unwrap_or_else(|| (name.clone(), owner.clone()));
            HoverInfo {
                signature,
                kind: "field".to_string(),
                container: Some(declaring.replace('/', ".")),
                doc: None,
            }
        }
    }
}

/// Look up a member's `raw_signature` on `owner` (walking supertypes, like the reference
/// walk's `declaring_owner`), with the binary name of the type that actually declares it.
/// `None` when the class isn't resolvable or the member is nowhere in the hierarchy (the
/// caller then synthesizes a fallback).
///
/// `argc` is how many arguments the call site passes (or how many parameters the declaration under
/// the caret takes) — the only thing that tells two overloads apart. Without it this took the first
/// member of the name it met, and `o.customer("x")` was answered with the no-argument getter. A
/// name with one member is unaffected, which is the overwhelming majority of hovers.
fn member_signature(
    resolver: &dyn TypeResolver,
    owner: &str,
    name: &str,
    is_method: bool,
    argc: Option<usize>,
) -> Option<(String, String)> {
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![owner.to_string()];
    while let Some(bn) = stack.pop() {
        if !visited.insert(bn.clone()) {
            continue;
        }
        // A supertype we can't resolve ends THAT branch of the walk, not the whole search —
        // an un-indexed base class must not hide a member the subclass declares itself.
        let Some(cm) = resolver.members_of(&bn) else {
            continue;
        };
        let pool = if is_method { &cm.methods } else { &cm.fields };
        if let Some(m) = pick_member(pool, name, argc) {
            if !m.raw_signature.is_empty() {
                return Some((m.raw_signature.clone(), bn.clone()));
            }
            // No recorded signature: synthesize a minimal one from the name (+ empty
            // param list for a method) so the hover still shows something meaningful.
            let sig = if is_method {
                format!("{name}()")
            } else {
                name.to_string()
            };
            return Some((sig, bn.clone()));
        }
        // `cm` is a shared `Arc` — clone the (small) supertype links, don't move.
        if let Some(sc) = cm.superclass.clone() {
            stack.push(sc);
        }
        stack.extend(cm.interfaces.iter().cloned());
    }
    None
}

/// The member of `pool` named `name` that a call passing `argc` arguments would bind to.
///
/// An exact parameter count wins outright; a varargs / trailing-array method that ADMITS the count
/// is the runner-up; failing both — and whenever the caret gave no count — the first of the name, as
/// before. Only the first of these is a real answer, and the others are the honest fallbacks: a
/// hover must show something.
fn pick_member<'m>(pool: &'m [Member], name: &str, argc: Option<usize>) -> Option<&'m Member> {
    let named = || pool.iter().filter(|m| m.name == name);
    let Some(argc) = argc else { return named().next() };
    named()
        .find(|m| m.params.len() == argc)
        .or_else(|| named().find(|m| bennu_java::prelude::method_admits_argc(m, argc)))
        .or_else(|| named().next())
}

// ── local variable / parameter: scope-exact single-file ──────────────────────────

/// A local rename: its edits, plus the reason it must not be applied when the new spelling is
/// already taken in the same scope.
struct LocalPlan {
    edits: Vec<Edit>,
    capture: Option<String>,
}

fn plan_local(
    source: &str,
    file: &str,
    def_start: usize,
    def_end: usize,
    name: &str,
    new_name: &str,
) -> LocalPlan {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return LocalPlan {
            edits: Vec::new(),
            capture: None,
        };
    };
    let bytes = source.as_bytes();

    let root = tree.root_node();
    let Some(def_node) = smallest_named_covering(&root, def_start) else {
        return LocalPlan {
            edits: Vec::new(),
            capture: None,
        };
    };
    let scope = enclosing_scope(&def_node).unwrap_or(root);
    // Asked of the SAME scope the edits are collected from, so the two can never disagree about
    // what "in scope" means.
    let capture = crate::conflict::local_capture(scope, bytes, new_name);

    let mut edits = Vec::new();
    let mut stack = vec![scope];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.kind() == "identifier"
            && !crate::refs::is_member_selector_node(&n)
            && !is_callable_own_name(&n)
        {
            if let Ok(t) = n.utf8_text(bytes) {
                if t == name {
                    edits.push(Edit {
                        file: file.to_string(),
                        start: n.start_byte(),
                        end: n.end_byte(),
                        new_text: new_name.to_string(),
                        old: name.to_string(),
                        reason: if n.start_byte() == def_start && n.end_byte() == def_end {
                            EditReason::Declaration
                        } else {
                            EditReason::Local
                        },
                        inferred: false,
                    });
                }
            }
        }
    }
    LocalPlan { edits, capture }
}

/// The nearest scope node that bounds a local binding: a method/constructor body block, a
/// `for`/`enhanced_for`/`catch` clause, or a lambda body.
/// Whether `node` is the **name** of the method or constructor it belongs to.
///
/// Now that a parameter's scope is the whole declaration rather than its body, the walk passes over
/// the callable's own name. Java allows `void foo(int foo)`, and renaming the parameter there must
/// not rename the method with it.
fn is_callable_own_name(node: &Node) -> bool {
    node.parent()
        .filter(|p| matches!(p.kind(), "method_declaration" | "constructor_declaration"))
        .and_then(|p| p.child_by_field_name("name"))
        .map(|name| name.id() == node.id())
        .unwrap_or(false)
}

fn enclosing_scope<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        match n.kind() {
            // The WHOLE declaration, header included — not just the body.
            //
            // A parameter is declared in the header, so a scope that starts at the body renames
            // every use of it and leaves `final Path source_directory` untouched: uses rewritten,
            // declaration not, which does not compile. A local declared inside the body is
            // unaffected by the wider scope — Java forbids one from shadowing a parameter, so the
            // extra ground holds no other binding of the same name.
            "method_declaration" | "constructor_declaration" => return Some(n),
            "for_statement" | "enhanced_for_statement" | "catch_clause" | "lambda_expression" => {
                return Some(n);
            }
            _ => {}
        }
        cur = n.parent();
    }
    None
}

// ── field / method: declaration + cross-file references ───────────────────────────

/// What planning one member came to: its edits, and every OTHER name the rename is responsible for.
///
/// The second list is what a field's generated accessors are called — `getFoo`, `setFoo`, the
/// builder's `foo`, the `Fields` constant. They matter beyond the edits: the blindness check has to
/// ask about them too, or a call the engine could not place is missed simply because it is spelled
/// `setElenco_fase_availables` while the rename is of `elenco_fase_availables`.
struct MemberPlan {
    edits: Vec<Edit>,
    also_named: Vec<String>,
}

fn plan_member(
    index: &ReferenceIndex,
    decl_file: &str,
    decl_source: &str,
    key: &DeclKey,
    new_name: &str,
) -> MemberPlan {
    let name = member_name(key);
    let mut edits = Vec::new();

    // Every declaration of the name in the owner — overloads are several declarations of one key.
    for (ds, de) in find_member_name_spans(decl_source, key) {
        edits.push(Edit {
            file: decl_file.to_string(),
            start: ds,
            end: de,
            new_text: new_name.to_string(),
            old: name.clone(),
            reason: EditReason::Declaration,
            inferred: false,
        });
    }

    let is_method = matches!(key, DeclKey::Method { .. });
    for u in index.usages_of(key) {
        edits.push(Edit {
            file: u.file.clone(),
            start: u.start,
            end: u.end,
            new_text: new_name.to_string(),
            old: name.clone(),
            reason: EditReason::Reference,
            inferred: is_method,
        });
    }

    // Accessors nobody wrote down, but callers use — see [`generated_accessors`].
    let mut also_named = Vec::new();
    if let DeclKey::Field { owner, .. } = key {
        for accessor in generated_accessors(decl_source, owner, &name, new_name) {
            let old = member_name(&accessor.key);
            if !also_named.contains(&old) {
                also_named.push(old.clone());
            }
            for u in index.usages_of(&accessor.key) {
                edits.push(Edit {
                    file: u.file.clone(),
                    start: u.start,
                    end: u.end,
                    new_text: accessor.new_name.clone(),
                    old: old.clone(),
                    reason: EditReason::Reference,
                    inferred: false,
                });
            }
        }
    }
    MemberPlan { edits, also_named }
}

/// A generated accessor to carry along with the field it belongs to: its key in the reference
/// index, and what it must be called after the rename.
struct GeneratedAccessor {
    key: DeclKey,
    new_name: String,
}

/// The accessors that exist for `field` without anyone having written them, whose call sites a
/// rename of the field must therefore carry: a **record component's** accessor (JLS §8.10 — a
/// component declares a private final field AND a public accessor of the same name) and **Lombok's**
/// generated getter / setter / wither.
///
/// Nothing else in the plan would move them. The declaration half of a rename edits source text,
/// and for these there is no source text to edit — but `f.source_path()` and `order.getId()` are
/// written down at every call site, and they read a method that will no longer exist.
///
/// The old and new names coincide for a record component and differ for Lombok
/// (`getSource_path` → `getSourcePath`), which is why the new name comes from re-running Lombok's
/// own naming rule ([`crate::lombok::PlannedAccessor::name_for`]) rather than from a second
/// implementation of it here.
///
/// Both halves are gated on the declaring source actually declaring the thing: an ordinary class
/// may hold a field `foo` and an unrelated method `foo()`, and renaming one must never take the
/// other with it.
fn generated_accessors(
    decl_source: &str,
    owner: &str,
    field: &str,
    new_name: &str,
) -> Vec<GeneratedAccessor> {
    generated_aliases(decl_source, owner, field)
        .into_iter()
        .map(|alias| GeneratedAccessor {
            new_name: alias.rename_to(new_name),
            key: alias.key,
        })
        .collect()
}

/// A member the field is ALSO known by, without anyone having written it down.
#[derive(Debug, Clone)]
pub struct FieldAlias {
    /// The declaration key its call sites are bucketed under.
    pub key: DeclKey,
    /// How it is written at a call site — `getName()`, `withName()`, `name`.
    pub label: String,
    /// Lombok's own rule for deriving the accessor's name from the field's, when the two differ.
    /// `None` where the member simply IS the field's name (a record component, a `Fields` constant,
    /// a builder setter).
    naming: Option<crate::lombok::PlannedAccessor>,
}

impl FieldAlias {
    /// What this member is called after the field is renamed to `new_field`.
    ///
    /// Re-runs Lombok's naming rule rather than reimplementing it, which is what keeps
    /// `getSource_path` → `getSourcePath` right in a place that is not Lombok's own module.
    fn rename_to(&self, new_field: &str) -> String {
        match &self.naming {
            Some(acc) => acc.name_for(new_field),
            None => new_field.to_string(),
        }
    }
}

/// Every member `field` on `owner` is also known by — the shared answer behind two questions.
///
/// **Rename** asks it because those call sites read a method that will no longer exist.
/// **Find-usages** asks it because they are uses of the field: `order.getName()` is how the field
/// `name` is read from outside, and a class whose accessors Lombok generates has *no other kind of
/// use site* — so a usages list without them reports a field nobody touches, and the getter itself
/// has no declaration anywhere to put a caret on and ask about.
///
/// One function so the two can never disagree: whatever a rename would move, a search finds.
pub fn generated_aliases(decl_source: &str, owner: &str, field: &str) -> Vec<FieldAlias> {
    let mut out = Vec::new();
    // A record component declares a private final field AND a public accessor of the same name
    // (JLS §8.10) — one written declaration, two members.
    if declares_record_component(decl_source, owner, field) {
        out.push(FieldAlias {
            key: DeclKey::Method { owner: owner.to_string(), name: field.to_string() },
            label: format!("{field}()"),
            naming: None,
        });
    }
    let symbols = bennu_java::prelude::extract_symbols(decl_source);
    if let Some(td) = symbols
        .types
        .iter()
        .find(|t| t.fqn.replace('.', "/") == owner)
    {
        for acc in crate::lombok::accessors_of_field(td, &symbols.imports, field) {
            out.push(FieldAlias {
                key: DeclKey::Method {
                    owner: owner.to_string(),
                    name: acc.name.clone(),
                },
                label: format!("{}()", acc.name),
                naming: Some(acc),
            });
        }
        // The two nested TYPES Lombok generates each hold one member per field, named exactly like
        // the field — `Dto.Fields.file_name` (a constant) and `Dto.builder().file_name(x)` (a
        // setter on the builder). Both are the field under another name, and a rename that moves
        // one without the others leaves code that does not compile.
        let generated = crate::lombok::generated_type_names(td, &symbols.imports);
        if let Some(fields_type) = generated.field_constants {
            out.push(FieldAlias {
                key: DeclKey::Field {
                    owner: format!("{owner}/{fields_type}"),
                    name: field.to_string(),
                },
                label: format!("{fields_type}.{field}"),
                naming: None,
            });
        }
        if let Some(builder_type) = generated.builder {
            out.push(FieldAlias {
                key: DeclKey::Method {
                    owner: format!("{owner}/{builder_type}"),
                    name: field.to_string(),
                },
                label: format!("{builder_type}.{field}()"),
                naming: None,
            });
        }
    }
    out
}

/// Whether `source` declares `name` as a component of the record whose binary name is `owner`.
fn declares_record_component(source: &str, owner: &str, name: &str) -> bool {
    let simple = simple_of(owner);
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return false;
    };
    let bytes = source.as_bytes();

    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.kind() != "record_declaration" {
            continue;
        }
        if n.child_by_field_name("name")
            .and_then(|nm| nm.utf8_text(bytes).ok())
            != Some(&simple)
        {
            continue;
        }
        let Some(params) = n.child_by_field_name("parameters") else {
            continue;
        };
        let mut pc = params.walk();
        for p in params.named_children(&mut pc) {
            let component = p
                .child_by_field_name("name")
                .and_then(|nm| nm.utf8_text(bytes).ok());
            if component == Some(name) {
                return true;
            }
        }
    }
    false
}

/// Whether this `formal_parameter` is a record component rather than a method parameter — the
/// grammar spells both the same way, and only the grandparent tells them apart.
fn is_record_component(node: &Node) -> bool {
    node.parent()
        .and_then(|params| params.parent())
        .map(|owner| owner.kind() == "record_declaration")
        .unwrap_or(false)
}

/// Find the byte span of a member declaration's NAME token in `source`. Shared by rename
/// (the declaration edit site) and go-to-declaration (the navigation target). `None` for a
/// [`DeclKey::Type`] (use [`bennu_java::prelude::find_type_name_span`]) or when `source` doesn't
/// declare the member.
pub fn find_member_name_span(source: &str, key: &DeclKey) -> Option<(usize, usize)> {
    find_member_name_spans(source, key).into_iter().next()
}

/// Every byte span in `source` where this member's name is DECLARED.
///
/// More than one for overloads: `foo(int)` and `foo(String)` are two declarations of one name, and
/// the reference index collapses them to a single key. Renaming from that key has to move both —
/// stopping at the first (which is what returning a single span meant) renamed one overload and
/// left the other declaring the old name, silently splitting one method into two.
pub fn find_member_name_spans(source: &str, key: &DeclKey) -> Vec<(usize, usize)> {
    let (name, want_field) = match key {
        DeclKey::Method { name, .. } => (name, false),
        DeclKey::Field { name, .. } => (name, true),
        DeclKey::Type { .. } => return Vec::new(),
    };
    let owner_simple = simple_of(key.owner_binary());
    // A declaration's name appears textually in the file that declares it — skip the
    // tree-sitter parse when the token isn't even present (a cheap early-out for callers that
    // probe more than one file, e.g. rename's edit-site search). A substring false-positive
    // just parses one file that then yields no match — correct, only slightly slower.
    if !source.contains(name.as_str()) {
        return Vec::new();
    }
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    // What this FILE says each of its types extends / implements, by simple name. An anonymous
    // class names the type it instantiates, which may be a SUB-type of the one whose member is
    // being renamed (`new Public() { … }` for a member declared on `Download`), and the file that
    // declares both is the one place that relationship is written down.
    let supertypes = file_supertypes(source);

    let mut found: Vec<(usize, usize)> = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        let hit = match n.kind() {
            "method_declaration" if !want_field => n.child_by_field_name("name"),
            // An `@interface` element (`boolean withCheck() default false;`) is a method — that is
            // what it compiles to and how the index records it. Its declaration lives under its own
            // node kind, so without this arm the member had uses and no declaration, and the rename
            // refused itself rather than edit half a pair.
            "annotation_type_element_declaration" if !want_field => n.child_by_field_name("name"),
            "variable_declarator" if want_field => {
                // A declarator under a `field_declaration` (a class field) OR a
                // `constant_declaration` (an interface's `int MAX = …;`) is the field decl.
                let is_field = n
                    .parent()
                    .map(|p| matches!(p.kind(), "field_declaration" | "constant_declaration"))
                    .unwrap_or(false);
                if is_field {
                    n.child_by_field_name("name")
                } else {
                    None
                }
            }
            // A record component IS the field's declaration — the JLS says the header declares a
            // `private final` field, and there is nowhere else in the source it is written.
            "formal_parameter" if want_field && is_record_component(&n) => {
                n.child_by_field_name("name")
            }
            _ => None,
        };
        if let Some(nm) = hit {
            if nm.utf8_text(bytes).ok() == Some(name.as_str())
                && declared_in_type(&n, bytes, &owner_simple, &supertypes)
            {
                found.push((nm.start_byte(), nm.end_byte()));
            }
        }
    }
    // The walk is a stack, so it arrives in no useful order; source order is what a preview wants.
    // Deduplicated because one span can now be claimed twice — an anonymous class's override is a
    // declaration of the interface's member AND of its own. `rename_plan` also dedups across the
    // override family, but this function is public and its other callers (go-to-declaration) get
    // no such pass.
    found.sort_unstable();
    found.dedup();
    found
}

/// Whether the declaration at `node` sits inside the type named `owner_simple`.
///
/// One file can declare several types — a nested class, a second top-level one — and any of them
/// may hold a member of the same name. Without this the span search took whichever the tree walk
/// reached first, so a rename could edit an unrelated member of an unrelated type.
///
/// A declaration with no enclosing named type is accepted: an anonymous class body or a shape the
/// grammar spells differently should not silently lose its declaration edit, and the caller is
/// already looking in the file that declares the owner.
fn declared_in_type(
    node: &Node,
    bytes: &[u8],
    owner_simple: &str,
    supertypes: &HashMap<String, Vec<String>>,
) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        // An anonymous body is the enclosing type, and the first one going up. Climbing past it
        // would compare the member against the name of the class the `new` sits in — which never
        // matches the anonymous owner's synthetic name, so no declaration span was ever found for
        // a member of one.
        //
        // Two names can match here, and both have to. The anonymous class's OWN name is an ordinal
        // (`"1"`) — that is the owner when the rename started from a member of the anonymous body
        // itself. The type it implements is the owner when the rename started from the INTERFACE:
        // `new Checker() { @Override public String create_attachment(…) }` declares an override
        // that must move with `Checker.create_attachment`, and matching only the ordinal meant it
        // never did — leaving a class that no longer overrides what it claims to.
        if bennu_java::prelude::is_anonymous_body(&n) {
            let own = bennu_java::prelude::anonymous_type_name(&n, bytes);
            if own.as_deref() == Some(owner_simple) {
                return true;
            }
            let Some(implements) = bennu_java::prelude::anonymous_supertype_name(&n, bytes) else {
                return false;
            };
            // The type it instantiates, or anything that type extends: `new Public() { … }`
            // overrides a member `Public` inherits from `Download`, and the declaration being
            // renamed is `Download`'s.
            return implements == owner_simple || reaches(supertypes, &implements, owner_simple);
        }
        if matches!(
            n.kind(),
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration"
        ) {
            return match n
                .child_by_field_name("name")
                .and_then(|nm| nm.utf8_text(bytes).ok())
            {
                Some(found) => found == owner_simple,
                None => true,
            };
        }
        cur = n.parent();
    }
    true
}

/// Each type this FILE declares, mapped to the simple names it extends / implements.
///
/// Only what the file itself says: enough for an anonymous class to be related to the interface
/// whose static factory builds it, which is where the pair is written together. A supertype
/// declared elsewhere is not reachable here, and the caller's other routes cover those.
fn file_supertypes(source: &str) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for td in bennu_java::prelude::extract_symbols(source).types {
        let mut parents: Vec<String> = Vec::new();
        if let Some(ext) = &td.extends {
            parents.push(simple_written_name(ext));
        }
        parents.extend(td.implements.iter().map(|i| simple_written_name(i)));
        out.insert(td.name.clone(), parents);
    }
    out
}

/// `p.Outer.Inner<T>` → `Inner`: the simple name of a type as WRITTEN in an extends/implements
/// clause.
fn simple_written_name(text: &str) -> String {
    let bare = text.split('<').next().unwrap_or(text).trim();
    bare.rsplit('.').next().unwrap_or(bare).trim().to_string()
}

/// Whether `from` reaches `target` through the file's own extends/implements edges.
fn reaches(graph: &HashMap<String, Vec<String>>, from: &str, target: &str) -> bool {
    /// A file whose declarations form a cycle is malformed; stop rather than loop.
    const MAX_DEPTH: usize = 32;
    let mut seen: Vec<&str> = vec![from];
    let mut queue: Vec<&str> = vec![from];
    let mut steps = 0;
    while let Some(cur) = queue.pop() {
        steps += 1;
        if steps > MAX_DEPTH {
            return false;
        }
        let Some(parents) = graph.get(cur) else {
            continue;
        };
        for p in parents {
            if p == target {
                return true;
            }
            if !seen.contains(&p.as_str()) {
                seen.push(p);
                queue.push(p);
            }
        }
    }
    false
}

/// 1-based `(line, col)` of byte `start` in `source`: line = 1 + count of `'\n'` before
/// `start`; col = 1 + bytes since the last `'\n'` (or from the start of the file). Byte
/// columns (not char columns) — the FE maps against the same byte buffer.
fn line_col_1based(source: &str, start: usize) -> (u32, u32) {
    let clamped = start.min(source.len());
    let head = &source.as_bytes()[..clamped];
    let line = 1 + head.iter().filter(|&&b| b == b'\n').count() as u32;
    let col = match head.iter().rposition(|&b| b == b'\n') {
        Some(i) => (clamped - (i + 1)) as u32 + 1,
        None => clamped as u32 + 1,
    };
    (line, col)
}

// ── class / interface: decl + refs + imports + Spring bean XML ────────────────────

/// One type rename in a batch — see [`plan_types`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRename {
    /// The type's binary name (`com/acme/OrderService`).
    pub binary: String,
    /// The new SIMPLE name. The package is kept.
    pub new_name: String,
}

/// Plan several type renames in **one pass** over the project's sources.
///
/// The declaration name and the `import` statements can only be found by reading every file, and
/// reading means parsing. Done one rename at a time that is `renames × files` parses — which is
/// invisible for a single Shift+F6 and is minutes on a bulk fix over a legacy tree, with the
/// caller's request blocked for all of it. Here each file is parsed **once** and matched against
/// every rename in the batch, so the cost is `files`, whatever the batch holds.
///
/// Returns one bucket of edits per input rename, in the same order, so a caller that has to
/// attribute an edit back to the name that caused it still can.
///
/// `on_file(done, total)` is called as the pass advances, and **returning `false` stops it**.
///
/// Both halves of that exist for the same reason: this is the slow step, and from the outside it is
/// one indivisible call. Without progress, a caller can only report "started" and "finished" — on a
/// large project that is a full progress bar sitting still, indistinguishable from a hang. Without
/// the stop, a Cancel button elsewhere does nothing at all for the whole time this runs, which is
/// worse than not offering one.
///
/// A stopped pass returns `(buckets, false)`. Those buckets are **incomplete and must not be
/// applied**: the reference edits come from the index but the declaration and `import` edits come
/// from this walk, so a half-done pass renames the call sites of a type and leaves its declaration
/// alone. The caller is told so it can discard them, not so it can use what it got.
pub fn plan_types(
    index: &ReferenceIndex,
    renames: &[TypeRename],
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    project_types: &HashMap<String, String>,
    on_file: &dyn Fn(usize, usize) -> bool,
) -> (Vec<Vec<Edit>>, bool) {
    let mut out: Vec<Vec<Edit>> = vec![Vec::new(); renames.len()];
    if renames.is_empty() {
        return (out, true);
    }
    let targets: Vec<TypeTarget> = renames.iter().map(TypeTarget::of).collect();

    // (1) simple-name use sites (the reference index buckets these under DeclKey::Type; the
    // builder EXCLUDES the declaration name, added separately in (2)). An index lookup per
    // rename — cheap, and unrelated to how many files the project has.
    for (i, target) in targets.iter().enumerate() {
        for u in index.usages_of(&DeclKey::Type {
            binary: target.binary.clone(),
        }) {
            out[i].push(Edit {
                file: u.file.clone(),
                start: u.start,
                end: u.end,
                new_text: target.new_name.clone(),
                old: target.old_simple.clone(),
                reason: EditReason::Reference,
                inferred: false,
            });
        }
    }

    // (2) the declaration name + import statements. ONE parse per file, matched against the
    // whole batch — this is the loop the batching exists for, and the one worth reporting on.
    let total = java_files.len();
    for (done, f) in java_files.iter().enumerate() {
        if !on_file(done, total) {
            return (out, false);
        }
        collect_type_decls_and_imports(&f.source, &f.path, &targets, project_types, &mut out);
    }
    on_file(total, total);

    // (3) Spring bean XML `<bean class="oldFQCN">` → rewrite the FQCN (package kept). A string
    // scan over a handful of config files, so it stays per-rename.
    for f in xml_files {
        for (i, target) in targets.iter().enumerate() {
            for span in bean_class_value_spans(&f.source, &target.old_fqcn) {
                out[i].push(Edit {
                    file: f.path.clone(),
                    start: span.start,
                    end: span.end,
                    new_text: target.new_fqcn.clone(),
                    old: target.old_fqcn.clone(),
                    reason: EditReason::SpringBean,
                    inferred: false,
                });
            }
        }
    }

    // One byte range, one edit. The references come from the index and the declaration from the
    // walk above, and the two agree only as long as the index correctly excludes declaration names
    // from the usages it buckets. That is a property of another module, and if it ever slips the
    // failure here is two splices over the same bytes — corruption, not a missing rename. Deduping
    // costs a sort per bucket and removes the coupling.
    // Ordered so the DECLARATION sorts first within an identical range, because `dedup_by` keeps
    // the first: the two candidates replace the same bytes with the same text, but a caller may
    // well check that a plan contains a declaration edit before trusting it, and dropping that one
    // in favour of a reference would fail such a check on a plan that is in fact complete.
    for bucket in &mut out {
        bucket.sort_by(|a, b| {
            (a.file.as_str(), a.start, a.end, reason_rank(&a.reason)).cmp(&(
                b.file.as_str(),
                b.start,
                b.end,
                reason_rank(&b.reason),
            ))
        });
        bucket.dedup_by(|a, b| a.file == b.file && a.start == b.start && a.end == b.end);
    }

    (out, true)
}

/// Every PROJECT type whose declaration of `name` must move when `owner`'s does — `owner` itself,
/// the supertypes it overrides the method from, and every subtype that overrides it.
///
/// A method that exists at several levels of a hierarchy is ONE method to every caller: renaming
/// `Base.doWork` while leaving `Impl.doWork` behind doesn't rename anything, it silently turns an
/// override into an unrelated method and an `@Override` into a compile error. And because the
/// reference index keys a call by the owner its receiver resolved to, calls made through an
/// `Impl`-typed variable live under `Impl`'s key — so missing the subtype loses ordinary-looking
/// call sites too, which is what "some usages were renamed and some weren't" looks like from
/// outside.
///
/// Only project types: library source can't be edited, and a family rooted at a JDK type
/// (everything "declares" `toString`) would drag in every unrelated class in the project.
pub(crate) fn override_family(
    resolver: &dyn TypeResolver,
    subtypes: &SubtypeMap,
    owner: &str,
    name: &str,
) -> Vec<String> {
    // A CLOSURE, not a walk up followed by a walk down.
    //
    // The connection between two declarations of one method is not always a straight line through
    // the starting type. A superclass method can satisfy an interface **on behalf of a subclass**:
    // `AbstractEmptyIterator.hasPrevious()` implements `OrderedIterator.hasPrevious()` for
    // `EmptyOrderedIterator`, which sits below both. Reaching the interface from the superclass
    // means going DOWN to the subclass and then UP — a step that "collect the roots, then descend
    // from them" cannot take, and Commons Collections is built this way throughout. Each side was
    // then renamed on its own, and a class stopped implementing the interface it declares.
    //
    // Iterating to a fixpoint also makes the family independent of WHERE the rename started, which
    // is what lets one refusal cover all of it: the library-override guard is asked of every
    // member, so a rename refused from the interface is refused from the implementor too.
    let mut family: Vec<String> = vec![owner.to_string()];
    let mut queue: Vec<String> = vec![owner.to_string()];
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(cur) = queue.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        // Up: a project supertype that declares the same name is the same method.
        for anc in project_ancestors(resolver, &cur) {
            if declares_method(resolver, &anc, name) && !family.contains(&anc) {
                family.push(anc.clone());
                queue.push(anc);
            }
        }
        // Down: every subtype, whether or not IT declares the name — a subtype that does not
        // declare it is still the path by which two that do are related.
        for sub in subtypes.children(&cur) {
            if declares_method(resolver, sub, name) && !family.contains(sub) {
                family.push(sub.clone());
            }
            if !seen.contains(sub) {
                queue.push(sub.clone());
            }
        }
    }
    family
}

/// Who directly extends or implements whom, across the whole project.
///
/// Built from the index's own list of declared types — the one place that knows them all, ANONYMOUS
/// classes included. That last part is not a detail: an anonymous class is where most overrides of a
/// callback interface actually live, and it has no name to look up, so every search that goes by
/// name is blind to it. Here it is an ordinary subtype of the interface it was written against, and
/// an override family finds it the same way it finds any other.
///
/// ## It has to keep up with an edit
///
/// It used to be built once and left, which is a worse kind of stale than a missing search result.
/// A method rename carries its whole override family, and the family is read from here — so a class
/// that started implementing an interface *this session* was not in it. The rename moved the
/// interface's method and every implementation the map knew about, and left that one declaring the
/// old name: a class that no longer overrides what it says it does, produced by a refactor whose
/// whole promise is that it does not do that.
///
/// So a type is re-filed by [`Self::refresh_type`] when its file is re-read, which is why `parents`
/// exists — see the field.
#[derive(Default)]
pub struct SubtypeMap {
    children: HashMap<String, Vec<String>>,
    /// The parents each type is currently filed under — the inverse of `children`.
    ///
    /// Kept so a type whose supertypes changed can be taken out of exactly the lists it is in.
    /// Without it, re-filing one type means scanning every list in the project, which is the whole
    /// map — and the map would then only be affordable to rebuild wholesale, which is what left it
    /// stale for the rest of the session in the first place.
    parents: HashMap<String, Vec<String>>,
}

impl SubtypeMap {
    /// Invert the supertype links of every project type.
    pub(crate) fn build(index: &ReferenceIndex, resolver: &dyn TypeResolver) -> Self {
        let mut map = Self::default();
        for binary in index.project_type_binaries() {
            map.refresh_type(&binary, resolver);
        }
        map
    }

    /// Re-file `binary` under whatever the resolver now says its supertypes are.
    ///
    /// Withdraw-then-file, so changing `extends A` to `extends B` moves it rather than filing it
    /// under both. A type the resolver cannot see is simply withdrawn: it is either gone, or not
    /// resolvable, and in both cases claiming it is a subtype of anything would be an invention.
    pub(crate) fn refresh_type(&mut self, binary: &str, resolver: &dyn TypeResolver) {
        self.withdraw_type(binary);
        let Some(cm) = resolver.members_of(binary) else { return };
        let mut filed: Vec<String> = Vec::new();
        for parent in cm.superclass.iter().chain(cm.interfaces.iter()) {
            let list = self.children.entry(parent.clone()).or_default();
            // Inserted in place rather than pushed-and-sorted: the list stays ordered (so an
            // override family descends deterministically) and stays deduplicated, without a sort
            // per insertion — which on a widely-implemented interface would be one sort of a
            // growing list per implementor, all through the initial build.
            if let Err(at) = list.binary_search_by(|c| c.as_str().cmp(binary)) {
                list.insert(at, binary.to_string());
            }
            if !filed.iter().any(|p| p == parent) {
                filed.push(parent.clone());
            }
        }
        if !filed.is_empty() {
            self.parents.insert(binary.to_string(), filed);
        }
    }

    /// Take `binary` out of every list it is in.
    pub(crate) fn withdraw_type(&mut self, binary: &str) {
        let Some(was) = self.parents.remove(binary) else { return };
        for parent in was {
            let Some(list) = self.children.get_mut(&parent) else { continue };
            list.retain(|c| c != binary);
            if list.is_empty() {
                self.children.remove(&parent);
            }
        }
    }

    pub(crate) fn children(&self, binary: &str) -> &[String] {
        self.children
            .get(binary)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// The LIBRARY type whose method `name` this owner overrides, if any.
///
/// A method that implements a library interface or overrides a library base class is not free to be
/// renamed: the name IS the contract, and the jar cannot be edited to follow. Renaming it produces
/// a class that no longer implements what it claims to — `@Override` stops overriding and the file
/// stops compiling — which is exactly the outcome a rename must never quietly produce.
///
/// Matched by NAME, like everything else in this engine (overloads collapse to one key). That can
/// name a library supertype whose same-named method takes different parameters and is therefore
/// not really overridden — refusing there costs a rename that would have been safe, which is the
/// cheaper of the two mistakes.
fn library_override(resolver: &dyn TypeResolver, owner: &str, name: &str) -> Option<String> {
    all_ancestors(resolver, owner)
        .into_iter()
        .find(|a| !resolver.is_project_type(a) && declares_method(resolver, a, name))
}

/// Whether `binary` declares a method called `name` **itself** (not inherited).
fn declares_method(resolver: &dyn TypeResolver, binary: &str, name: &str) -> bool {
    resolver
        .members_of(binary)
        .map(|cm| cm.methods.iter().any(|m| m.name == name))
        .unwrap_or(false)
}

/// The PROJECT supertypes of `binary`, superclass and interfaces. Library supertypes end the walk
/// on that branch — nothing above them can be edited, so nothing above them joins a rename.
fn project_ancestors(resolver: &dyn TypeResolver, binary: &str) -> Vec<String> {
    ancestors(resolver, binary, true)
}

/// Every supertype of `binary`, project and library alike — for deciding whether a rename is
/// allowed at all, which depends on supertypes that cannot be edited.
fn all_ancestors(resolver: &dyn TypeResolver, binary: &str) -> Vec<String> {
    ancestors(resolver, binary, false)
}

/// Supertypes of `binary`, superclass and interfaces. With `project_only`, a library supertype is
/// neither returned nor walked through.
fn ancestors(resolver: &dyn TypeResolver, binary: &str, project_only: bool) -> Vec<String> {
    /// A hierarchy this deep is a cycle in a malformed index, not a real one.
    const MAX_DEPTH: usize = 64;
    let mut out: Vec<String> = Vec::new();
    let mut queue: Vec<String> = vec![binary.to_string()];
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(binary.to_string());
    let mut steps = 0usize;
    while let Some(next) = queue.pop() {
        steps += 1;
        if steps > MAX_DEPTH {
            break;
        }
        let Some(cm) = resolver.members_of(&next) else {
            continue;
        };
        let supers = cm
            .superclass
            .iter()
            .cloned()
            .chain(cm.interfaces.iter().cloned());
        for s in supers {
            if (project_only && !resolver.is_project_type(&s)) || !seen.insert(s.clone()) {
                continue;
            }
            out.push(s.clone());
            queue.push(s);
        }
    }
    out
}

/// Sort order for two edits over the same bytes: the declaration wins.
fn reason_rank(reason: &EditReason) -> u8 {
    match reason {
        EditReason::Declaration => 0,
        _ => 1,
    }
}

/// A [`TypeRename`] with the spellings the walk compares against precomputed.
struct TypeTarget {
    binary: String,
    old_simple: String,
    old_fqcn: String,
    new_name: String,
    new_fqcn: String,
}

impl TypeTarget {
    fn of(rename: &TypeRename) -> TypeTarget {
        let old_fqcn = rename.binary.replace('/', ".");
        TypeTarget {
            binary: rename.binary.clone(),
            old_simple: simple_of(&rename.binary),
            new_fqcn: replace_simple(&old_fqcn, &rename.new_name),
            old_fqcn,
            new_name: rename.new_name.clone(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_type(
    index: &ReferenceIndex,
    binary: &str,
    _old_simple: &str,
    new_name: &str,
    java_files: &[PlanFile],
    xml_files: &[PlanFile],
    project_types: &HashMap<String, String>,
) -> Vec<Edit> {
    // A batch of one. Keeping a second implementation for the single case is how the two would
    // eventually disagree about what renaming a type means.
    let renames = [TypeRename {
        binary: binary.to_string(),
        new_name: new_name.to_string(),
    }];
    // One rename, one pass, nothing to report and nothing to stop.
    let (buckets, _) = plan_types(
        index,
        &renames,
        java_files,
        xml_files,
        project_types,
        &|_, _| true,
    );
    buckets.into_iter().next().unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
/// One file, every rename in the batch. Parses once and pushes into each rename's bucket.
fn collect_type_decls_and_imports(
    source: &str,
    path: &str,
    targets: &[TypeTarget],
    project_types: &HashMap<String, String>,
    out: &mut [Vec<Edit>],
) {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return;
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();

    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        match n.kind() {
            // Every kind that declares a type. `record_declaration` and
            // `annotation_type_declaration` were missing here, and the failure mode is the worst
            // one a rename has: the *uses* come from the reference index and were all rewritten,
            // while the declaration — which only this walk can find — was left alone. The result
            // is code that no longer compiles, produced by a refactor that reported success.
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
                let Some(nm) = n.child_by_field_name("name") else {
                    continue;
                };
                let Ok(text) = nm.utf8_text(bytes) else {
                    continue;
                };
                for (i, target) in targets.iter().enumerate() {
                    if text != target.old_simple {
                        continue;
                    }
                    // Confirm this really is the target type (same binary) so a same-named
                    // class in another package isn't hit.
                    let is_target = project_types
                        .get(&target.old_simple)
                        .map(|b| *b == target.binary)
                        .unwrap_or(true);
                    if is_target {
                        out[i].push(Edit {
                            file: path.to_string(),
                            start: nm.start_byte(),
                            end: nm.end_byte(),
                            new_text: target.new_name.clone(),
                            old: target.old_simple.clone(),
                            reason: EditReason::Declaration,
                            inferred: false,
                        });
                    }
                }
            }
            // A CONSTRUCTOR is spelled with its type's name and nothing else. Left behind by a type
            // rename it stops being a constructor — javac says "invalid method declaration; return
            // type required" — so the declaration walk has to move it with the type.
            //
            // Scoped to the type that actually declares it: one file holds several types, each with
            // its own constructors, and only the renamed type's may move.
            "constructor_declaration" => {
                let Some(nm) = n.child_by_field_name("name") else {
                    continue;
                };
                let Ok(text) = nm.utf8_text(bytes) else {
                    continue;
                };
                for (i, target) in targets.iter().enumerate() {
                    if text != target.old_simple {
                        continue;
                    }
                    if !declared_in_type(&n, bytes, &target.old_simple, &HashMap::new()) {
                        continue;
                    }
                    out[i].push(Edit {
                        file: path.to_string(),
                        start: nm.start_byte(),
                        end: nm.end_byte(),
                        new_text: target.new_name.clone(),
                        old: target.old_simple.clone(),
                        reason: EditReason::Declaration,
                        inferred: false,
                    });
                }
            }
            "import_declaration" => {
                let Some(pn) = n
                    .named_children(&mut n.walk())
                    .find(|c| matches!(c.kind(), "scoped_identifier" | "identifier"))
                else {
                    continue;
                };
                let Ok(text) = pn.utf8_text(bytes) else {
                    continue;
                };
                for (i, target) in targets.iter().enumerate() {
                    if text != target.old_fqcn {
                        continue;
                    }
                    // Replace only the trailing simple name (after the final `.`).
                    let path_end = pn.end_byte();
                    let simple_start = path_end - target.old_simple.len();
                    out[i].push(Edit {
                        file: path.to_string(),
                        start: simple_start,
                        end: path_end,
                        new_text: target.new_name.clone(),
                        old: target.old_simple.clone(),
                        reason: EditReason::Import,
                        inferred: false,
                    });
                }
            }
            _ => {}
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────────

fn member_name(key: &DeclKey) -> String {
    match key {
        DeclKey::Method { name, .. } | DeclKey::Field { name, .. } => name.clone(),
        DeclKey::Type { binary } => simple_of(binary),
    }
}

fn simple_of(binary: &str) -> String {
    binary.rsplit('/').next().unwrap_or(binary).to_string()
}

/// Replace the trailing simple name of a dotted FQCN (`com.x.Foo` + `Bar` → `com.x.Bar`).
fn replace_simple(fqcn: &str, new_simple: &str) -> String {
    match fqcn.rfind('.') {
        Some(i) => format!("{}.{}", &fqcn[..i], new_simple),
        None => new_simple.to_string(),
    }
}

fn smallest_named_covering<'t>(root: &Node<'t>, offset: usize) -> Option<Node<'t>> {
    let probe = offset;
    let mut best: Option<Node> = None;
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let mut cur = n.walk();
        for c in n.named_children(&mut cur) {
            stack.push(c);
        }
        if n.start_byte() <= probe && probe < n.end_byte() && n.is_named() {
            match &best {
                Some(b) if (b.end_byte() - b.start_byte()) <= (n.end_byte() - n.start_byte()) => {}
                _ => best = Some(n),
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::{build_reference_index, SourceFile};
    use bennu_java::prelude::{
        extract_symbols, ClassMembers, Import, Member, MemberKind, TypeRef, TypeResolver,
        Visibility,
    };

    struct SrcResolver {
        project: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    fn build_resolver(files: &[(&str, &str)]) -> (SrcResolver, HashMap<String, String>) {
        let mut project_types: HashMap<String, String> = HashMap::new();
        for (_p, s) in files {
            for td in extract_symbols(s).types {
                project_types.insert(td.name.clone(), td.fqn.replace('.', "/"));
            }
        }
        let mut project = HashMap::new();
        for (_p, s) in files {
            for td in &extract_symbols(s).types {
                let binary = td.fqn.replace('.', "/");
                let methods = td
                    .methods
                    .iter()
                    .map(|m| Member {
                        name: m.name.clone(),
                        kind: MemberKind::Method,
                        return_type: TypeRef {
                            binary_name: String::new(),
                            type_args: vec![],
                        },
                        params: vec![],
                        is_static: m.is_static,
                        is_abstract: false,
                        is_default: false,
                        is_final: m.is_final,
                        visibility: Visibility::Public,
                        raw_signature: String::new(),
                        throws: Vec::new(),
                        annotations: Vec::new(),
                    })
                    .collect();
                project.insert(
                    binary,
                    ClassMembers {
                        type_params: Vec::new(),
                        superclass: None,
                        interfaces: vec![],
                        methods,
                        fields: vec![],
                        flags: Default::default(),
                    },
                );
            }
        }
        let mut simple = project_types.clone();
        simple.insert("String".into(), "java/lang/String".into());
        (SrcResolver { project, simple }, project_types)
    }

    impl TypeResolver for SrcResolver {
        fn members_of(&self, binary: &str) -> Option<std::sync::Arc<ClassMembers>> {
            self.project.get(binary).cloned().map(std::sync::Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String> {
            for imp in imports {
                if imp.simple_name() == Some(name) {
                    return Some(imp.path.replace('.', "/"));
                }
            }
            self.simple.get(name).cloned()
        }
    }

    fn plan(
        files: &[(&str, &str)],
        xml: &[(&str, &str)],
        target_file: &str,
        offset: usize,
        new_name: &str,
    ) -> Option<RenamePlan> {
        let (resolver, project_types) = build_resolver(files);
        let src: Vec<SourceFile> = files
            .iter()
            .map(|(p, s)| SourceFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        let java_files: Vec<PlanFile> = files
            .iter()
            .map(|(p, s)| PlanFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let xml_files: Vec<PlanFile> = xml
            .iter()
            .map(|(p, s)| PlanFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let source = files.iter().find(|(p, _)| *p == target_file).unwrap().1;
        let subtypes = SubtypeMap::build(&index, &resolver);
        rename_plan(
            &index,
            target_file,
            source,
            offset,
            new_name,
            &resolver,
            &resolver,
            &project_types,
            &subtypes,
            &java_files,
            &xml_files,
            LangLevel(0),
        )
    }

    fn decl(
        files: &[(&str, &str)],
        target_file: &str,
        offset: usize,
    ) -> Option<DeclarationLocation> {
        let (resolver, project_types) = build_resolver(files);
        let src: Vec<SourceFile> = files
            .iter()
            .map(|(p, s)| SourceFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let index = build_reference_index(&src, &resolver, &project_types);
        let java_files: Vec<PlanFile> = files
            .iter()
            .map(|(p, s)| PlanFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let source = files.iter().find(|(p, _)| *p == target_file).unwrap().1;
        resolve_declaration(
            &index,
            target_file,
            source,
            offset,
            &resolver,
            &project_types,
            &java_files,
            LangLevel(0),
        )
    }

    #[test]
    fn declaration_method_from_call_resolves_to_decl_name() {
        // A bare call inside the SAME class keys to its enclosing owner (p/A) — no receiver
        // inference needed, so this exercises the member-decl path deterministically.
        let src =
            "package p; public class A { int compute() { return 1; } int caller() { return compute(); } }";
        let files = [("A.java", src)];
        let off = src.rfind("compute()").unwrap() + 2; // caret inside the CALL
        let d = decl(&files, "A.java", off).expect("resolved");
        assert_eq!(d.file, "A.java");
        let decl_off = src.find("compute()").unwrap(); // the DECLARATION name
        assert_eq!((d.start, d.end), (decl_off, decl_off + "compute".len()));
        assert_eq!(d.label, "method p.A.compute()");
    }

    #[test]
    fn declaration_field_from_decl_name() {
        // Field with an initializer so the `variable_declarator` span exceeds the name span
        // and the classifier lands on the name identifier (the bare `int x;` tie is an
        // upstream classifier edge, not this feature's concern).
        let src = "package p; public class A { int count = 0; int m() { return count; } }";
        let files = [("A.java", src)];
        let off = src.find("count").unwrap() + 2; // caret on the field DECL name
        let d = decl(&files, "A.java", off).expect("resolved");
        assert_eq!(d.file, "A.java");
        let decl_off = src.find("count").unwrap();
        assert_eq!((d.start, d.end), (decl_off, decl_off + "count".len()));
        assert_eq!(d.label, "field p.A.count");
    }

    #[test]
    fn declaration_local_var_is_current_file() {
        let src = "package p; public class C { int f() { int x = 1; return x + x; } }";
        let files = [("C.java", src)];
        let off = src.rfind("x + x").unwrap() + 1; // caret on a USE of `x`
        let d = decl(&files, "C.java", off).expect("resolved");
        assert_eq!(d.file, "C.java");
        let decl_off = src.find("x = 1").unwrap(); // the declarator name
        assert_eq!((d.start, d.end), (decl_off, decl_off + 1));
        assert_eq!(d.label, "local `x`");
    }

    #[test]
    fn declaration_param_resolves_to_param_name() {
        let src = "package p; public class C { int f(int y) { return y * 2; } }";
        let files = [("C.java", src)];
        let off = src.rfind("y * 2").unwrap() + 1;
        let d = decl(&files, "C.java", off).expect("resolved");
        let decl_off = src.find("int y)").unwrap() + "int ".len();
        assert_eq!((d.start, d.end), (decl_off, decl_off + 1));
        assert_eq!(d.label, "local `y`");
    }

    #[test]
    fn declaration_type_from_cross_file_reference() {
        let widget = ("Widget.java", "package com.acme; public class Widget { }");
        let consumer = (
            "Consumer.java",
            "package com.app; import com.acme.Widget; public class Consumer { Widget w; }",
        );
        let files = [widget, consumer];
        let src = consumer.1;
        let off = src.rfind("Widget w").unwrap() + 1; // caret on the type USE
        let d = decl(&files, "Consumer.java", off).expect("resolved");
        assert_eq!(d.file, "Widget.java");
        let wsrc = widget.1;
        let decl_off = wsrc.find("Widget {").unwrap();
        assert_eq!((d.start, d.end), (decl_off, decl_off + "Widget".len()));
        assert_eq!(d.label, "class com.acme.Widget");
    }

    #[test]
    fn declaration_jdk_symbol_is_none() {
        // `String` isn't a project type — no project source declares it → None (not an
        // error): the FE simply doesn't navigate into an unopenable JDK class.
        let src = "package p; public class C { String s; }";
        let files = [("C.java", src)];
        let off = src.find("String s").unwrap() + 1;
        assert!(decl(&files, "C.java", off).is_none());
    }

    #[test]
    fn line_col_1based_counts_lines_and_bytes() {
        let src = "line1\nline2\nabcXdef";
        let start = src.find('X').unwrap();
        assert_eq!(line_col_1based(src, start), (3, 4));
        assert_eq!(line_col_1based("first", 0), (1, 1));
        // Clamps an out-of-range offset to the source length rather than panicking.
        assert_eq!(line_col_1based("ab", 999), (1, 3));
    }

    #[test]
    fn local_rename_is_scope_exact() {
        let src = "package p; public class C { int f() { int x = 1; return x + x; } int g() { int x = 2; return x; } }";
        let files = [("C.java", src)];
        let off = src.find("int x = 1").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "y").expect("classified");
        // f()'s x: decl + two uses = 3, none in g().
        assert_eq!(p.total_edits(), 3);
        let g = src.find("int g()").unwrap();
        assert!(p.files[0].edits.iter().all(|e| e.start < g));
    }

    /// A record and a second file that reads it through the accessor the compiler generates.
    const RECORD_FILES: [(&str, &str); 2] = [
        (
            "Failure.java",
            "package p; public record Failure(String source_path, int code) {}",
        ),
        (
            "Report.java",
            "package p; public class Report { String show(Failure f) { return f.source_path(); } }",
        ),
    ];

    #[test]
    fn renaming_a_record_component_renames_its_accessor_uses() {
        // A record component is not a local: the JLS says it declares a private final field AND a
        // public accessor of the same name, and callers use the accessor (`f.source_path()`).
        // Classified as a local, the rename walked the record's own scope, renamed the component,
        // and left every caller reading a method that no longer exists.
        let src = RECORD_FILES[0].1;
        let off = src.find("String source_path").unwrap() + "String ".len();
        let p = plan(&RECORD_FILES, &[], "Failure.java", off, "sourcePath").expect("classified");

        let declarations: Vec<&Edit> = p
            .files
            .iter()
            .flat_map(|f| &f.edits)
            .filter(|e| e.reason == EditReason::Declaration)
            .collect();
        assert_eq!(declarations.len(), 1, "the component itself: {:?}", p.files);
        assert_eq!(
            &src[declarations[0].start..declarations[0].end],
            "source_path",
            "the declaration edit must be the component in the record header"
        );

        // …and the accessor call in the other file.
        let report = p.files.iter().find(|f| f.file == "Report.java");
        let report =
            report.unwrap_or_else(|| panic!("the accessor's caller must be edited: {:?}", p.files));
        assert_eq!(report.edits.len(), 1, "{:?}", report.edits);
        assert_eq!(
            &RECORD_FILES[1].1[report.edits[0].start..report.edits[0].end],
            "source_path",
        );
    }

    #[test]
    fn renaming_a_nested_record_component_renames_its_accessor_uses() {
        // The shape real code has: the record is declared INSIDE another class, which is where the
        // binary name stops being `p/Failure` and becomes `p/Outer/Failure`. Two bugs today came
        // from exactly that difference, so the top-level case passing proves less than it looks.
        let files = [(
            "Outer.java",
            "package p; public class Outer { \
             private record Failure(String source_path) {} \
             String show(Failure f) { return f.source_path(); } }",
        )];
        let src = files[0].1;
        let off = src.find("String source_path)").unwrap() + "String ".len();
        let p = plan(&files, &[], "Outer.java", off, "sourcePath").expect("classified");

        let edits = &p.files[0].edits;
        assert_eq!(
            edits.len(),
            2,
            "the component and the accessor call: {edits:?}"
        );
        assert!(
            edits.iter().any(|e| e.reason == EditReason::Declaration),
            "the component's own declaration: {edits:?}"
        );
        let accessor = src.find("f.source_path()").unwrap() + "f.".len();
        assert!(
            edits.iter().any(|e| e.start == accessor),
            "the accessor call must be renamed: {edits:?}"
        );
    }

    #[test]
    fn a_record_component_is_not_classified_as_a_local() {
        // The classification itself is the thing under test: a local rename is scope-exact and is
        // applied without a preview, so mis-classifying a component is what made an incomplete
        // rename look safe.
        let src = RECORD_FILES[0].1;
        let off = src.find("String source_path").unwrap() + "String ".len();
        let (resolver, project_types) = build_resolver(&RECORD_FILES);
        let files: Vec<SourceFile> = RECORD_FILES
            .iter()
            .map(|(p, s)| SourceFile {
                path: p.to_string(),
                source: s.to_string(),
            })
            .collect();
        let index = build_reference_index(&files, &resolver, &project_types);
        let target = crate::refs::classify_target(
            &index,
            "Failure.java",
            src,
            off,
            &resolver,
            &project_types,
            LangLevel(0),
        )
        .expect("classified");
        assert!(
            !matches!(target, crate::refs::RenameTarget::Local { .. }),
            "a record component must classify as a member, not a local: {target:?}"
        );
    }

    #[test]
    fn parameter_rename_includes_its_own_declaration() {
        // The regression this pins: a parameter is declared in the method HEADER, and the scope
        // walk used to start at the body — so every use was rewritten and `int source_count` was
        // left as it was. A rename that does that does not compile.
        let src = "package p; public class C { int f(int source_count) { return source_count + source_count; } }";
        let files = [("C.java", src)];
        let off = src.find("int source_count)").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "sourceCount").expect("classified");
        assert_eq!(p.total_edits(), 3, "declaration + two uses");
        let declarations: Vec<&Edit> = p.files[0]
            .edits
            .iter()
            .filter(|e| e.reason == EditReason::Declaration)
            .collect();
        assert_eq!(
            declarations.len(),
            1,
            "exactly one declaration edit: {:?}",
            p.files[0].edits
        );
        // …and it is the one in the header, before the body opens.
        let body = src.find('{').and_then(|_| src.find(") {")).expect("body");
        assert!(
            declarations[0].start < body,
            "the declaration edit must be the header's"
        );
    }

    #[test]
    fn renaming_a_parameter_leaves_a_method_of_the_same_name_alone() {
        // Java allows `void foo(int foo)`. Widening the scope to the whole declaration put the
        // method's own name in range, and renaming the parameter must not take it with it.
        let src = "package p; public class C { void foo(int foo) { System.out.println(foo); } }";
        let files = [("C.java", src)];
        let off = src.find("int foo)").unwrap() + "int ".len();
        let p = plan(&files, &[], "C.java", off, "count").expect("classified");
        assert_eq!(
            p.total_edits(),
            2,
            "the parameter and its one use: {:?}",
            p.files[0].edits
        );
        let method_name = src.find("void foo(").unwrap() + "void ".len();
        assert!(
            p.files[0].edits.iter().all(|e| e.start != method_name),
            "the method's own name must not be renamed"
        );
    }

    #[test]
    fn method_rename_hits_decl_and_calls() {
        let files = [
            (
                "A.java",
                "package p; public class A { int v() { return 1; } }",
            ),
            (
                "B.java",
                "package p; public class B { int u(A a) { return a.v() + a.v(); } }",
            ),
        ];
        let src = files[0].1;
        let off = src.find("int v()").unwrap() + "int ".len();
        let p = plan(&files, &[], "A.java", off, "value").expect("classified");
        assert_eq!(p.total_edits(), 3);
        let decls = p
            .files
            .iter()
            .flat_map(|f| &f.edits)
            .filter(|e| e.reason == EditReason::Declaration)
            .count();
        assert_eq!(decls, 1);
    }

    #[test]
    fn class_rename_edits_decl_import_ref_and_bean() {
        let widget = ("Widget.java", "package com.acme; public class Widget { }");
        let consumer = (
            "Consumer.java",
            "package com.app; import com.acme.Widget; public class Consumer { Widget w; Widget make() { return new Widget(); } }",
        );
        let beans = (
            "beans.xml",
            r#"<beans><bean id="w" class="com.acme.Widget"/><package><action name="s" class="w"/></package></beans>"#,
        );
        let files = [widget, consumer];
        let src = widget.1;
        let off = src.find("class Widget").unwrap() + "class ".len();
        let p = plan(&files, &[beans], "Widget.java", off, "Gadget").expect("classified");
        let all: Vec<_> = p.files.iter().flat_map(|f| &f.edits).collect();
        let cnt = |r: EditReason| all.iter().filter(|e| e.reason == r).count();
        assert_eq!(cnt(EditReason::Declaration), 1);
        assert_eq!(cnt(EditReason::Import), 1);
        assert_eq!(cnt(EditReason::SpringBean), 1);
        assert!(cnt(EditReason::Reference) >= 2);
        // Spring edit rewrites the FQCN; the <action class="w"> bean-id is untouched.
        let bean = all
            .iter()
            .find(|e| e.reason == EditReason::SpringBean)
            .unwrap();
        assert_eq!(bean.old, "com.acme.Widget");
        assert_eq!(bean.new_text, "com.acme.Gadget");
        // rename_apply flattens the same set.
        assert_eq!(rename_apply(&p).len(), p.total_edits());
    }

    #[test]
    fn leading_javadoc_extracts_type_doc() {
        let src = "package p;\n\n/**\n * Represents an order.\n * Second line.\n */\npublic class Order { }\n";
        let start = decl_site_for_key(
            src,
            &DeclKey::Type {
                binary: "p/Order".into(),
            },
        )
        .expect("type decl found");
        let doc = leading_javadoc(src, start).expect("javadoc found");
        assert_eq!(doc, "Represents an order.\nSecond line.");
    }

    #[test]
    fn leading_javadoc_extracts_method_doc() {
        let src = "package p;\npublic class C {\n  /** Does the thing. */\n  public int go() { return 1; }\n}\n";
        let start = decl_site_for_key(
            src,
            &DeclKey::Method {
                owner: "p/C".into(),
                name: "go".into(),
            },
        )
        .expect("method decl found");
        let doc = leading_javadoc(src, start).expect("javadoc found");
        assert_eq!(doc, "Does the thing.");
    }

    #[test]
    fn no_javadoc_yields_none() {
        let src = "package p;\n// just a line comment\npublic class C { }\n";
        let start = decl_site_for_key(
            src,
            &DeclKey::Type {
                binary: "p/C".into(),
            },
        )
        .unwrap();
        assert!(leading_javadoc(src, start).is_none());
    }
}

/// The subtype map, and its one hard requirement: that it keeps up with an `extends` clause.
#[cfg(test)]
mod subtype_map_tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import};
    use std::cell::RefCell;
    use std::sync::Arc;

    /// A resolver whose supertype links can be rewritten between calls — which is the whole point:
    /// an edit changes what `members_of` answers, and the map has to be asked again.
    #[derive(Default)]
    struct MutableResolver {
        types: RefCell<HashMap<String, (Option<String>, Vec<String>)>>,
    }

    impl MutableResolver {
        fn set(&self, binary: &str, superclass: Option<&str>, interfaces: &[&str]) {
            self.types.borrow_mut().insert(
                binary.to_string(),
                (
                    superclass.map(str::to_string),
                    interfaces.iter().map(|s| s.to_string()).collect(),
                ),
            );
        }
        fn remove(&self, binary: &str) {
            self.types.borrow_mut().remove(binary);
        }
    }

    impl TypeResolver for MutableResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            let (superclass, interfaces) = self.types.borrow().get(binary).cloned()?;
            Some(Arc::new(ClassMembers {
                superclass,
                interfaces,
                methods: Vec::new(),
                fields: Vec::new(),
                flags: Default::default(),
                type_params: Vec::new(),
            }))
        }

        /// Never consulted here — the map reads supertypes off resolved members, never off a name.
        fn resolve_simple_name(&self, _name: &str, _imports: &[Import]) -> Option<String> {
            None
        }
    }

    fn kids(map: &SubtypeMap, parent: &str) -> Vec<String> {
        map.children(parent).to_vec()
    }

    #[test]
    fn a_type_is_filed_under_each_of_its_supertypes() {
        let r = MutableResolver::default();
        r.set("p/Sub", Some("p/Base"), &["p/Marker"]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Sub", &r);
        assert_eq!(kids(&map, "p/Base"), vec!["p/Sub".to_string()]);
        assert_eq!(kids(&map, "p/Marker"), vec!["p/Sub".to_string()]);
    }

    /// The case the whole change exists for: changing `extends` MOVES the type, rather than filing
    /// it under both — a rename descending the old parent would edit a class that no longer
    /// overrides anything.
    #[test]
    fn changing_a_supertype_moves_the_type_rather_than_duplicating_it() {
        let r = MutableResolver::default();
        r.set("p/Sub", Some("p/Old"), &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Sub", &r);
        assert_eq!(kids(&map, "p/Old"), vec!["p/Sub".to_string()]);

        r.set("p/Sub", Some("p/New"), &[]);
        map.refresh_type("p/Sub", &r);
        assert!(kids(&map, "p/Old").is_empty(), "{:?}", kids(&map, "p/Old"));
        assert_eq!(kids(&map, "p/New"), vec!["p/Sub".to_string()]);
    }

    /// Adding an interface keeps the ones already there.
    #[test]
    fn adding_an_interface_keeps_the_existing_links() {
        let r = MutableResolver::default();
        r.set("p/Sub", Some("p/Base"), &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Sub", &r);

        r.set("p/Sub", Some("p/Base"), &["p/Marker"]);
        map.refresh_type("p/Sub", &r);
        assert_eq!(kids(&map, "p/Base"), vec!["p/Sub".to_string()]);
        assert_eq!(kids(&map, "p/Marker"), vec!["p/Sub".to_string()]);
    }

    /// Withdrawing takes the type out of every list it was in, and only those.
    #[test]
    fn withdrawing_a_type_leaves_its_siblings_alone() {
        let r = MutableResolver::default();
        r.set("p/A", Some("p/Base"), &[]);
        r.set("p/B", Some("p/Base"), &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/A", &r);
        map.refresh_type("p/B", &r);
        assert_eq!(kids(&map, "p/Base"), vec!["p/A".to_string(), "p/B".to_string()]);

        map.withdraw_type("p/A");
        assert_eq!(kids(&map, "p/Base"), vec!["p/B".to_string()]);
    }

    /// A type the resolver can no longer see is withdrawn rather than left filed — claiming it is
    /// a subtype of anything would be an invention.
    #[test]
    fn a_type_the_resolver_lost_is_withdrawn() {
        let r = MutableResolver::default();
        r.set("p/Sub", Some("p/Base"), &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Sub", &r);
        r.remove("p/Sub");
        map.refresh_type("p/Sub", &r);
        assert!(kids(&map, "p/Base").is_empty());
    }

    /// Refreshing the same unchanged type twice must not file it twice — absorbing is not additive.
    #[test]
    fn refreshing_an_unchanged_type_is_idempotent() {
        let r = MutableResolver::default();
        r.set("p/Sub", Some("p/Base"), &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Sub", &r);
        map.refresh_type("p/Sub", &r);
        assert_eq!(kids(&map, "p/Base"), vec!["p/Sub".to_string()]);
    }

    /// The children of a parent stay sorted whatever order they arrive in — an override family
    /// descends them, and a rename that visited them in a different order each run would produce a
    /// different edit list each run.
    #[test]
    fn children_stay_sorted_whatever_order_they_arrive_in() {
        let r = MutableResolver::default();
        for name in ["p/C", "p/A", "p/B"] {
            r.set(name, Some("p/Base"), &[]);
        }
        let mut map = SubtypeMap::default();
        for name in ["p/C", "p/A", "p/B"] {
            map.refresh_type(name, &r);
        }
        assert_eq!(
            kids(&map, "p/Base"),
            vec!["p/A".to_string(), "p/B".to_string(), "p/C".to_string()]
        );
    }

    /// A type with no supertype the resolver knows records nothing — and so has nothing to withdraw
    /// later, which is what keeps `parents` from growing an entry per type in the project.
    #[test]
    fn a_type_with_no_known_supertypes_records_nothing() {
        let r = MutableResolver::default();
        r.set("p/Root", None, &[]);
        let mut map = SubtypeMap::default();
        map.refresh_type("p/Root", &r);
        assert!(map.parents.is_empty());
    }
}
