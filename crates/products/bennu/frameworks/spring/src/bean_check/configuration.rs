//! `@Configuration` classes and their `@Bean` methods — what Spring refuses before the context
//! starts.
//!
//! A `@Configuration` class is **subclassed** by CGLIB so that a call from one `@Bean` method to
//! another returns the container's singleton instead of a new object. Everything the subclass cannot
//! override or call — a `private` or `final` `@Bean` method, a `final` class, a class whose
//! constructors are all private — stops the context. With `proxyBeanMethods = false` nothing is
//! subclassed and all of it is legal, so every one of those checks reads that flag first.
//!
//! Two checks hold in every mode: a `void` `@Bean` method (there is no bean), and a non-static
//! `@Bean` method returning a `BeanFactoryPostProcessor` (Spring's own startup warning).

use bennu_facts::prelude::AnnFacts;
use bennu_java::prelude::{annotations_of, has_modifier, modifier_words, simple_name};
use bennu_proto::prelude::severity;
use tree_sitter::Node;

use super::{
    issue, members, method_name, methods, simple_type, BeanIssue, Origins,
    CODE_BEAN_NOT_OVERRIDABLE, CODE_BEAN_VOID, CODE_CONFIGURATION_FINAL,
    CODE_CONFIGURATION_NO_VISIBLE_CONSTRUCTOR, CODE_POST_PROCESSOR_NOT_STATIC,
};

/// `@Configuration` and the annotations that are one by definition.
const CONFIGURATION: &[&str] =
    &["Configuration", "SpringBootConfiguration", "SpringBootApplication", "TestConfiguration"];

/// The `BeanFactoryPostProcessor`s a `@Bean` method returns in practice, by simple name. A project's
/// own post-processor is not recognised — that needs the type hierarchy, and missing it is silence.
const POST_PROCESSORS: &[&str] = &[
    "BeanFactoryPostProcessor",
    "BeanDefinitionRegistryPostProcessor",
    "PropertySourcesPlaceholderConfigurer",
    "PropertyPlaceholderConfigurer",
    "PreferencesPlaceholderConfigurer",
    "CustomScopeConfigurer",
    "CustomEditorConfigurer",
    "CustomAutowireConfigurer",
    "ConfigurationClassPostProcessor",
    "MapperScannerConfigurer",
];

/// Lombok annotations that add a constructor beside the written ones.
const LOMBOK_CONSTRUCTORS: &[&str] =
    &["NoArgsConstructor", "AllArgsConstructor", "RequiredArgsConstructor"];

