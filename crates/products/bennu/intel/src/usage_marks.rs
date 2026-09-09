//! **How many places use this**, for every declaration in one file at once.
//!
//! The number IntelliJ draws above every member, and the greying-out of the ones nothing reaches.
//! Both come from here, and they are one answer rather than two: the count IS the fact, and
//! "unused" is what the count being zero means when it is safe to believe it.
//!
//! ## Why it is not find-usages in a loop
//!
//! [`crate::engine::SemanticEngine::find_usages`] answers for one caret, and getting there costs a
//! resolver-backed classification of what the caret is standing on. That is the right price for a
//! keystroke and the wrong one for thirty members: the file would be classified thirty times to
//! learn thirty things it already knows from its own text.
//!
//! So this walks the file once, builds each declaration's [`DeclKey`] from what the file itself
//! declares, and looks the key up. A lookup is a hash of a key the index is already stored by, so
//! the whole file costs one parse.
//!
//! ## Three answers, not two
//!
//! This is the part worth being careful about, because both ways of getting it wrong are bad in
//! their own way: a member greyed out wrongly is an invitation to delete working code, and a "no
//! usages" on every test method in a file is an editor that has learnt to say something true and
//! useless.
//!
//! So a declaration is one of three things, and only the first two are ever drawn:
//!
//! | | what is known | what is drawn |
//! |---|---|---|
//! | [`Reach::Code`] | nothing outside the code reaches it | the count; a zero greys the name |
//! | [`Reach::Maybe`] | something *might* — it carries an annotation this engine does not know | the count, and nothing is greyed |
//! | [`Reach::Entry`] | something *does*, by design — a framework, the runtime, an override | the count when there is one, and **nothing at all** when it is zero |
//!
//! [`Reach::Entry`] is the one that earns silence. A `@Test` is invoked by JUnit, a `@Bean` by the
//! container, a `@PrePersist` by the persistence provider, `main` by the JVM, an override through
//! its supertype: for all of these a count of zero is not a finding, it is the wrong question, and
//! answering it anyway puts a grey line above every method in a test file. The list of who calls
//! what is [`crate::framework_entry`], written out and attributed rather than guessed at.
//!
//! **A type whose members are entry points is one itself.** That is what makes a test class silent
//! without anybody guessing from its name: a class holding `@Test` methods is a class JUnit
//! instantiates, and it is true of a `@Bean`-bearing configuration and a `@GetMapping`-bearing
//! controller for exactly the same reason.
//!
//! [`Reach::Maybe`] is the residue, and it keeps the rule [`crate::safe_delete`] measured: **any**
//! annotation means a framework may reach a member by name, so nothing annotated is ever greyed.
//! The count is still shown, because a `@Deprecated` method nothing calls is precisely what
//! somebody wants to see.
//!
//! Two more that are neither: `main` and the serialization protocol, from
//! [`bennu_check::unused_member::RUNTIME_NAMES`] — borrowed rather than restated, since the
//! single-file unused-member check asks the same question about the same names — and a
//! **constructor**, which gets no mark at all because its callers are `new` expressions the index
//! keys separately.

use std::collections::HashMap;

use bennu_java::prelude::{parse_java, TypeResolver};
use tree_sitter::Node;

use crate::refs::{DeclKey, ReferenceIndex};
use crate::rename::SubtypeMap;

/// What is known about who reaches a declaration from outside the code — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Nothing outside the code can. The count is the whole story, and a zero means unused.
    Code,
    /// Something *might*: it carries an annotation this engine does not recognise, so no claim
    /// either way is safe. The count is shown; nothing is greyed.
    Maybe(String),
    /// Something *does*, by design — a framework, the runtime, or a supertype the call is keyed
    /// to. The reason names who. A count of zero here is drawn as nothing at all.
    Entry(&'static str),
}

