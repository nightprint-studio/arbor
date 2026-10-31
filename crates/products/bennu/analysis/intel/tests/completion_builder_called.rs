//! A builder does not offer the setters the chain just called ahead of the ones it has not: after
//! `Device.builder().name("a").` the list opens on `id`, `type`, `build`, and `name` sinks to the end.

mod common;
use common::*;

fn labels(chain: &str) -> Vec<String> {
    let p = Project::new(&[
        (
            "Device.java",
            "package z;\npublic class Device {\n    public static Device builder() { return null; }\n    public Device id(long v) { return this; }\n    public Device name(String v) { return this; }\n    public Device type(String v) { return this; }\n    public Device build() { return this; }\n    public String label() { return null; }\n}\n",
        ),
        ("Use.java", &format!("package z;\npublic class Use {{\n    void run() {{\n        {chain};\n    }}\n}}\n")),
    ]);
    let src = p.source("Use.java");
    let off = at(src, ";\n    }") ;
    p.complete_labels("Use.java", off)
}

fn pos(l: &[String], name: &str) -> usize {
    l.iter().position(|x| x == name).unwrap_or_else(|| panic!("{name} missing from {l:?}"))
}

#[test]
fn setters_already_called_sink_below_the_ones_not_yet_called() {
    let l = labels("Device.builder().name(\"a\").id(1).");
    assert!(pos(&l, "type") < pos(&l, "name"), "{l:?}");
    assert!(pos(&l, "type") < pos(&l, "id"), "{l:?}");
    assert!(pos(&l, "build") < pos(&l, "name"), "{l:?}");
}

#[test]
fn a_method_that_does_not_return_the_receiver_is_not_demoted() {
    let l = labels("Device.builder().label().length().");
    // `label()` returns a String, so the receiver here is an int: nothing of Device is offered at all
    assert!(!l.iter().any(|x| x == "build"));
    let l = labels("Device.builder().name(\"a\").");
    assert!(pos(&l, "label") < pos(&l, "name"), "{l:?}");
}

#[test]
fn nothing_called_keeps_the_usual_order() {
    let l = labels("Device.builder().");
    for m in ["id", "name", "type", "build"] {
        assert!(l.contains(&m.to_string()), "{m} missing from {l:?}");
    }
}