pub(super) fn check(type_decl: Node<'_>, origins: &Origins<'_>, out: &mut Vec<BeanIssue>) {
    if type_decl.kind() != "class_declaration" {
        return;
    }
    let beans: Vec<(Node<'_>, AnnFacts)> = methods(type_decl)
        .filter_map(|m| origins.first_of(m, &["Bean"]).map(|(_, ann)| (m, ann)))
        .collect();
    let configuration = origins.first_of(type_decl, CONFIGURATION);
    let proxied = configuration.as_ref().is_some_and(|(_, ann)| proxies_bean_methods(ann));

    for (method, bean) in &beans {
        check_bean_method(*method, bean, proxied, origins, out);
    }

    // The class-level checks only where there is something to intercept: a non-static `@Bean`
    // method is what the subclass exists for.
    let Some((annotation, ann)) = configuration else { return };
    let intercepts = beans.iter().any(|(m, _)| !has_modifier(*m, origins.source, "static"));
    if !proxied || !intercepts {
        return;
    }
    let class = type_decl.child_by_field_name("name").map(|n| origins.text(n)).unwrap_or_default();
    let at = (ann.start, ann.end);
    if has_modifier(type_decl, origins.source, "final") {
        out.push(issue(
            CODE_CONFIGURATION_FINAL,
            severity::ERROR,
            format!(
                "@{annotation} class `{class}` must not be final — Spring subclasses it to \
                 intercept its @Bean methods, and refuses to start. Remove `final`, or declare \
                 @{annotation}(proxyBeanMethods = false)"
            ),
            at,
        ));
    }
    if only_private_constructors(type_decl, origins) {
        out.push(issue(
            CODE_CONFIGURATION_NO_VISIBLE_CONSTRUCTOR,
            severity::ERROR,
            format!(
                "@{annotation} class `{class}` has only private constructors — Spring subclasses \
                 it to intercept its @Bean methods, the subclass can call none of them, and the \
                 context fails to start. Make a constructor package-private, or declare \
                 @{annotation}(proxyBeanMethods = false)"
            ),
            at,
        ));
    }
}

fn check_bean_method(
    method: Node<'_>,
    bean: &AnnFacts,
    proxied: bool,
    origins: &Origins<'_>,
    out: &mut Vec<BeanIssue>,
) {
    let name = method_name(method, origins);
    let returns = method.child_by_field_name("type").map(|t| origins.text(t)).unwrap_or_default();
    let words = modifier_words(method, origins.source);
    let is_static = words.iter().any(|w| w == "static");
    let at = (bean.start, bean.end);

    if returns == "void" {
        out.push(issue(
            CODE_BEAN_VOID,
            severity::ERROR,
            format!(
                "@Bean method `{name}` must not be declared void — the bean is the object the \
                 method returns, and Spring refuses to start. Return the object, or remove @Bean"
            ),
            at,
        ));
    }

    // A static `@Bean` method is never intercepted, so its modifiers are its own business.
    if proxied && !is_static {
        let blocking: Vec<&str> =
            ["private", "final"].into_iter().filter(|w| words.iter().any(|x| x == w)).collect();
        if !blocking.is_empty() {
            let either = blocking.join(" or ");
            let written = blocking.join(" ");
            out.push(issue(
                CODE_BEAN_NOT_OVERRIDABLE,
                severity::ERROR,
                format!(
                    "@Bean method `{name}` must not be {either} — Spring subclasses this \
                     @Configuration class to intercept its @Bean methods, and refuses to start. \
                     Remove `{written}`, or declare the class @Configuration(proxyBeanMethods = \
                     false)"
                ),
                at,
            ));
        }
    }

    let returned = simple_type(returns);
    if !is_static && POST_PROCESSORS.contains(&returned) {
        out.push(issue(
            CODE_POST_PROCESSOR_NOT_STATIC,
            severity::WARNING,
            format!(
                "@Bean method `{name}` returns a {returned}, which Spring runs before any other \
                 bean exists — declared non-static, it forces this class to be created that early \
                 too, and the class's own @Autowired, @Value and @PostConstruct are silently \
                 skipped. Declare the method static"
            ),
            at,
        ));
    }
}

/// Whether the configuration is subclassed. A value that is not a literal is read as "not known",
/// and not known is silence.
fn proxies_bean_methods(configuration: &AnnFacts) -> bool {
    matches!(configuration.pair("proxyBeanMethods").map(str::trim), None | Some("true"))
}

fn only_private_constructors(type_decl: Node<'_>, origins: &Origins<'_>) -> bool {
    let constructors: Vec<Node<'_>> = members(type_decl)
        .into_iter()
        .filter(|m| m.kind() == "constructor_declaration")
        .collect();
    if constructors.is_empty() {
        return false;
    }
    let lombok_adds_one = annotations_of(type_decl, origins.source)
        .iter()
        .any(|(written, _)| LOMBOK_CONSTRUCTORS.contains(&simple_name(written)));
    !lombok_adds_one
        && constructors.iter().all(|c| has_modifier(*c, origins.source, "private"))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::with_code;
    use super::*;

    /// The defect the user sees as a startup failure: reported on the `@Bean` itself.
    #[test]
    fn a_private_bean_method_in_a_configuration_is_an_error() {
        let (src, found) = with_code(
            "@Configuration class C { @Bean private Object data() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&src[found[0].start..found[0].end], "@Bean");
        assert_eq!(found[0].severity, "error");
        assert!(found[0].message.contains("must not be private"), "{}", found[0].message);
    }

    #[test]
    fn a_private_final_bean_method_names_both_modifiers_once() {
        let (_, found) = with_code(
            "@Configuration class C { @Bean private final Object data() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("private or final"), "{}", found[0].message);
    }

    /// With `proxyBeanMethods = false` nothing is subclassed, and every one of these is legal.
    #[test]
    fn proxy_bean_methods_false_allows_all_of_it() {
        let (_, all) = super::super::test_support::issues(
            "@Configuration(proxyBeanMethods = false) final class C { private C() { } \
             @Bean private final Object data() { return null; } }",
        );
        assert!(all.is_empty(), "{all:?}");
    }

    /// A `proxyBeanMethods` that is a constant is not known, and not known is silence.
    #[test]
    fn a_non_literal_proxy_flag_is_not_judged() {
        let (_, found) = with_code(
            "@Configuration(proxyBeanMethods = Flags.PROXY) class C { @Bean private Object d() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// Lite mode: a `@Component` with `@Bean` methods is never subclassed.
    #[test]
    fn a_component_with_bean_methods_is_not_subclassed() {
        let (_, found) = with_code(
            "@Component class C { @Bean private Object data() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn a_static_bean_method_is_never_intercepted() {
        let (_, found) = with_code(
            "@Configuration class C { @Bean private static Object data() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// `@SpringBootApplication` is a `@Configuration` by definition.
    #[test]
    fn a_boot_application_is_a_configuration() {
        let (_, found) = with_code(
            "@SpringBootApplication public class App { @Bean final Object data() { return null; } }",
            CODE_BEAN_NOT_OVERRIDABLE,
        );
        assert_eq!(found.len(), 1, "{found:?}");
    }

    #[test]
    fn a_void_bean_method_is_an_error_in_any_class() {
        let (_, found) = with_code("@Component class C { @Bean public void data() { } }", CODE_BEAN_VOID);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, "error");
    }

    #[test]
    fn a_non_static_post_processor_bean_is_a_warning_and_a_static_one_is_not() {
        let (src, found) = with_code(
            "@Configuration class C {\n\
             @Bean public PropertySourcesPlaceholderConfigurer a() { return null; }\n\
             @Bean public static PropertySourcesPlaceholderConfigurer b() { return null; }\n}",
            CODE_POST_PROCESSOR_NOT_STATIC,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, "warning");
        assert!(src[found[0].end..].starts_with(" public PropertySourcesPlaceholderConfigurer a()"));
    }

    #[test]
    fn a_final_configuration_class_is_an_error_on_its_annotation() {
        let (src, found) = with_code(
            "@Configuration public final class C { @Bean Object data() { return null; } }",
            CODE_CONFIGURATION_FINAL,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&src[found[0].start..found[0].end], "@Configuration");
    }

    /// Nothing to intercept, nothing subclassed that could fail on it.
    #[test]
    fn a_final_configuration_with_only_static_bean_methods_is_not_reported() {
        let (_, found) = with_code(
            "@Configuration final class C { @Bean static Object data() { return null; } }",
            CODE_CONFIGURATION_FINAL,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn only_private_constructors_are_an_error_unless_something_adds_a_visible_one() {
        let body = |extra: &str, ctors: &str| {
            format!("{extra} @Configuration class C {{ {ctors} @Bean Object d() {{ return null; }} }}")
        };
        let code = CODE_CONFIGURATION_NO_VISIBLE_CONSTRUCTOR;
        assert_eq!(with_code(&body("", "private C() { }"), code).1.len(), 1);
        assert!(with_code(&body("", "private C() { } C(int a) { }"), code).1.is_empty());
        assert!(with_code(&body("@NoArgsConstructor", "private C(int a) { }"), code).1.is_empty());
        assert!(with_code(&body("", ""), code).1.is_empty(), "the implicit constructor is visible");
    }
}