/// One declaration in a file, and what is known about who uses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageMark {
    /// Byte offset of the start of the declaration, annotations included — where a row drawn
    /// *above* it belongs.
    pub decl: usize,
    /// Byte span of the declaration's NAME token. What a "nothing uses this" tint colours: the
    /// name is the part you read, and tinting a whole method body would make the file unreadable.
    pub start: usize,
    pub end: usize,
    /// `"type"` | `"method"` | `"field"`.
    pub kind: &'static str,
    /// The declared name, for a label that says what it is about.
    pub name: String,
    /// How many use sites the index holds. Declarations are not use sites, so a member used only
    /// where it is written counts zero.
    pub count: usize,
    /// Who reaches it from outside the code, if anything is known to.
    pub reach: Reach,
}

impl UsageMark {
    /// Whether nothing is known to reach this. The question the caller asks to decide whether to
    /// grey the name.
    pub fn is_unused(&self) -> bool {
        self.count == 0 && matches!(self.reach, Reach::Code)
    }

    /// Whether there is nothing worth saying about it at all — an entry point nothing *also*
    /// calls. The caller draws no row: "no usages · the test runner runs it" above every method
    /// of a test file is noise wearing an explanation.
    pub fn is_silent(&self) -> bool {
        self.count == 0 && matches!(self.reach, Reach::Entry(_))
    }
}

/// Every declaration in `source` with its use count, for the file `file` of a project whose
/// `index` is built.
///
/// `policy` must be the **full-classpath** resolver, not the walk one. Without the JDK in it,
/// `equals`, `toString`, `run` and every other override of a library method resolves to nothing
/// and is reported as unused — which is the same trap rename and safe delete document, arriving
/// here as a file full of greyed-out overrides.
pub fn usage_marks(
    index: &ReferenceIndex,
    source: &str,
    policy: &dyn TypeResolver,
    subtypes: &SubtypeMap,
) -> Vec<UsageMark> {
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let symbols = bennu_java::prelude::extract_symbols_from_root(&tree.root_node(), source);

    // The binary name of every type this file declares, by the name the file writes — so a member
    // walk can name its owner without re-deriving the nesting.
    let owners: HashMap<String, String> = symbols
        .types
        .iter()
        .filter(|t| !t.is_anonymous)
        .map(|t| (t.fqn.clone(), t.fqn.replace('.', "/")))
        .collect();

    let mut out = Vec::new();
    let mut walker = Walker { source, index, policy, subtypes, owners: &owners, out: &mut out };
    walker.file(&tree.root_node(), symbols.package.as_deref(), None);
    out.sort_by_key(|m| m.start);
    out
}

struct Walker<'a> {
    source: &'a str,
    index: &'a ReferenceIndex,
    policy: &'a dyn TypeResolver,
    subtypes: &'a SubtypeMap,
    owners: &'a HashMap<String, String>,
    out: &'a mut Vec<UsageMark>,
}

/// The declarations a type body can hold that this reports on.
const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

