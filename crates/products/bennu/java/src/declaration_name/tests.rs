use super::*;

/// The caret marker. Not `|`, which is Java.
const CARET: &str = "‸";

/// The name proposed at the caret marked in `marked`, matching case-insensitively.
fn name_at(marked: &str) -> Option<String> {
    found(marked, false).map(|found| found.name)
}

fn found(marked: &str, case_sensitive: bool) -> Option<DeclarationName> {
    let offset = marked.find(CARET).expect("no caret marker");
    let source = marked.replacen(CARET, "", 1);
    declaration_name_at(&source, offset, case_sensitive, |name, _| name)
}

/// What `spell` is told at the caret marked in `marked`: the kind, and the file's names in order.
fn context_at(marked: &str) -> Option<(DeclarationKind, Vec<String>)> {
    let offset = marked.find(CARET).expect("no caret marker");
    let source = marked.replacen(CARET, "", 1);
    let mut seen = None;
    declaration_name_at(&source, offset, false, |name, context| {
        seen = Some((context.kind, context.declared_in_file.iter().map(|n| n.to_string()).collect()));
        name
    });
    seen
}

fn in_method(body: &str) -> String {
    format!("class A {{\n    void m() {{\n        {body}\n    }}\n}}\n")
}

// ── where it answers ──────────────────────────────────────────────────────────

#[test]
fn a_field_after_its_modifiers_is_named_after_its_type() {
    assert_eq!(name_at("class A {\n    private final MyMsRestClientApi ‸\n}").as_deref(), Some("myMsRestClientApi"));
    assert_eq!(name_at("class A { static URLBuilder ‸ }").as_deref(), Some("urlBuilder"));
}

#[test]
fn a_field_under_annotations_is_still_a_field() {
    let src = "class A {\n    @Autowired\n    @Qualifier(\"fast\")\n    private IdentityResolver ‸\n}";
    assert_eq!(name_at(src).as_deref(), Some("identityResolver"));
}

#[test]
fn a_package_private_field_straight_after_the_brace_is_a_field() {
    assert_eq!(name_at("class A { Order ‸ }").as_deref(), Some("order"));
}

#[test]
fn a_local_at_the_start_of_a_statement_is_named() {
    assert_eq!(name_at(&in_method("final String ‸")).as_deref(), Some("s"));
    assert_eq!(name_at(&in_method("Order ‸")).as_deref(), Some("order"));
    assert_eq!(name_at(&in_method("foo();\n        Order ‸")).as_deref(), Some("order"));
    assert_eq!(name_at(&in_method("if (x) { }\n        Order ‸")).as_deref(), Some("order"));
}

#[test]
fn a_local_in_a_lambda_block_is_named() {
    assert_eq!(name_at(&in_method("run(() -> { Order ‸")).as_deref(), Some("order"));
}

#[test]
fn a_method_parameter_is_named() {
    assert_eq!(name_at("class A { void m(final IdentityResolver ‸) {} }").as_deref(), Some("identityResolver"));
    assert_eq!(name_at("class A { void m(int a, Order ‸) {} }").as_deref(), Some("order"));
    assert_eq!(name_at("class A { public List<Order> find(@Valid Query ‸) {} }").as_deref(), Some("query"));
    assert_eq!(name_at("class A { int[] m(Order ‸) {} }").as_deref(), Some("order"));
}

#[test]
fn a_constructor_parameter_is_named() {
    assert_eq!(name_at("class A { public A(OrderRepository ‸) {} }").as_deref(), Some("orderRepository"));
    assert_eq!(name_at("class A { A(OrderRepository ‸) {} }").as_deref(), Some("orderRepository"));
    assert_eq!(name_at("class A { @Inject A(OrderRepository ‸) {} }").as_deref(), Some("orderRepository"));
}

#[test]
fn a_record_component_is_named() {
    assert_eq!(name_at("record Line(Order ‸)").as_deref(), Some("order"));
}

#[test]
fn a_for_variable_and_a_resource_are_named() {
    assert_eq!(name_at(&in_method("for (Order ‸")).as_deref(), Some("order"));
    assert_eq!(name_at(&in_method("try (Connection ‸")).as_deref(), Some("connection"));
    assert_eq!(name_at(&in_method("try (Connection c = open(); Statement ‸")).as_deref(), Some("statement"));
}

