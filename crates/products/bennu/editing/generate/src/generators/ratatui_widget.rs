//! A ratatui widget skeleton: a `Widget` (also implemented for `&T` so it can be drawn without a clone), a
//! `StatefulWidget` with its state type, or a component with key handling and a `render` of its own.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, check_fields, require_ident, write_struct, Field};
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RatatuiWidget;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WidgetOptions {
    pub name: String,
    /// `widget` | `stateful` | `component`
    pub kind: String,
    pub fields: Vec<Field>,
    pub bordered: bool,
    /// `widget`: also `impl Widget for &Name`.
    pub by_ref: bool,
    /// `component`: a `handle_key` method returning whether the key was used.
    pub keys: bool,
}

impl Default for WidgetOptions {
    fn default() -> Self {
        WidgetOptions { name: "Meter".into(), kind: "widget".into(), fields: vec![Field::new("label", "String"), Field::new("ratio", "f64")], bordered: true, by_ref: true, keys: true }
    }
}

impl Generator for RatatuiWidget {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "ratatui-widget",
            title: "Generate ratatui widget or component",
            description: "A Widget, a StatefulWidget with its state, or a component with key handling and render.",
            icon: "LayoutPanelLeft",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["ratatui"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(WidgetOptions::default()).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: WidgetOptions = parse_options(options)?;
        require_ident(&o.name, "Name")?;
        check_fields(&o.fields, "Field")?;
        let mut l = Lines::new();
        match o.kind.as_str() {
            "widget" => widget(&mut l, &o),
            "stateful" => stateful(&mut l, &o),
            "component" => component(&mut l, &o),
            other => return Err(format!("Unknown kind `{other}`")),
        }
        let uses = match o.kind.as_str() {
            "widget" => "use ratatui::{buffer::Buffer, layout::Rect, widgets::{Block, Widget}};",
            "stateful" => "use ratatui::{buffer::Buffer, layout::Rect, widgets::{Block, StatefulWidget, Widget}};",
            _ => "use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::Rect, widgets::Block, Frame};",
        };
        Ok(at_caret(l.finish()).note(format!("Needs `{uses}`.")))
    }
}

fn frame_lines(l: &mut Lines, o: &WidgetOptions, buf: bool) {
    if o.bordered {
        l.line(format!("let block = Block::bordered().title({:?});", o.name));
        l.line("let inner = block.inner(area);");
        if buf {
            l.line("block.render(area, buf);");
        } else {
            l.line("frame.render_widget(block, area);");
        }
    } else {
        l.line("let inner = area;");
    }
    l.line("// TODO: draw into `inner`");
    l.line("let _ = inner;");
}

fn widget(l: &mut Lines, o: &WidgetOptions) {
    let name = &o.name;
    write_struct(l, name, &o.fields, &["Debug", "Clone", "Default"], &[]);
    l.blank();
    l.block(format!("impl Widget for {name}"), |l| {
        l.block("fn render(self, area: Rect, buf: &mut Buffer)", |l| {
            if o.by_ref {
                l.line("Widget::render(&self, area, buf);");
            } else {
                frame_lines(l, o, true);
            }
        });
    });
    if o.by_ref {
        l.blank();
        l.block(format!("impl Widget for &{name}"), |l| {
            l.block("fn render(self, area: Rect, buf: &mut Buffer)", |l| {
                frame_lines(l, o, true);
            });
        });
    }
}

fn stateful(l: &mut Lines, o: &WidgetOptions) {
    let name = &o.name;
    let state = format!("{name}State");
    write_struct(l, name, &o.fields, &["Debug", "Clone", "Default"], &[]);
    l.blank();
    write_struct(l, &state, &[Field::new("selected", "usize")], &["Debug", "Clone", "Default"], &[]);
    l.blank();
    l.block(format!("impl StatefulWidget for {name}"), |l| {
        l.line(format!("type State = {state};"));
        l.blank();
        l.block("fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State)", |l| {
            frame_lines(l, o, true);
            l.line("let _ = state;");
        });
    });
}

fn component(l: &mut Lines, o: &WidgetOptions) {
    let name = &o.name;
    write_struct(l, name, &o.fields, &["Debug", "Default"], &[]);
    l.blank();
    l.block(format!("impl {name}"), |l| {
        if o.keys {
            l.line("/// Whether the key was used.");
            l.block("pub fn handle_key(&mut self, key: KeyEvent) -> bool", |l| {
                l.block("match key.code", |l| {
                    l.line("KeyCode::Enter => true,");
                    l.line("_ => false,");
                });
            });
            l.blank();
        }
        l.block("pub fn render(&self, frame: &mut Frame, area: Rect)", |l| {
            frame_lines(l, o, false);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn text(options: serde_json::Value) -> String {
        RatatuiWidget.generate(&options, &sample_context()).unwrap().members.unwrap().text
    }

    #[test]
    fn a_widget_is_implemented_for_the_value_and_the_reference() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("pub struct Meter {\n    pub label: String,\n    pub ratio: f64,\n}"), "{t}");
        assert!(t.contains("impl Widget for Meter {"), "{t}");
        assert!(t.contains("impl Widget for &Meter {"), "{t}");
        assert!(t.contains("Widget::render(&self, area, buf);"), "{t}");
        assert!(t.contains("let block = Block::bordered().title(\"Meter\");"), "{t}");
    }

    #[test]
    fn without_by_ref_the_value_renders_itself() {
        let t = text(serde_json::json!({ "by_ref": false, "bordered": false }));
        assert!(!t.contains("for &Meter"), "{t}");
        assert!(t.contains("let inner = area;"), "{t}");
    }

    #[test]
    fn a_stateful_widget_declares_its_state() {
        let t = text(serde_json::json!({ "kind": "stateful" }));
        assert!(t.contains("pub struct MeterState {\n    pub selected: usize,\n}"), "{t}");
        assert!(t.contains("impl StatefulWidget for Meter {\n    type State = MeterState;"), "{t}");
        assert!(t.contains("state: &mut Self::State"), "{t}");
    }

    #[test]
    fn a_component_handles_keys_and_renders_into_a_frame() {
        let t = text(serde_json::json!({ "kind": "component" }));
        assert!(t.contains("pub fn handle_key(&mut self, key: KeyEvent) -> bool {"), "{t}");
        assert!(t.contains("pub fn render(&self, frame: &mut Frame, area: Rect) {"), "{t}");
        assert!(t.contains("frame.render_widget(block, area);"), "{t}");
        assert!(!text(serde_json::json!({ "kind": "component", "keys": false })).contains("handle_key"));
    }

    #[test]
    fn a_bad_kind_or_name_is_refused() {
        assert!(RatatuiWidget.generate(&serde_json::json!({ "kind": "x" }), &sample_context()).is_err());
        assert!(RatatuiWidget.generate(&serde_json::json!({ "name": "1x" }), &sample_context()).is_err());
    }
}
