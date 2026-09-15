//! Completion and hover along an `Optional` returned by a project INTERFACE — the reported
//! Spring/Lombok class, reproduced as written.
//!
//! ```java
//! @RequiredArgsConstructor @Component
//! public class CheckAssignedUser implements AttributeValidator {
//!     private final IdentityResolver identity_resolver;
//!     private void check_delegate(final String username) {
//!         identity_resolver.resolve_identity().ma|
//!     }
//! }
//! ```
//!
//! Four things went wrong on it, and each has its section below:
//!
//! 1. after `.ma` the list was not `Optional`'s members;
//! 2. inside `.map(Re|)` the type the function receives was not offered first — and with nothing
//!    typed, not at all;
//! 3. after `.map(ResolvedIdentity::|)` nothing of `ResolvedIdentity` was offered;
//! 4. `val delegate = ….map(ResolvedIdentity::identifier)` hovered with no type.
//!
//! One trigger per file: a half-written member access is a syntax error, and several in one class
//! body let tree-sitter's recovery swallow the declarations between them.

mod common;
use common::*;

const IDENTITY_RESOLVER: &str = "package com.acme;\n\
     import java.util.Optional;\n\
     public interface IdentityResolver {\n\
     \x20   Optional<ResolvedIdentity> resolve_identity();\n\
     \x20   Optional<LombokIdentity> lombok_identity();\n\
     }\n";

/// A record: `identifier()` and `roles()` are synthesized accessors, nobody wrote them.
const RESOLVED_IDENTITY: &str = "package com.acme;\n\
     public record ResolvedIdentity(String identifier, int roles) {\n\
     \x20   public static ResolvedIdentity anonymous() { return null; }\n\
     \x20   public static String describe(ResolvedIdentity identity) { return \"\"; }\n\
     \x20   public boolean has_role(String role) { return false; }\n\
     }\n";

/// The same data behind Lombok: `getIdentifier()` is generated.
const LOMBOK_IDENTITY: &str = "package com.acme;\n\
     import lombok.Getter;\n\
     @Getter\n\
     public class LombokIdentity {\n\
     \x20   private String identifier;\n\
     }\n";

const UTIL: &str = "package com.acme;\n\
     public class Util {\n\
     \x20   public static Token convert(ResolvedIdentity identity) { return null; }\n\
     }\n";

const TOKEN: &str = "package com.acme;\npublic class Token { }\n";

const USER: &str = "com/acme/CheckAssignedUser.java";

/// The reported project, with `body` as the single statement of `check_delegate`.
fn project(body: &str) -> Project {
    let user = format!(
        "package com.acme;\n\
         \n\
         import lombok.RequiredArgsConstructor;\n\
         import lombok.val;\n\
         import org.springframework.stereotype.Component;\n\
         \n\
         @RequiredArgsConstructor @Component\n\
         public class CheckAssignedUser implements AttributeValidator {{\n\
         \x20   private final IdentityResolver identity_resolver;\n\
         \n\
         \x20   private void check_delegate(final String username) {{\n\
         \x20       {body}\n\
         \x20   }}\n\
         }}\n"
    );
    Project::new(&[
        ("com/acme/IdentityResolver.java", IDENTITY_RESOLVER),
        ("com/acme/ResolvedIdentity.java", RESOLVED_IDENTITY),
        ("com/acme/LombokIdentity.java", LOMBOK_IDENTITY),
        ("com/acme/Util.java", UTIL),
        ("com/acme/Token.java", TOKEN),
        (USER, &user),
    ])
}

/// The offset just after the LAST `needle` in the user file.
fn after(p: &Project, needle: &str) -> usize {
    at_last(p.source(USER), needle) + needle.len()
}

fn rank_of(labels: &[String], name: &str) -> usize {
    labels.iter().position(|l| l == name).unwrap_or(usize::MAX)
}

/// The members a completion on an `Optional` may offer — `Optional`'s own, and `Object`'s.
const OPTIONAL_MEMBERS: &[&str] = &["map", "orElse", "isPresent", "toString"];