#[test]
fn generic_types_are_named_after_what_they_are() {
    assert_eq!(name_at(&in_method("Map<String, List<Order>> ‸")).as_deref(), Some("map"));
    assert_eq!(name_at(&in_method("List<Order> ‸")).as_deref(), Some("orders"));
    assert_eq!(name_at(&in_method("Optional<Order> ‸")).as_deref(), Some("order"));
    assert_eq!(name_at("class A { private final java.util.Set<com.acme.Entry> ‸ }").as_deref(), Some("entries"));
    assert_eq!(name_at(&in_method("Class<?> ‸")).as_deref(), Some("clazz"));
}

#[test]
fn arrays_and_varargs_are_plural() {
    assert_eq!(name_at(&in_method("Order[] ‸")).as_deref(), Some("orders"));
    assert_eq!(name_at(&in_method("int[] ‸")).as_deref(), Some("ints"));
    assert_eq!(name_at("class A { void m(String... ‸) {} }").as_deref(), Some("strings"));
}

#[test]
fn an_enum_declares_members_after_its_constants() {
    assert_eq!(name_at("enum E { RED, GREEN; private Order ‸ }").as_deref(), Some("order"));
}

#[test]
fn a_switch_case_can_start_a_declaration() {
    assert_eq!(name_at(&in_method("switch (k) { case 1: Order ‸")).as_deref(), Some("order"));
    assert_eq!(name_at(&in_method("switch (k) { case 1: go(); Order ‸")).as_deref(), Some("order"));
}

#[test]
fn a_type_after_a_comment_on_the_line_above_is_still_a_declaration() {
    assert_eq!(name_at(&in_method("// the order\n        Order ‸")).as_deref(), Some("order"));
}

// ── a partial name ────────────────────────────────────────────────────────────

#[test]
fn a_partial_name_that_the_prediction_continues_is_replaced() {
    let src = "class A { private final MyMsRestClientApi my‸ }";
    let found = found(src, false).expect("a proposal");
    assert_eq!(found.name, "myMsRestClientApi");
    let start = src.find("my").unwrap();
    assert_eq!((found.typed_start, found.typed_end), (start, start + 2));
}

#[test]
fn nothing_typed_replaces_nothing() {
    let src = "class A { Order ‸ }";
    let found = found(src, false).expect("a proposal");
    assert_eq!(found.typed_start, found.typed_end);
    assert_eq!(found.typed_end, src.find(CARET).unwrap());
}

#[test]
fn a_partial_name_is_matched_ignoring_case_unless_match_case_is_on() {
    let src = "class A { private final MyMsRestClientApi myms‸ }";
    assert_eq!(found(src, false).map(|f| f.name).as_deref(), Some("myMsRestClientApi"));
    assert_eq!(found(src, true), None);
}

#[test]
fn a_partial_name_the_prediction_does_not_continue_gets_nothing() {
    assert_eq!(name_at("class A { private final MyMsRestClientApi client‸ }"), None);
}

#[test]
fn a_name_already_typed_in_full_gets_nothing() {
    assert_eq!(name_at("class A { private Order order‸ }"), None);
}

/// A capital after the type is another type being written, or a constant: not a name.
#[test]
fn a_capitalised_word_after_the_type_is_not_a_name() {
    assert_eq!(name_at("class A { private Order Or‸ }"), None);
}

// ── where it must not answer ──────────────────────────────────────────────────

#[test]
fn not_after_a_value_keyword() {
    for body in ["return Order ‸", "x = new Order ‸", "throw new IllegalStateException ‸", "yield Order ‸"] {
        assert_eq!(name_at(&in_method(body)), None, "{body}");
    }
    assert_eq!(name_at(&in_method("switch (k) { case FOO ‸")), None);
}

#[test]
fn not_in_an_expression() {
    for body in [
        "x = b + Order ‸",
        "x = Order ‸",
        "if (o instanceof Order ‸",
        "x = (Order) ‸",
        "x = flag ? a : Order ‸",
        "run(x -> Order ‸",
        "boolean b = a < Order ‸",
    ] {
        assert_eq!(name_at(&in_method(body)), None, "{body}");
    }
}

#[test]
fn not_in_an_argument_list() {
    for body in ["foo(Order ‸", "list.add(Order ‸", "x = new Foo(Order ‸", "if (Order ‸", "foo(a, Order ‸", "Foo(Order ‸"] {
        assert_eq!(name_at(&in_method(body)), None, "{body}");
    }
    assert_eq!(name_at("class A { @Qualifier(Order ‸) Foo f; }"), None);
}

