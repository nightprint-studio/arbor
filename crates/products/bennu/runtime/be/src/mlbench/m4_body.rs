//! `flc_body` on held-out accessors: for every `getX` / `isX` / `hasX` / `setX` method with a body of 1–8 statements, the body is
//! emptied, the caret put on the empty line under the signature, and the layer asked. No index is needed — the layer reads the file.
//!
//! `cargo test -p bennu-be mlbench::m4_body -- --ignored --nocapture`

use super::common::*;
use super::m4_methods::{methods_of, normalize};
use crate::flc_body;

#[test]
#[ignore = "needs the public corpus: see the module docs"]
fn flc_body_on_held_out_accessors() {
    let (mut methods, mut proposed, mut exact, mut fluent_ok) = ([0usize; 2], [0usize; 2], [0usize; 2], 0usize);
    let mut shown = 0;
    for spec in PROJECTS {
        let (_, held) = assemble(spec);
        for h in &held {
            for m in methods_of(&h.text, 1) {
                let Some(kind) = ["get", "is", "has", "set"].iter().position(|p| m.name.starts_with(p) && m.name[p.len()..].starts_with(|c: char| c.is_uppercase())) else { continue };
                let k = usize::from(kind == 3);
                let line_start = h.text[..m.start].rfind('\n').map_or(0, |p| p + 1);
                let outer: String = h.text[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let inner = format!("{outer}    ");
                let emptied = format!("{}{{\n{inner}\n{outer}}}{}", &h.text[..m.body.0], &h.text[m.body.1..]);
                let caret = m.body.0 + 2 + inner.len();
                methods[k] += 1;
                let Some(b) = flc_body::suggest(&emptied, caret) else { continue };
                proposed[k] += 1;
                let drawn: Vec<String> = b.text.split('\n').map(|s| normalize(s)).collect();
                let want: Vec<String> = m.statements.iter().map(|s| normalize(s)).collect();
                if drawn == want {
                    exact[k] += 1;
                } else if k == 1 && drawn.len() == want.len() + 1 && drawn[..want.len()] == want[..] {
                    fluent_ok += 1;
                } else if shown < 8 {
                    shown += 1;
                    eprintln!("[{}] {}: want {:?}, got {:?}", spec.name, m.name, want, drawn);
                }
            }
        }
    }
    let row = |name: &str, k: usize| format!("| {name} | {} | {:.1}% | {:.1}% | {:.1}% |\n", methods[k], pct(proposed[k], methods[k]), pct(exact[k], proposed[k]), pct(exact[k], methods[k]));
    let out = format!("## flc_body on held-out accessors\n\n| kind | methods | proposed | exact of proposed | exact of all |\n|---|---|---|---|---|\n{}{}\nSetters that also got `return this;` where the real body did not have it: {fluent_ok}.\n", row("getters (get / is / has)", 0), row("setters", 1));
    let path = write_result("m4_body_report.md", &out);
    eprintln!("{out}\nreport: {}", path.display());
}
