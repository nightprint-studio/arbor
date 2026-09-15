//! The assertion that asserts nothing.
//!
//! `assertThat(x)` builds an assertion object and stops. Nothing is compared until a check —
//! `isEqualTo`, `isEmpty`, `contains` — is called on it, and the methods that only *configure* the
//! object (`as`, `extracting`, `usingComparator`) are not checks. A statement that ends there
//! compiles, runs, and passes whatever the value is: the one kind of test that can never go red.
//!
//! The list of configuring methods is closed and deliberately short. A method missing from it is read
//! as a check, which under-reports; a checking method wrongly in it would report a real assertion as
//! broken, and that is the mistake this crate must not make.

use bennu_proto::prelude::{severity, Diagnostic};
use tree_sitter::Node;

use crate::chain::{call_name, chain_of, statement_call};
use crate::ext::CODE_ASSERTS_NOTHING;
use crate::resolve::assertj_entry;
use crate::unit::Unit;

/// Methods that return an assertion object without checking anything.
const NON_CHECKING: &[&str] = &[
    "as",
    "describedAs",
    "withFailMessage",
    "overridingErrorMessage",
    "withRepresentation",
    "withThreadDumpOnError",
    "usingComparator",
    "usingDefaultComparator",
    "usingElementComparator",
    "usingRecursiveComparison",
    "usingRecursiveFieldByFieldElementComparator",
    "extracting",
    "extractingResultOf",
    "flatExtracting",
    "filteredOn",
    "asInstanceOf",
    "asString",
    "asList",
    "inHexadecimal",
    "inBinary",
    "inUnicode",
    "map",
];

pub(crate) fn diagnostics(unit: &Unit<'_>) -> Vec<Diagnostic> {
    unit.nodes()
        .into_iter()
        .filter(|n| n.kind() == "expression_statement")
        .filter_map(|statement| {
            let entry = unchecked_entry(unit, statement)?;
            Some(Diagnostic {
                message: format!(
                    "this `{entry}(…)` checks nothing — with no assertion such as isEqualTo(…) after \
                     it, the statement compiles and passes whatever the value is"
                ),
                severity: severity::WARNING.to_string(),
                code: CODE_ASSERTS_NOTHING.to_string(),
                start: statement.start_byte(),
                end: statement.end_byte(),
            })
        })
        .collect()
}

/// The entry point's name when `statement` is an AssertJ chain with no check in it.
fn unchecked_entry<'s>(unit: &Unit<'s>, statement: Node<'_>) -> Option<&'s str> {
    let chain = chain_of(statement_call(statement)?)?;
    assertj_entry(unit, chain.root)?;
    let unchecked = chain.links.iter().all(|link| NON_CHECKING.contains(&call_name(*link, unit.source)));
    unchecked.then(|| call_name(chain.root, unit.source))
}

#[cfg(test)]
mod tests {
    use crate::ext::CODE_ASSERTS_NOTHING as CODE;
    use crate::testing::squiggled;

    fn test_class(imports: &str, body: &str) -> String {
        format!("package com.acme;\n\n{imports}\n\nclass OrderTest {{\n    void totals() {{\n{body}\n    }}\n}}\n")
    }

    const ASSERTJ: &str = "import static org.assertj.core.api.Assertions.assertThat;\nimport static org.assertj.core.api.Assertions.assertThatThrownBy;";

    #[test]
    fn an_entry_point_with_nothing_after_it_is_reported() {
        let src = test_class(ASSERTJ, "        assertThat(total);");
        assert_eq!(squiggled(&src, CODE), ["assertThat(total);"]);
    }

    #[test]
    fn a_description_is_not_a_check() {
        let src = test_class(ASSERTJ, "        assertThat(total).as(\"the total\").usingComparator(byValue);");
        assert_eq!(squiggled(&src, CODE).len(), 1);
    }

    #[test]
    fn a_chain_that_ends_in_a_check_is_left_alone() {
        let body = "        assertThat(total).isEqualTo(3);\n\
                    \x20       assertThat(total).as(\"the total\").isPositive();\n\
                    \x20       assertThat(names).extracting(\"id\").containsExactly(1, 2);\n\
                    \x20       assertThat(names).first();\n\
                    \x20       assertThatThrownBy(() -> order.pay());";
        assert!(squiggled(&test_class(ASSERTJ, body), CODE).is_empty());
    }

    #[test]
    fn an_exception_chain_is_not_an_assertion_object() {
        let imports = "import static org.assertj.core.api.Assertions.assertThatExceptionOfType;";
        let body = "        assertThatExceptionOfType(IllegalStateException.class).isThrownBy(() -> order.pay());";
        assert!(squiggled(&test_class(imports, body), CODE).is_empty());
    }

    /// Hamcrest's `assertThat` asserts on its own — and it is only reached through an import that is
    /// not AssertJ's.
    #[test]
    fn somebody_elses_assert_that_is_not_judged() {
        let hamcrest = "import static org.hamcrest.MatcherAssert.assertThat;\nimport org.assertj.core.api.SoftAssertions;";
        assert!(squiggled(&test_class(hamcrest, "        assertThat(total, is(3));"), CODE).is_empty());
        let helper = "import static com.acme.Checks.assertThat;\nimport static org.assertj.core.api.Assertions.*;";
        assert!(squiggled(&test_class(helper, "        assertThat(total);"), CODE).is_empty());
    }

    #[test]
    fn a_method_of_the_same_name_in_the_file_shadows_the_import() {
        let src = format!(
            "{}\n",
            test_class(ASSERTJ, "        assertThat(total);").replace(
                "    void totals()",
                "    static Object assertThat(Object o) { return o; }\n    void totals()"
            )
        );
        assert!(squiggled(&src, CODE).is_empty());
    }

    #[test]
    fn soft_assertions_bdd_and_with_assertions_are_entry_points_too() {
        let soft = test_class(
            "import org.assertj.core.api.SoftAssertions;",
            "        SoftAssertions softly = new SoftAssertions();\n        softly.assertThat(total);\n        softly.assertAll();",
        );
        assert_eq!(squiggled(&soft, CODE), ["softly.assertThat(total);"]);

        let bdd = test_class("import static org.assertj.core.api.BDDAssertions.then;", "        then(total);");
        assert_eq!(squiggled(&bdd, CODE), ["then(total);"]);

        let with = "package com.acme;\n\nimport org.assertj.core.api.WithAssertions;\n\nclass OrderTest implements WithAssertions {\n    void totals() {\n        assertThat(total).as(\"the total\");\n    }\n}\n";
        assert_eq!(squiggled(with, CODE), ["assertThat(total).as(\"the total\");"]);
    }

    /// BDDMockito has a `then` too, and `then(repository).should()` verifies a call.
    #[test]
    fn mockitos_then_is_not_assertjs() {
        let imports = "import static org.mockito.BDDMockito.then;\nimport static org.assertj.core.api.Assertions.assertThat;";
        assert!(squiggled(&test_class(imports, "        then(repository).should().save(order);"), CODE).is_empty());
    }
}
