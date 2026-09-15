//! The catalogue, asked what it writes for shapes built by hand — the shapes themselves are tested
//! against a resolver in `shape`.

use super::*;

const UNIT: &str = "    ";

fn written(text: &str, imports: &[&str]) -> Written {
    Written { text: text.to_string(), imports: imports.iter().map(|i| i.to_string()).collect() }
}

fn order_element() -> Element {
    Element { ty: written("Order", &["com.acme.Order"]), name: "order".to_string(), primitive: None }
}

fn value(text: &str, shape: ValueShape) -> Subject {
    Subject::Value { text: text.to_string(), shape: ValueShape { repeatable: !text.contains('('), ..shape } }
}

fn orders(text: &str) -> Subject {
    value(
        text,
        ValueShape {
            ty: written("List<Order>", &["com.acme.Order", "java.util.List"]),
            name: "orders".to_string(),
            iterable_element: Some(order_element()),
            collection: true,
            length: Some(Length::Size),
            ..ValueShape::default()
        },
    )
}

fn found(text: &str) -> Subject {
    value(
        text,
        ValueShape {
            ty: written("Optional<Order>", &["com.acme.Order", "java.util.Optional"]),
            name: "order".to_string(),
            optional: Some(OptionalShape::Of(order_element())),
            ..ValueShape::default()
        },
    )
}

fn array(text: &str, element: &'static str) -> Subject {
    value(
        text,
        ValueShape {
            ty: written(&format!("{element}[]"), &[]),
            name: text.to_string(),
            array_element: Some(Element { ty: written(element, &[]), name: "id".to_string(), primitive: Some(element) }),
            length: Some(Length::Field),
            ..ValueShape::default()
        },
    )
}

fn reference(text: &str) -> Subject {
    value(text, ValueShape { ty: written("Shape", &["com.acme.Shape"]), name: "shape".to_string(), ..ValueShape::default() })
}

/// The one expansion named `name` at `level`, if it is offered.
fn expand(subject: &Subject, level: u32, name: &str) -> Option<Expansion> {
    let ctx = PostfixContext { level, unit: UNIT };
    expansions(subject, &ctx, &|candidate| candidate == name).into_iter().next()
}

fn text(subject: &Subject, level: u32, name: &str) -> String {
    expand(subject, level, name).unwrap_or_else(|| panic!("`.{name}` is not offered at Java {level}")).text
}

#[test]
fn for_writes_the_element_type_below_java_10_and_var_from_it() {
    let java8 = expand(&orders("orders"), 8, "for").expect("offered");
    assert_eq!(java8.text, "for (Order order : orders) {\n    \n}");
    assert_eq!(java8.imports, ["com.acme.Order"]);
    let java10 = expand(&orders("orders"), 10, "for").expect("offered");
    assert_eq!(java10.text, "for (var order : orders) {\n    \n}");
    assert!(java10.imports.is_empty());
}

#[test]
fn var_declares_with_the_type_until_the_module_has_var() {
    assert_eq!(text(&orders("orders"), 8, "var"), "List<Order> orders = orders;");
    assert_eq!(text(&orders("orders"), 11, "var"), "var orders = orders;");
}

#[test]
fn an_array_streams_through_arrays_where_an_overload_exists() {
    let ints = expand(&array("ids", "int"), 8, "stream").expect("int[] has an overload");
    assert_eq!(ints.text, "Arrays.stream(ids)");
    assert_eq!(ints.imports, ["java.util.Arrays"]);
    assert!(expand(&array("letters", "char"), 8, "stream").is_none(), "there is no Arrays.stream(char[])");
}

/// `stream()` is the collection's own member, right above in the list.
#[test]
fn a_collection_is_not_offered_a_stream_template() {
    assert!(expand(&orders("orders"), 8, "stream").is_none());
}

#[test]
fn an_indexed_loop_counts_to_what_the_type_measures() {
    assert_eq!(text(&orders("orders"), 8, "fori"), "for (int i = 0; i < orders.size(); i++) {\n    \n}");
    assert_eq!(text(&array("ids", "int"), 8, "fori"), "for (int i = 0; i < ids.length; i++) {\n    \n}");
}

/// The bound of `fori` is evaluated per iteration; `forr` evaluates it once.
#[test]
fn a_call_is_not_an_indexed_loops_bound_but_counts_down_fine() {
    assert!(expand(&orders("repo.findAll()"), 8, "fori").is_none());
    assert_eq!(
        text(&orders("repo.findAll()"), 8, "forr"),
        "for (int i = repo.findAll().size() - 1; i >= 0; i--) {\n    \n}",
    );
}

#[test]
fn if_present_or_else_is_the_method_from_java_9_and_an_if_before_it() {
    assert_eq!(
        text(&found("found"), 9, "ifpe"),
        "found.ifPresentOrElse(order -> {\n    \n}, () -> {\n    \n});",
    );
    assert_eq!(
        text(&found("found"), 8, "ifpe"),
        "if (found.isPresent()) {\n    Order order = found.get();\n    \n} else {\n    \n}",
    );
}

