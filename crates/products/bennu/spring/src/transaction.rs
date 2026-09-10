//! A transaction held open across a network call.
//!
//! ## The defect, and why the symptom points at the wrong thing
//!
//! A `@Transactional` method borrows a connection from the pool for its whole duration. That is the
//! point of it. What it is not for is waiting on somebody else's server:
//!
//! ```java
//! @Transactional
//! public void conferma(Ordine o) {
//!     repository.save(o);
//!     corriere.postForObject(url, o, Esito.class);   // ← the connection is still held
//!     o.setTracking(…);
//! }
//! ```
//!
//! On a quiet afternoon this is fine. Under load the pool has ten connections and the courier's
//! endpoint answers in four seconds, so ten concurrent confirmations hold every connection in the
//! application while doing no database work at all — and the eleventh request, which only wanted to
//! read a customer, waits for a connection.
//!
//! **What you are shown is a slow database.** Every dashboard says so: connection wait time up,
//! queries queueing, pool saturated. The database is idle. The cause is one line, in one method,
//! that looks like an ordinary call to a collaborator — and nothing about it says "this is remote".
//!
//! ## How the remote call is recognised
//!
//! By the **declared type of the receiver**, never by the method name. `execute`, `send` and `get`
//! are on everything; `RestTemplate`, `WebClient` and `CloseableHttpClient` are on exactly one kind
//! of thing. So a field, parameter or local whose written type is one of the known clients makes
//! every call on it a network call, and everything else is silence.
//!
//! That rules out the false positive that would matter most: a repository, a mapper or a service of
//! the project's own is never mistaken for a remote call, however its methods are spelled.
//!
//! ## What it does not claim
//!
//! A Feign client, a generated SOAP stub or a hand-written gateway interface is a remote call this
//! cannot see — recognising them needs the project's type index, not one file. Under-report rather
//! than risk a false positive (docs §7): everything reported here is certainly remote, and there
//! are remote calls it stays quiet about.

use bennu_java::prelude::{
    annotation_named, annotation_value_text, has_modifier, node_text, parse_java, simple_name,
    type_declarations,
};
use tree_sitter::Node;

pub const CODE_REMOTE_IN_TRANSACTION: &str = "spring.transaction.remote-call";
pub const CODE_SLEEP_IN_TRANSACTION: &str = "spring.transaction.sleep";

/// The types whose every method is a call to somebody else's server.
///
/// Simple names, because that is how a field is declared. Deliberately specific: a bare `Client` or
/// anything ending in `Client` would sweep in the project's own gateway classes, which is a
/// different claim from this one and a much weaker one.
const REMOTE_TYPES: &[&str] = &[
    "RestTemplate",
    "TestRestTemplate",
    "RestClient",
    "WebClient",
    "OkHttpClient",
    "CloseableHttpClient",
    "HttpClient",
    "HttpURLConnection",
    "URLConnection",
    "WebTarget",
    "Jedis",
    "MongoClient",
];

/// The propagations that **suspend** the transaction, so nothing is being held while the call runs.
const SUSPENDING: &[&str] = &["NOT_SUPPORTED", "NEVER"];

/// One thing a transaction is waiting for that is not the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldTransaction {
    pub code: &'static str,
    pub message: String,
    pub start: usize,
    pub end: usize,
}

/// Read a source for calls that hold a connection while waiting on something else.
pub fn issues_in(source: &str) -> Vec<HeldTransaction> {
    // The cheap reject: no transaction, nothing to hold.
    if !source.contains("Transactional") {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    for type_decl in type_declarations(tree.root_node()) {
        check_type(type_decl, source, &mut out);
    }
    out.sort_by_key(|i| i.start);
    out
}

fn check_type(type_decl: Node<'_>, source: &str, out: &mut Vec<HeldTransaction>) {
    let Some(body) = type_decl.child_by_field_name("body") else { return };
    let class_transactional = annotation_named(type_decl, source, "Transactional").is_some();

    // The remote-typed fields of this class, by name. Collected before the methods so a method can
    // ask about a field declared after it, which is where half of them are.
    let mut remote: Vec<String> = Vec::new();
    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "field_declaration" {
            continue;
        }
        if let Some(name) = remote_declaration(member, source) {
            remote.push(name);
        }
    }

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        if member.kind() != "method_declaration" {
            continue;
        }
        let own = annotation_named(member, source, "Transactional");
        // A class-level `@Transactional` covers the public methods; a private helper is reached
        // through one of them and is already inside whatever transaction they opened, so it is not
        // reported on its own — the call there belongs to the caller's transaction, and reporting
        // both would be reporting one defect twice.
        let transactional = match own {
            Some(_) => true,
            None => class_transactional && has_modifier(member, source, "public"),
        };
        if !transactional {
            continue;
        }
        if let Some(annotation) = own {
            if let Some(propagation) = annotation_value_text(annotation, source, "propagation") {
                if SUSPENDING.iter().any(|p| propagation.ends_with(p)) {
                    continue;
                }
            }
        }
        let Some(method_body) = member.child_by_field_name("body") else { continue };

        // Parameters and locals of remote type join the field list, for this method only.
        let mut in_scope = remote.clone();
        if let Some(params) = member.child_by_field_name("parameters") {
            let mut c = params.walk();
            for param in params.named_children(&mut c) {
                if let Some(name) = remote_parameter(param, source) {
                    in_scope.push(name);
                }
            }
        }
        collect_locals(method_body, source, &mut in_scope);

        walk_body(method_body, source, &in_scope, out);
    }
}