// ── 1. after the dot: Optional's members, and only those ─────────────────────────────────────

#[test]
fn after_a_prefix_on_the_optional_only_its_members_are_offered() {
    let p = project("identity_resolver.resolve_identity().ma");
    let labels = p.complete_labels(USER, after(&p, ".ma"));
    assert!(labels.contains(&"map".to_string()), "{labels:?}");
    for label in &labels {
        assert!(OPTIONAL_MEMBERS.contains(&label.as_str()), "{label} is not a member of Optional: {labels:?}");
    }
}

#[test]
fn right_after_the_dot_every_member_of_the_optional_is_offered() {
    let p = project("identity_resolver.resolve_identity().");
    let labels = p.complete_labels(USER, after(&p, "resolve_identity()."));
    for want in ["map", "orElse", "isPresent"] {
        assert!(labels.contains(&want.to_string()), "{want}: {labels:?}");
    }
}

/// The same receiver held in a Lombok `val` first.
#[test]
fn a_lombok_val_holding_the_optional_completes_the_same() {
    let p = project("val identity = identity_resolver.resolve_identity(); identity.");
    let labels = p.complete_labels(USER, after(&p, "identity."));
    assert!(labels.contains(&"map".to_string()), "{labels:?}");
}

// ── 2. inside map(…): the type the function receives ─────────────────────────────────────────

#[test]
fn a_capitalised_word_in_map_is_offered_the_optionals_element() {
    let p = project("identity_resolver.resolve_identity().map(Re)");
    assert_eq!(p.functional_types(USER, after(&p, "map(Re")), ["ResolvedIdentity"]);
}

/// Ctrl+Space with nothing typed.
#[test]
fn an_empty_argument_in_map_is_offered_the_optionals_element() {
    let p = project("identity_resolver.resolve_identity().map()");
    assert_eq!(p.functional_types(USER, after(&p, "map(")), ["ResolvedIdentity"]);
}

/// A lower-case word is a lambda parameter or a local being written, not a type.
#[test]
fn a_lower_case_word_in_map_is_left_to_the_scope() {
    let p = project("identity_resolver.resolve_identity().map(re)");
    assert!(p.functional_types(USER, after(&p, "map(re")).is_empty());
}

/// `orElse(T)` takes a value: no function, no descriptor, nothing offered.
#[test]
fn a_value_argument_offers_no_function_types() {
    let p = project("identity_resolver.resolve_identity().orElse(Re)");
    assert!(p.functional_types(USER, after(&p, "orElse(Re")).is_empty());
}

// ── 3. after `::`: the methods a reference can name ─────────────────────────────────────────

#[test]
fn after_type_colons_the_types_methods_are_offered() {
    let p = project("identity_resolver.resolve_identity().map(ResolvedIdentity::)");
    let labels = p.complete_labels(USER, after(&p, "ResolvedIdentity::"));
    for want in ["identifier", "roles", "has_role", "anonymous", "describe", "new"] {
        assert!(labels.contains(&want.to_string()), "{want}: {labels:?}");
    }
    // The popup that used to open here offered locals and classes.
    for not_a_member in ["identity_resolver", "username", "check_delegate", "Token"] {
        assert!(!labels.contains(&not_a_member.to_string()), "{not_a_member}: {labels:?}");
    }
}

/// `Function<ResolvedIdentity, U>`: a no-argument instance method and a static taking one
/// `ResolvedIdentity` compile; `has_role(String)` and `anonymous()` do not.
#[test]
fn the_methods_that_fit_the_function_come_first() {
    let p = project("identity_resolver.resolve_identity().map(ResolvedIdentity::)");
    let labels = p.complete_labels(USER, after(&p, "ResolvedIdentity::"));
    assert!(rank_of(&labels, "identifier") < rank_of(&labels, "has_role"), "{labels:?}");
    assert!(rank_of(&labels, "describe") < rank_of(&labels, "anonymous"), "{labels:?}");
}