#[test]
fn not_in_type_arguments() {
    assert_eq!(name_at("class A { void m(Map<String, Order ‸) {} }"), None);
    assert_eq!(name_at(&in_method("Map<String, Order ‸")), None);
    assert_eq!(name_at(&in_method("List<Order ‸")), None);
}

#[test]
fn not_in_a_string_or_a_comment() {
    assert_eq!(name_at(&in_method("String s = \"Order ‸")), None);
    assert_eq!(name_at(&in_method("// Order ‸")), None);
    assert_eq!(name_at(&in_method("/* Order ‸ */")), None);
    assert_eq!(name_at(&in_method("String s = \"\"\"\n Order ‸")), None);
}

#[test]
fn not_after_an_annotation() {
    assert_eq!(name_at("class A {\n    @Autowired\n    ‸\n}"), None);
    assert_eq!(name_at("class A { @Autowired ‸ }"), None);
    assert_eq!(name_at("class A { @Qualifier(\"x\") ‸ }"), None);
    assert_eq!(name_at("class A { @javax.inject.Named ‸ }"), None);
}

#[test]
fn not_in_a_type_declaration_header() {
    for src in [
        "class Order ‸",
        "public interface Repository ‸",
        "enum Kind ‸",
        "record Line ‸",
        "class A extends Base ‸",
        "class A implements Runnable ‸",
        "class A implements Runnable, Closeable ‸",
        "class O { class A extends Base implements Runnable, Closeable ‸ }",
        "class A<T extends Comparable ‸",
        "class A { void m() throws IOException ‸ }",
    ] {
        assert_eq!(name_at(src), None, "{src}");
    }
}

#[test]
fn not_for_an_enum_constant() {
    assert_eq!(name_at("enum E { RED ‸ }"), None);
    assert_eq!(name_at("enum E { RED, GREEN ‸ }"), None);
}

#[test]
fn not_in_an_array_initializer() {
    assert_eq!(name_at(&in_method("Object[] a = { Order ‸")), None);
    assert_eq!(name_at(&in_method("x = new Object[] { Order ‸")), None);
}

/// The type a generic method returns is followed by the method's name, which is not a variable's.
#[test]
fn not_after_a_generic_methods_type_parameters() {
    assert_eq!(name_at("class A { public <T> T ‸ }"), None);
}

#[test]
fn not_straight_after_a_switchs_brace() {
    assert_eq!(name_at(&in_method("switch (k) { Order ‸")), None);
}

#[test]
fn not_without_a_type() {
    assert_eq!(name_at(&in_method("var ‸")), None);
    assert_eq!(name_at(&in_method("order ‸")), None);
    assert_eq!(name_at(&in_method("final ‸")), None);
    assert_eq!(name_at("class A { private ‸ }"), None);
    assert_eq!(name_at("Order ‸"), None);
}

#[test]
fn not_without_a_space_after_the_type_or_across_a_line_break() {
    assert_eq!(name_at(&in_method("Order‸")), None);
    assert_eq!(name_at(&in_method("Order\n        ‸")), None);
    assert_eq!(name_at(&in_method("Order /* c */ ‸")), None);
}

#[test]
fn not_with_the_caret_inside_a_word() {
    assert_eq!(name_at(&in_method("Order ‸order;")), None);
}

#[test]
fn an_offset_out_of_range_or_inside_a_character_is_refused() {
    assert_eq!(declaration_name_at("class A { Order ", 99, false, |name, _| name), None);
    let src = "class A { è Order ";
    let inside = src.find('è').unwrap() + 1;
    assert_eq!(declaration_name_at(src, inside, false, |name, _| name), None);
}

// ── what the spelling is told ─────────────────────────────────────────────────