/// The name of a field declared with a remote type.
fn remote_declaration(member: Node<'_>, source: &str) -> Option<String> {
    let type_node = member.child_by_field_name("type")?;
    if !is_remote(node_text(&type_node, source)) {
        return None;
    }
    let declarator = member.child_by_field_name("declarator")?;
    let name = declarator.child_by_field_name("name")?;
    Some(node_text(&name, source).to_string())
}

fn remote_parameter(param: Node<'_>, source: &str) -> Option<String> {
    let type_node = param.child_by_field_name("type")?;
    if !is_remote(node_text(&type_node, source)) {
        return None;
    }
    let name = param.child_by_field_name("name")?;
    Some(node_text(&name, source).to_string())
}

fn collect_locals(node: Node<'_>, source: &str, out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "local_variable_declaration" {
            if let Some(type_node) = child.child_by_field_name("type") {
                if is_remote(node_text(&type_node, source)) {
                    let mut c = child.walk();
                    for declarator in child.named_children(&mut c) {
                        if declarator.kind() != "variable_declarator" {
                            continue;
                        }
                        if let Some(name) = declarator.child_by_field_name("name") {
                            out.push(node_text(&name, source).to_string());
                        }
                    }
                }
            }
        }
        collect_locals(child, source, out);
    }
}

/// A written type naming one of the known clients. Generics are stripped — `WebClient` and a
/// `HttpClient<T>` are the same thing to this question.
fn is_remote(written: &str) -> bool {
    let raw = written.split_once('<').map(|(head, _)| head).unwrap_or(written).trim();
    REMOTE_TYPES.contains(&simple_name(raw))
}

fn walk_body(node: Node<'_>, source: &str, remote: &[String], out: &mut Vec<HeldTransaction>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // A lambda body regularly runs somewhere else entirely — on a scheduler, in a retry
        // template, inside a `CompletableFuture`. Whether the transaction is still open there is
        // not a question this can answer, so it is not asked.
        if matches!(child.kind(), "lambda_expression" | "class_declaration") {
            continue;
        }
        if child.kind() == "method_invocation" {
            if let Some(issue) = remote_call(child, source, remote) {
                out.push(issue);
            }
        }
        walk_body(child, source, remote, out);
    }
}

fn remote_call(call: Node<'_>, source: &str, remote: &[String]) -> Option<HeldTransaction> {
    let name_node = call.child_by_field_name("name")?;
    let called = node_text(&name_node, source);

    // `Thread.sleep(…)` inside a transaction: the same defect without a server at the other end —
    // a pooled connection held for a fixed number of seconds of doing nothing.
    if called == "sleep" {
        if let Some(object) = call.child_by_field_name("object") {
            if node_text(&object, source) == "Thread" {
                return Some(HeldTransaction {
                    code: CODE_SLEEP_IN_TRANSACTION,
                    message: "this sleeps while holding a database connection — the transaction \
                              keeps its connection for the whole wait, and under load the pool \
                              empties"
                        .to_string(),
                    start: call.start_byte(),
                    end: call.end_byte(),
                });
            }
        }
        return None;
    }

    let object = call.child_by_field_name("object")?;
    if object.kind() != "identifier" {
        return None;
    }
    let receiver = node_text(&object, source);
    if !remote.iter().any(|r| r == receiver) {
        return None;
    }
    Some(HeldTransaction {
        code: CODE_REMOTE_IN_TRANSACTION,
        message: format!(
            "`{receiver}.{called}(…)` is a network call inside a transaction — the database \
             connection is held for as long as the other server takes to answer. Under load the \
             pool empties and the database is what looks slow. Move the call outside the \
             transactional method"
        ),
        start: call.start_byte(),
        end: name_node.end_byte(),
    })
}

