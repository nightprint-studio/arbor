//! The model end to end: load the ONNX graphs, build a prompt, complete a line. Needs the model files, so it
//! skips itself unless `BENNU_FLC_MODEL` names a folder that has them:
//! `BENNU_FLC_MODEL=/path/to/model cargo test -p bennu-flc --features onnx --release -- --nocapture`.
#![cfg(feature = "onnx")]

use bennu_flc::prelude::*;

#[test]
fn completes_a_line_and_respects_a_constraint() {
    let Ok(dir) = std::env::var("BENNU_FLC_MODEL") else {
        eprintln!("BENNU_FLC_MODEL not set: skipping");
        return;
    };
    let mut model = LineModel::load(&dir).expect("the model loads");
    let prefix = "package p;\n\nimport java.util.List;\n\nclass A {\n    int count(List<String> items) {\n        return items.";
    let suffix = "\n    }\n}\n";
    let mut prompt = bennu_flc::prompt::build(&model.tk, "//@ List: size isEmpty get add\n", prefix, suffix);
    prompt.fit(model.ctx - MAX_NEW_TOKENS);

    let started = std::time::Instant::now();
    let free = model.complete(&prompt.ids, 1, None).expect("completes");
    let ms = started.elapsed().as_millis();
    let best = free.first().expect("a candidate");
    eprintln!("free: {:?}  confidence {:.2}  ({ms} ms)", best.text, best.confidence);
    assert!(!best.text.is_empty());
    assert!(!best.text.contains('\n'));

    // constrained to two names, whatever the model preferred, the member it writes is one of them
    let names = ["size", "isEmpty"];
    let constraint = names_constraint(&names, "");
    let bound = model.complete(&prompt.ids, 1, Some(&constraint)).expect("completes");
    let text = &bound.first().expect("a candidate").text;
    eprintln!("bound: {text:?}");
    assert!(text.starts_with("size") || text.starts_with("isEmpty"), "{text}");
}

/// Reusing what the last prompt left behind must give exactly what a fresh read gives: same text, same confidence.
#[test]
fn a_warm_model_answers_like_a_cold_one() {
    let Ok(dir) = std::env::var("BENNU_FLC_MODEL") else {
        eprintln!("BENNU_FLC_MODEL not set: skipping");
        return;
    };
    let mut warm = LineModel::load(&dir).expect("the model loads");
    let filler: String = (0..40).map(|i| format!("    int field{i} = {i};\n")).collect();
    let make = |model: &LineModel, header: &str, tail: &str| {
        let prefix = format!("package p;\n\nimport java.util.List;\n\nclass A {{\n{filler}    int count(List<String> items) {{\n        return items.{tail}");
        let mut p = bennu_flc::prompt::build(&model.tk, header, &prefix, "\n    }\n}\n");
        p.fit(model.ctx - MAX_NEW_TOKENS);
        p.ids
    };
    let agree = |a: &[Candidate], b: &[Candidate], what: &str| {
        let (a, b) = (a.first().expect("a candidate"), b.first().expect("a candidate"));
        assert_eq!(a.text, b.text, "{what}");
        assert!((a.confidence - b.confidence).abs() < 1e-3, "{what}: {} vs {}", a.confidence, b.confidence);
    };
    let head = "//@ List: size isEmpty get add\n";
    let base = make(&warm, head, "");
    let cases = [
        ("the same prompt again", base.clone()),
        ("a changed tail", make(&warm, head, "si")),
        ("a changed tail again", make(&warm, head, "isE")),
        ("a changed head", make(&warm, "//@ List: add addAll size\n", "")),
        ("a shorter prompt that starts the last one", base[..base.len() - 3].to_vec()),
    ];
    warm.complete(&base, 1, None).expect("completes");
    for (what, ids) in &cases {
        let w = warm.complete(ids, 1, None).expect("completes");
        // a model that has read nothing before is the reference
        let mut fresh = LineModel::load(&dir).expect("the model loads");
        let f = fresh.complete(ids, 1, None).expect("completes");
        agree(&w, &f, what);
    }
}

/// What the reuse saves: a ~600-token prompt asked again, and again with the typed tail changed.
#[test]
fn reusing_the_prompt_is_faster() {
    let Ok(dir) = std::env::var("BENNU_FLC_MODEL") else {
        eprintln!("BENNU_FLC_MODEL not set: skipping");
        return;
    };
    let mut model = LineModel::load(&dir).expect("the model loads");
    let filler: String = (0..60).map(|i| format!("    int field{i} = compute({i}) + other{i};\n")).collect();
    let make = |model: &LineModel, tail: &str| {
        let prefix = format!("package p;\n\nimport java.util.List;\n\nclass A {{\n{filler}    int count(List<String> items) {{\n        return items.{tail}");
        let mut p = bennu_flc::prompt::build(&model.tk, "//@ List: size isEmpty get add\n", &prefix, "\n    }\n}\n");
        p.fit(model.ctx - MAX_NEW_TOKENS);
        p.ids
    };
    let time = |model: &mut LineModel, ids: &[u16]| {
        let t = std::time::Instant::now();
        model.complete(ids, 1, None).expect("completes");
        t.elapsed().as_millis()
    };
    let a = make(&model, "");
    let b = make(&model, "si");
    eprintln!("prompt of {} tokens", a.len());
    let cold = time(&mut model, &a);
    let same = time(&mut model, &a);
    let tail = time(&mut model, &b);
    eprintln!("cold {cold} ms, same prompt {same} ms, changed tail {tail} ms");
    assert!(same <= cold);
}