#[test]
fn the_kind_of_declaration_is_told() {
    let kind = |marked: &str| context_at(marked).map(|(kind, _)| kind);
    assert_eq!(kind("class A { private final Order ‸ }"), Some(DeclarationKind::Field));
    assert_eq!(kind("enum E { RED; private Order ‸ }"), Some(DeclarationKind::Field));
    assert_eq!(kind(&in_method("Order ‸")), Some(DeclarationKind::Local));
    assert_eq!(kind(&in_method("for (Order ‸")), Some(DeclarationKind::Local));
    assert_eq!(kind(&in_method("try (Connection c = open(); Statement ‸")), Some(DeclarationKind::Local));
    assert_eq!(kind(&in_method("switch (k) { case 1: Order ‸")), Some(DeclarationKind::Local));
    assert_eq!(kind("class A { void m(int a, Order ‸) {} }"), Some(DeclarationKind::Parameter));
    assert_eq!(kind("class A { A(OrderRepository ‸) {} }"), Some(DeclarationKind::Parameter));
    assert_eq!(kind("class A { void m() { run(new Runnable() { Order ‸ }); } }"), Some(DeclarationKind::Field));
}

#[test]
fn every_variable_the_file_declares_is_told_whatever_its_scope() {
    let src = "class A {\n    private IdentityResolver identity_resolver;\n    void m(String first_name) { int x = 1; }\n    private Order ‸\n}";
    let (_, names) = context_at(src).expect("a declaration");
    assert_eq!(names, ["identity_resolver", "first_name", "x"]);
}

#[test]
fn the_names_a_file_declares_are_listed_even_mid_edit() {
    let src = "class A {\n    private IdentityResolver identity_resolver;\n    void m(String first_name) { orders.fo";
    assert_eq!(declared_variable_names(src), ["identity_resolver", "first_name"]);
}

/// The digit is added to the name that will be written, so a clash in the project's spelling counts.
#[test]
fn a_spelled_name_already_taken_gets_a_digit() {
    let marked = "class A { private Order my_order; private MyOrder ‸ }";
    let offset = marked.find(CARET).unwrap();
    let source = marked.replacen(CARET, "", 1);
    let found = declaration_name_at(&source, offset, false, |_, _| "my_order".to_string());
    assert_eq!(found.map(|f| f.name).as_deref(), Some("my_order1"));
}

// ── names already taken ───────────────────────────────────────────────────────

#[test]
fn a_field_name_already_taken_gets_a_digit() {
    assert_eq!(name_at("class A { private Order order; private Order ‸ }").as_deref(), Some("order1"));
    assert_eq!(
        name_at("class A { private Order order; private Order order1; private Order ‸ }").as_deref(),
        Some("order2"),
    );
}

#[test]
fn a_field_declared_further_down_is_taken_too() {
    assert_eq!(name_at("class A {\n    private Order ‸\n    private Order order;\n}").as_deref(), Some("order1"));
}

#[test]
fn a_parameter_already_in_the_list_is_taken() {
    assert_eq!(name_at("class A { void m(Order order, Order ‸) {} }").as_deref(), Some("order1"));
    assert_eq!(name_at("class A { void m(Order ‸, Order order) {} }").as_deref(), Some("order1"));
}

#[test]
fn a_local_or_parameter_in_scope_is_taken_for_a_local() {
    assert_eq!(name_at("class A { void m(Order order) { Order ‸ } }").as_deref(), Some("order1"));
    assert_eq!(name_at(&in_method("Order order = load();\n        Order ‸")).as_deref(), Some("order1"));
    assert_eq!(name_at(&in_method("for (Order order : orders) { Order ‸")).as_deref(), Some("order1"));
}

/// Shadowing a field is legal, and a constructor parameter named like its field is the norm.
#[test]
fn a_field_is_not_taken_for_a_parameter_or_a_local() {
    assert_eq!(name_at("class A { private Order order; A(Order ‸) {} }").as_deref(), Some("order"));
    assert_eq!(name_at("class A { private Order order; void m() { Order ‸ } }").as_deref(), Some("order"));
}

#[test]
fn a_name_in_a_closed_sibling_scope_is_not_taken() {
    assert_eq!(name_at("class A { void a() { Order order; } void b() { Order ‸ } }").as_deref(), Some("order"));
    assert_eq!(name_at(&in_method("if (x) { Order order; }\n        Order ‸")).as_deref(), Some("order"));
    assert_eq!(name_at("class A { void a(Order order) {} void b() { Order ‸ } }").as_deref(), Some("order"));
}

/// A partial name is matched against the name that will actually be written.
#[test]
fn a_partial_name_continues_the_suffixed_name() {
    assert_eq!(name_at("class A { Order order; Order ord‸ }").as_deref(), Some("order1"));
}