#[test]
fn a_typed_member_prefix_narrows_the_reference() {
    let p = project("identity_resolver.resolve_identity().map(ResolvedIdentity::ide)");
    let labels = p.complete_labels(USER, after(&p, "ResolvedIdentity::ide"));
    assert!(labels.contains(&"identifier".to_string()), "{labels:?}");
    assert!(!labels.contains(&"has_role".to_string()), "{labels:?}");
}

/// A reference names the method: `map(ResolvedIdentity::identifier())` does not compile.
#[test]
fn a_reference_inserts_the_name_without_parentheses() {
    let p = project("identity_resolver.resolve_identity().map(ResolvedIdentity::)");
    let items = p.complete(USER, after(&p, "ResolvedIdentity::"));
    assert!(!items.is_empty());
    for item in &items {
        assert!(item.insert_text.is_none(), "{} inserts {:?}", item.label, item.insert_text);
        assert!(item.kind == "method" || item.kind == "constructor", "{}: {}", item.label, item.kind);
    }
}

// ── 4. hovering the val: Optional of what the reference returns ──────────────────────────────

/// The type a `val` named `name` is declared with, read at its name — what hover shows.
fn declared_type(p: &Project, name: &str) -> Option<(String, Vec<String>)> {
    let start = at(p.source(USER), &format!(" {name} =")) + 1;
    let ty = p.type_of(USER, start, start + name.len())?;
    Some((ty.binary_name, ty.type_args.into_iter().map(|a| a.binary_name).collect()))
}

#[track_caller]
fn assert_optional_of(p: &Project, name: &str, element: &str) {
    assert_eq!(
        declared_type(p, name),
        Some(("java/util/Optional".to_string(), vec![element.to_string()])),
    );
}

#[test]
fn a_val_mapped_through_a_record_accessor_reference_is_an_optional_of_its_type() {
    let p = project("val delegate = identity_resolver.resolve_identity().map(ResolvedIdentity::identifier);");
    assert_optional_of(&p, "delegate", "java/lang/String");
}

#[test]
fn a_val_mapped_through_a_lombok_getter_reference_is_an_optional_of_its_type() {
    let p = project("val delegate = identity_resolver.lombok_identity().map(LombokIdentity::getIdentifier);");
    assert_optional_of(&p, "delegate", "java/lang/String");
}

#[test]
fn a_val_mapped_through_a_static_reference_is_an_optional_of_what_it_returns() {
    let p = project("val delegate = identity_resolver.resolve_identity().map(Util::convert);");
    assert_optional_of(&p, "delegate", "com/acme/Token");
}

#[test]
fn a_val_mapped_through_a_constructor_reference_is_an_optional_of_the_type() {
    let p = project("val delegate = identity_resolver.resolve_identity().map(ResolvedIdentity::new);");
    assert_optional_of(&p, "delegate", "com/acme/ResolvedIdentity");
}

#[test]
fn a_val_mapped_through_an_expression_lambda_is_an_optional_of_its_body() {
    let p = project("val delegate = identity_resolver.resolve_identity().map(r -> r.identifier());");
    assert_optional_of(&p, "delegate", "java/lang/String");
}

// ── 5. the reported method: three vals in a row, and a wrong third argument ──────────────────────

const VALIDATION_OUTCOME: &str = "package com.acme;\n\
     public class ValidationOutcome {\n\
     \x20   public static ValidationOutcome allow() { return null; }\n\
     \x20   public static ValidationOutcome deny() { return null; }\n\
     }\n";

