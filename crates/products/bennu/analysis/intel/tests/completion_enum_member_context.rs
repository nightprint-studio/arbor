//! After `EnumType.TI|` the constants that share a word with the names around the caret rise: a local called `pin`
//! puts `TIPO_INVIO_OFFERTA_PIN` ahead of the other `TIPO_…` constants that match the typed `TI` equally.

mod common;
use common::*;

fn labels(context: &str) -> Vec<String> {
    let p = Project::new(&[
        (
            "ECommunicationType.java",
            "package z;\npublic enum ECommunicationType { TIPO_BUSTA_ECONOMICA, TIPO_BUSTA_TECNICA, TIPO_INVIO_OFFERTA_PIN, TIPO_BUSTA_RIEPILOGO }\n",
        ),
        ("Use.java", &format!("package z;\npublic class Use {{\n    void run() {{\n{context}\n    }}\n}}\n")),
    ]);
    let src = p.source("Use.java");
    let off = at(src, "ECommunicationType.TI") + "ECommunicationType.TI".len();
    p.complete_labels("Use.java", off)
}

#[test]
fn a_constant_sharing_a_word_with_a_nearby_name_leads() {
    let l = labels("        java.util.List<String> pin = null;\n        use(ECommunicationType.TI);");
    assert_eq!(l.first().map(String::as_str), Some("TIPO_INVIO_OFFERTA_PIN"), "{l:?}");
}

#[test]
fn with_no_word_in_common_the_declaration_order_stands() {
    let l = labels("        use(ECommunicationType.TI);");
    assert_eq!(l.first().map(String::as_str), Some("TIPO_BUSTA_ECONOMICA"), "{l:?}");
}