impl Walker<'_> {
    /// Walk every type declared at this level, and recurse into their bodies.
    fn file(&mut self, node: &Node<'_>, package: Option<&str>, outer: Option<&str>) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if !TYPE_DECLS.contains(&child.kind()) {
                continue;
            }
            let Some(name_node) = child.child_by_field_name("name") else { continue };
            let Ok(name) = name_node.utf8_text(self.source.as_bytes()) else { continue };
            let fqn = match (outer, package) {
                (Some(o), _) => format!("{o}.{name}"),
                (None, Some(p)) => format!("{p}.{name}"),
                (None, None) => name.to_string(),
            };
            let Some(binary) = self.owners.get(&fqn).cloned() else { continue };

            // The members first, because they decide the type: a class holding `@Test` methods is
            // a class JUnit instantiates, and a class of `@Bean` factories is one Spring builds.
            // That is what makes a test class silent without anybody guessing from its name.
            let members = match child.child_by_field_name("body") {
                Some(body) => self.members(&body, &binary),
                None => Vec::new(),
            };
            let annotations = annotations_on(&child, self.source);
            let reach = annotations
                .iter()
                .find_map(|a| crate::framework_entry::entry_for_type(a).map(Reach::Entry))
                .or_else(|| {
                    members.iter().find_map(|m| match m.reach {
                        Reach::Entry(why) => Some(Reach::Entry(why)),
                        _ => None,
                    })
                })
                .or_else(|| annotations.first().cloned().map(Reach::Maybe))
                .unwrap_or(Reach::Code);

            self.push(
                &child,
                &name_node,
                "type",
                name,
                self.index.usages_of(&DeclKey::Type { binary: binary.clone() }).len(),
                reach,
            );
            self.out.extend(members);

            if let Some(body) = child.child_by_field_name("body") {
                // A nested type is declared in that body, and its own members are declared in its.
                self.file(&body, package, Some(&fqn));
            }
        }
    }

    /// The methods and fields declared directly in one type body.
    /// The methods and fields declared directly in one type body.
    ///
    /// Returned rather than pushed, because the type above needs to read them before it can say
    /// what it is itself.
    fn members(&mut self, body: &Node<'_>, owner: &str) -> Vec<UsageMark> {
        let bytes = self.source.as_bytes();
        let mut out = Vec::new();
        let mut cursor = body.walk();
        for member in body.named_children(&mut cursor) {
            match member.kind() {
                "method_declaration" => {
                    let Some(name_node) = member.child_by_field_name("name") else { continue };
                    let Ok(name) = name_node.utf8_text(bytes) else { continue };
                    let key = DeclKey::Method { owner: owner.to_string(), name: name.to_string() };
                    let annotations = annotations_on(&member, self.source);
                    // Every annotation is examined, not just the first: `@Override @Bean` is
                    // ordinary, and stopping at `@Override` would miss what Spring does with it.
                    let reach = annotations
                        .iter()
                        .find_map(|a| crate::framework_entry::entry_for_method(a).map(Reach::Entry))
                        .or_else(|| reached_by_the_runtime(name))
                        .or_else(|| self.inherited(owner, name))
                        .or_else(|| annotations.first().cloned().map(Reach::Maybe))
                        .unwrap_or(Reach::Code);
                    let count = self.index.usages_of(&key).len();
                    out.push(mark(&member, &name_node, "method", name, count, reach));
                }
                // A constructor is deliberately absent, and not by omission: its callers are `new`
                // expressions, which the index keys by the TYPE. Counting them here would report
                // every constructor in the project as used zero times.
                "field_declaration" => {
                    // A field is not *called*, so no framework-entry list applies to one. What
                    // reaches it is injection or serialization, and both are `Maybe`: an
                    // `@Autowired` field nothing reads is a real finding, and silencing it would
                    // hide exactly the thing worth seeing.
                    let annotation = annotations_on(&member, self.source).into_iter().next();
                    for (name, name_node) in field_names(&member, bytes) {
                        let key =
                            DeclKey::Field { owner: owner.to_string(), name: name.to_string() };
                        let count = self.index.usages_of(&key).len();
                        let reach = reached_by_the_runtime(&name)
                            .or_else(|| annotation.clone().map(Reach::Maybe))
                            .unwrap_or(Reach::Code);
                        out.push(mark(&member, &name_node, "field", &name, count, reach));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Whether this method is one rung of an override family — so a call keyed to another rung is
    /// still a call to this one, and a zero here is structurally meaningless rather than merely
    /// uncertain.
    fn inherited(&self, owner: &str, name: &str) -> Option<Reach> {
        if crate::rename::library_override(self.policy, owner, name).is_some() {
            return Some(Reach::Entry("it overrides a library method"));
        }
        let start = bennu_java::prelude::TypeRef::simple(owner);
        let above = bennu_java::prelude::walk_up(self.policy, &start, |a| {
            (a.depth > 0 && a.members.methods.iter().any(|m| m.name == name))
                .then(|| a.ty.binary_name.clone())
        });
        if above.is_some() {
            return Some(Reach::Entry("it overrides a supertype's method"));
        }
        // The call is keyed to whichever rung was written, so a zero on this one says nothing
        // about the program — which is why it is an entry point and not a `Maybe`.
        self.implementor(owner, name).map(|_| Reach::Entry("something in the project implements it"))
    }

    /// A project type BELOW `owner` that declares the same method — the half that is easy to
    /// forget. An interface's method is called through the interface, so the *interface's* count is
    /// the real one and the implementation's is zero; looking only upward would grey every
    /// implementation in the project.
    fn implementor(&self, owner: &str, name: &str) -> Option<String> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut queue: std::collections::VecDeque<String> =
            self.subtypes.children(owner).iter().cloned().collect();
        while let Some(child) = queue.pop_front() {
            if seen.len() > 256 || !seen.insert(child.clone()) {
                continue;
            }
            if self
                .policy
                .members_of(&child)
                .is_some_and(|m| m.methods.iter().any(|x| x.name == name))
            {
                return Some(child);
            }
            queue.extend(self.subtypes.children(&child).iter().cloned());
        }
        None
    }

    fn push(
        &mut self,
        decl: &Node<'_>,
        name_node: &Node<'_>,
        kind: &'static str,
        name: &str,
        count: usize,
        reach: Reach,
    ) {
        let m = mark(decl, name_node, kind, name, count, reach);
        self.out.push(m);
    }
}

/// One mark, from the nodes it was read off.
fn mark(
    decl: &Node<'_>,
    name_node: &Node<'_>,
    kind: &'static str,
    name: &str,
    count: usize,
    reach: Reach,
) -> UsageMark {
    UsageMark {
        decl: decl.start_byte(),
        start: name_node.start_byte(),
        end: name_node.end_byte(),
        kind,
        name: name.to_string(),
        count,
        reach,
    }
}

/// Every annotation written on a declaration, by simple name and in source order.
///
/// All of them, not the first: `@Override @Bean` is an ordinary pair, and stopping at `@Override`
/// would miss the one that says Spring calls the method.
///
/// Read off the `modifiers` child rather than by scanning the text, so an `@Deprecated` inside the
/// method's body or in its javadoc is not mistaken for one on the declaration.
fn annotations_on(decl: &Node<'_>, source: &str) -> Vec<String> {
    let modifiers = match decl.child_by_field_name("modifiers") {
        Some(node) => Some(node),
        None => {
            let mut c = decl.walk();
            let found = decl.children(&mut c).find(|n| n.kind() == "modifiers");
            found
        }
    };
    let Some(modifiers) = modifiers else { return Vec::new() };
    let mut c = modifiers.walk();
    modifiers
        .named_children(&mut c)
        .filter(|n| n.kind() == "marker_annotation" || n.kind() == "annotation")
        .filter_map(|n| n.child_by_field_name("name"))
        // The name as written: `@org.junit.Test` qualifies, and its last segment is the name every
        // table here is keyed by.
        .filter_map(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|name| name.rsplit('.').next().unwrap_or(name).to_string())
        .collect()
}

/// Whether the runtime reaches this name without any source file naming it — `main`,
/// `serialVersionUID`, `readObject` and the rest of the serialization protocol.
///
/// The list belongs to `bennu-check`'s single-file unused-member check, which asks the same
/// question about the same names. Borrowed rather than restated: two lists drift, and the drift
/// shows up as a member one of them greys out and the other does not.
fn reached_by_the_runtime(name: &str) -> Option<Reach> {
    bennu_check::prelude::RUNTIME_NAMES
        .contains(&name)
        .then_some(Reach::Entry("the runtime reaches it by name"))
}

/// Every name a `field_declaration` declares — `int a, b;` is two — with the token for each.
fn field_names<'t>(decl: &Node<'t>, bytes: &[u8]) -> Vec<(String, Node<'t>)> {
    let mut out = Vec::new();
    let mut c = decl.walk();
    for child in decl.named_children(&mut c) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else { continue };
        if let Ok(name) = name_node.utf8_text(bytes) {
            out.push((name.to_string(), name_node));
        }
    }
    out
}