/// The reported method verbatim, against a `DelegateClient` whose third parameter is `third`.
fn delegate_project(third: &str) -> Project {
    let client = format!(
        "package com.acme;\n\
         import java.util.Optional;\n\
         public interface DelegateClient {{\n\
         \x20   Optional<Token> delegate_for_user(String username, String identifier, {third} delegate);\n\
         }}\n"
    );
    let user = "package com.acme;\n\
         \n\
         import lombok.RequiredArgsConstructor;\n\
         import lombok.val;\n\
         import org.springframework.stereotype.Component;\n\
         \n\
         @RequiredArgsConstructor @Component\n\
         public class CheckAssignedUser implements AttributeValidator {\n\
         \x20   private static final ValidationOutcome STD_DENIED = ValidationOutcome.deny();\n\
         \x20   private final IdentityResolver identity_resolver;\n\
         \x20   private final DelegateClient client;\n\
         \n\
         \x20   private ValidationOutcome check_delegate(final String username) {\n\
         \x20       val delegate_opt = identity_resolver.resolve_identity();\n\
         \n\
         \x20       if (delegate_opt.isEmpty())\n\
         \x20           return STD_DENIED;\n\
         \n\
         \x20       val delegate = delegate_opt.get();\n\
         \n\
         \x20       val delegate_db_opt =\n\
         \x20           client.delegate_for_user(\n\
         \x20               username\n\
         \x20               , delegate.identifier()\n\
         \x20               , delegate_opt.get()\n\
         \x20           );\n\
         \n\
         \x20       if (delegate_db_opt.isEmpty())\n\
         \x20           return STD_DENIED;\n\
         \n\
         \x20       return ValidationOutcome.allow();\n\
         \x20   }\n\
         }\n";
    Project::new(&[
        ("com/acme/IdentityResolver.java", IDENTITY_RESOLVER),
        ("com/acme/ResolvedIdentity.java", RESOLVED_IDENTITY),
        ("com/acme/LombokIdentity.java", LOMBOK_IDENTITY),
        ("com/acme/Token.java", TOKEN),
        ("com/acme/ValidationOutcome.java", VALIDATION_OUTCOME),
        ("com/acme/DelegateClient.java", &client),
        (USER, user),
    ])
}

fn argument_type_messages(p: &Project) -> Vec<String> {
    p.validate(USER)
        .into_iter()
        .filter(|d| d.code == "argument-type")
        .map(|d| d.message)
        .collect()
}

#[test]
fn the_reported_vals_are_typed_one_from_the_other() {
    let p = delegate_project("Token");
    assert_optional_of(&p, "delegate_opt", "com/acme/ResolvedIdentity");
    assert_eq!(
        declared_type(&p, "delegate").map(|(b, _)| b).as_deref(),
        Some("com/acme/ResolvedIdentity")
    );
    assert_optional_of(&p, "delegate_db_opt", "com/acme/Token");
}

#[test]
fn completion_on_the_reported_val_offers_its_members() {
    let p = delegate_project("Token");
    let labels = p.complete_labels(USER, after(&p, ", delegate."));
    assert!(labels.contains(&"identifier".to_string()), "{labels:?}");
    let labels = p.complete_labels(USER, after(&p, "(delegate_opt."));
    assert!(labels.contains(&"isEmpty".to_string()), "{labels:?}");
}

#[test]
fn the_reported_wrong_third_argument_is_an_error() {
    let p = delegate_project("Token");
    let messages = argument_type_messages(&p);
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(
        messages[0].contains("Argument 3 of `delegate_for_user`")
            && messages[0].contains("`ResolvedIdentity`")
            && messages[0].contains("`Token`"),
        "{messages:?}"
    );
}

#[test]
fn the_reported_call_with_a_matching_third_parameter_is_clean() {
    let p = delegate_project("ResolvedIdentity");
    assert!(argument_type_messages(&p).is_empty(), "{:?}", argument_type_messages(&p));
}

/// And the element flows on: `.orElse(…)` after the `map` is a `String`, and completion after the
/// `map(…)` offers `Optional`'s members.
#[test]
fn the_mapped_element_flows_into_the_rest_of_the_chain() {
    let p = project("val name = identity_resolver.resolve_identity().map(ResolvedIdentity::identifier).orElse(null);");
    let src = p.source(USER);
    let start = at(src, "identity_resolver.resolve_identity()");
    let end = after(&p, ".orElse(null)");
    assert_eq!(p.type_of(USER, start, end).map(|t| t.binary_name).as_deref(), Some("java/lang/String"));

    let p = project("identity_resolver.resolve_identity().map(ResolvedIdentity::identifier).");
    let labels = p.complete_labels(USER, after(&p, "::identifier)."));
    assert!(labels.contains(&"orElse".to_string()), "{labels:?}");
}