/// The Java 8 form names the optional twice; a call is declared into a local so it runs once.
#[test]
fn an_optional_that_does_work_is_declared_once_before_it_is_unwrapped() {
    let expansion = expand(&found("repo.findById(id)"), 8, "ifpe").expect("offered");
    assert_eq!(
        expansion.text,
        "Optional<Order> found = repo.findById(id);\nif (found.isPresent()) {\n    Order order = found.get();\n    \n} else {\n    \n}",
    );
    assert!(expansion.imports.contains(&"java.util.Optional".to_string()));
    // Tab visits the local once: its three occurrences are one group, listed together and first.
    let firsts: Vec<&str> = expansion.stops[..3].iter().map(|s| &expansion.text[s.start..s.end]).collect();
    assert_eq!(firsts, ["found", "found", "found"]);
    assert!(expansion.stops[..3].iter().all(|s| s.group == 2));
}

#[test]
fn if_get_takes_the_value_out_with_var_where_the_module_has_it() {
    assert_eq!(text(&found("found"), 11, "ifget"), "if (found.isPresent()) {\n    var order = found.get();\n    \n}");
}

#[test]
fn an_optional_stream_on_java_8_goes_through_stream_of() {
    let expansion = expand(&found("found"), 8, "stream").expect("offered on 8");
    assert_eq!(expansion.text, "found.map(Stream::of).orElseGet(Stream::empty)");
    assert_eq!(expansion.imports, ["java.util.stream.Stream"]);
    assert!(expand(&found("found"), 9, "stream").is_none(), "Optional.stream() is a member from 9");
}

#[test]
fn opt_wraps_by_what_the_value_can_be() {
    let int = value("count", ValueShape { primitive: Some("int"), ..ValueShape::default() });
    let wrapped = expand(&int, 8, "opt").expect("offered");
    assert_eq!((wrapped.text.as_str(), wrapped.imports[0].as_str()), ("OptionalInt.of(count)", "java.util.OptionalInt"));
    assert_eq!(text(&reference("shape"), 8, "opt"), "Optional.ofNullable(shape)");
    let created = value("new Shape()", ValueShape { non_null: true, ..ValueShape::default() });
    assert_eq!(text(&created, 8, "opt"), "Optional.of(new Shape())");
}

#[test]
fn instanceof_binds_a_pattern_from_java_16_and_casts_before_it() {
    assert_eq!(text(&reference("shape"), 16, "inst"), "if (shape instanceof Object value) {\n    \n}");
    assert_eq!(
        text(&reference("shape"), 11, "inst"),
        "if (shape instanceof Object) {\n    Object value = (Object) shape;\n    \n}",
    );
    assert!(expand(&reference("shapes.next()"), 11, "inst").is_none(), "the cast would call it twice");
}

#[test]
fn null_checks_are_not_offered_where_null_cannot_be() {
    let int = value("count", ValueShape { primitive: Some("int"), ..ValueShape::default() });
    assert!(expand(&int, 8, "nn").is_none());
    let created = value("new Shape()", ValueShape { non_null: true, ..ValueShape::default() });
    assert!(expand(&created, 8, "nn").is_none());
    assert_eq!(text(&reference("shape"), 8, "nn"), "if (shape != null) {\n    \n}");
}

#[test]
fn nothing_is_offered_that_the_module_cannot_compile() {
    assert!(expand(&reference("shape"), 7, "opt").is_none());
    assert!(expand(&reference("shape"), 7, "lambda").is_none());
    assert!(expand(&orders("orders"), 7, "forEach").is_none());
    assert!(expand(&found("found"), 7, "ifp").is_none());
    let closeable = value("in", ValueShape { closeable: true, ..ValueShape::default() });
    assert!(expand(&closeable, 6, "twr").is_none());
    let name = value("name", ValueShape { string: true, switchable: true, ..ValueShape::default() });
    assert!(expand(&name, 6, "switch").is_none());
    assert!(expand(&name, 7, "switch").is_some());
}

#[test]
fn a_labelled_print_escapes_the_expression_it_quotes() {
    let literal = value("\"a\"", ValueShape { string: true, non_null: true, ..ValueShape::default() });
    assert_eq!(text(&literal, 8, "soutv"), "System.out.println(\"\\\"a\\\" = \" + \"a\");");
}

#[test]
fn a_class_name_is_only_offered_new() {
    let class = Subject::Type { text: "Order".to_string() };
    let ctx = PostfixContext { level: 21, unit: UNIT };
    let names: Vec<&str> = expansions(&class, &ctx, &|_| true).iter().map(|e| e.name).collect();
    assert_eq!(names, ["new"]);
    assert_eq!(text(&class, 21, "new"), "new Order()");
}

#[test]
fn a_void_call_can_only_be_wrapped_in_a_try() {
    let call = value("run()", ValueShape { void: true, ..ValueShape::default() });
    let ctx = PostfixContext { level: 21, unit: UNIT };
    let names: Vec<&str> = expansions(&call, &ctx, &|_| true).iter().map(|e| e.name).collect();
    assert_eq!(names, ["try"]);
}

/// Whatever a template writes, its stops point inside it — a stop past the end would be armed at a
/// position the editor has to invent.
#[test]
fn every_stop_lies_inside_its_expansion() {
    let subjects = [orders("orders"), orders("repo.findAll()"), found("found"), found("repo.findById(id)"), array("ids", "int"), reference("shape")];
    for level in [6, 8, 9, 16, 21] {
        let ctx = PostfixContext { level, unit: UNIT };
        for subject in &subjects {
            for expansion in expansions(subject, &ctx, &|_| true) {
                for stop in &expansion.stops {
                    assert!(
                        stop.start <= stop.end && stop.end <= expansion.text.len(),
                        "`.{}` at Java {level}: {stop:?} outside {:?}",
                        expansion.name,
                        expansion.text,
                    );
                }
            }
        }
    }
}