/// Whether this file declares a `@Transactional` anywhere — the host's cheap gate.
pub fn mentions_transaction(source: &str) -> bool {
    source.contains("Transactional")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"@Service
public class OrdiniService {

    private final RestTemplate corriere;
    private final OrdineRepository repository;

    @Transactional
    public void conferma(Ordine o) {
        repository.save(o);
        corriere.postForObject(url, o, Esito.class);
        repository.flush();
    }

    public void fuoriTransazione(Ordine o) {
        corriere.postForObject(url, o, Esito.class);
    }

    @Transactional(propagation = Propagation.NOT_SUPPORTED)
    public void sospesa(Ordine o) {
        corriere.getForObject(url, Esito.class);
    }

    @Transactional
    public void aspetta() throws Exception {
        Thread.sleep(5000);
    }
}
"#;

    #[test]
    fn a_network_call_inside_a_transaction_is_reported_on_the_call() {
        let found = issues_in(SRC);
        let issue = found
            .iter()
            .find(|i| i.code == CODE_REMOTE_IN_TRANSACTION)
            .expect("the remote call");
        assert_eq!(&SRC[issue.start..issue.end], "corriere.postForObject");
        assert!(issue.message.contains("the database is what looks slow"), "{}", issue.message);
    }

    /// The same call outside a transaction is an ordinary call, and saying anything about it would
    /// make the check noise on every client in the project.
    #[test]
    fn the_same_call_outside_a_transaction_is_nothing()

    {
        let found = issues_in(SRC);
        assert_eq!(
            found.iter().filter(|i| i.code == CODE_REMOTE_IN_TRANSACTION).count(),
            1,
            "only the transactional one: {found:?}"
        );
    }

    /// A propagation that suspends the transaction holds no connection while the call runs — which
    /// is exactly the fix, and reporting it would be reporting the fix as the defect.
    #[test]
    fn a_suspended_transaction_holds_nothing() {
        let found = issues_in(SRC);
        assert!(
            found.iter().all(|i| !SRC[i.start..i.end].contains("getForObject")),
            "{found:?}"
        );
    }

    #[test]
    fn sleeping_inside_a_transaction_is_the_same_defect_without_a_server() {
        let found = issues_in(SRC);
        let issue = found.iter().find(|i| i.code == CODE_SLEEP_IN_TRANSACTION).expect("the sleep");
        assert_eq!(&SRC[issue.start..issue.end], "Thread.sleep(5000)");
    }

    /// The false positive that would matter most: the project's own repository, whose methods are
    /// spelled exactly like a client's.
    #[test]
    fn the_projects_own_collaborators_are_never_mistaken_for_remote_calls() {
        let src = r#"class A {
            private final OrdineRepository repository;
            private final PagamentoGateway gateway;
            @Transactional public void go() {
                repository.execute();
                gateway.send(null);
                repository.get(1L);
            }
        }"#;
        assert!(issues_in(src).is_empty(), "{:?}", issues_in(src));
    }

    /// A class-level `@Transactional` covers the public methods.
    #[test]
    fn a_class_level_transaction_covers_its_public_methods() {
        let src = r#"@Transactional
        class A {
            private final WebClient web;
            public void go() { web.get(); }
            private void helper() { web.get(); }
        }"#;
        let found = issues_in(src);
        assert_eq!(found.len(), 1, "the public one only: {found:?}");
    }

    #[test]
    fn a_local_client_counts_as_much_as_a_field() {
        let src = r#"class A {
            @Transactional public void go() {
                CloseableHttpClient http = HttpClients.createDefault();
                http.execute(request);
            }
        }"#;
        let found = issues_in(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].code, CODE_REMOTE_IN_TRANSACTION);
    }

    /// A lambda body regularly runs somewhere else entirely, and whether the transaction is still
    /// open there is not a question one file can answer.
    #[test]
    fn a_call_inside_a_lambda_is_not_claimed() {
        let src = r#"class A {
            private final RestTemplate rest;
            @Transactional public void go() {
                retry.execute(ctx -> rest.getForObject(url, String.class));
            }
        }"#;
        assert!(issues_in(src).is_empty());
    }

    #[test]
    fn a_file_with_no_transaction_is_not_even_parsed() {
        assert!(issues_in("class A { RestTemplate r; void go() { r.getForObject(); } }").is_empty());
    }
}
