//! A ratatui application skeleton: terminal set-up and restore, the event loop, an `App` with its state,
//! and `draw` / key handling — for ratatui 0.28+ (`ratatui::init`) or the manual crossterm set-up before.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, check_fields, require_ident, Field};
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RatatuiApp;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppOptions {
    pub name: String,
    pub title: String,
    /// `single` | `header-body-footer`
    pub layout: String,
    /// Milliseconds between ticks; `0` waits for input only.
    pub tick_ms: u32,
    pub mouse: bool,
    /// Extra state fields of `App` (they need `Default`).
    pub state: Vec<Field>,
    /// The ratatui minor version (`29` for 0.29).
    pub ratatui_minor: u32,
}

impl Default for AppOptions {
    fn default() -> Self {
        AppOptions { name: "App".into(), title: "My app".into(), layout: "header-body-footer".into(), tick_ms: 250, mouse: false, state: vec![Field::new("counter", "u32")], ratatui_minor: 29 }
    }
}

impl Generator for RatatuiApp {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "ratatui-app",
            title: "Generate ratatui application",
            description: "main with terminal init/restore, an App with state, the event loop, draw and key handling.",
            icon: "Terminal",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["ratatui"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(AppOptions::default()).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: AppOptions = parse_options(options)?;
        require_ident(&o.name, "App name")?;
        check_fields(&o.state, "State field")?;
        let mut l = Lines::new();
        write_uses(&mut l, &o);
        l.blank();
        write_main(&mut l, &o);
        l.blank();
        write_app(&mut l, &o);
        let mut out = at_caret(l.finish()).extra("Cargo.toml", "toml", format!("ratatui = \"0.{}\"\n", o.ratatui_minor));
        if o.ratatui_minor < 28 {
            out = out.note("Before 0.28 there is no `ratatui::init()`: the terminal is set up by hand and restored in `restore_terminal`; a panic leaves the terminal raw unless you add a hook.");
        }
        Ok(out)
    }
}

fn modern(o: &AppOptions) -> bool {
    o.ratatui_minor >= 28
}

fn write_uses(l: &mut Lines, o: &AppOptions) {
    l.line("use std::{io, time::Duration};");
    l.blank();
    l.line("use ratatui::{");
    l.indent();
    let mut event = vec!["self", "Event", "KeyCode", "KeyEvent", "KeyEventKind"];
    if o.mouse {
        event.push("MouseEvent");
    }
    l.line(format!("crossterm::event::{{{}}},", event.join(", ")));
    if modern(o) {
        l.line("DefaultTerminal, Frame,");
    } else {
        l.line("backend::CrosstermBackend, Frame, Terminal,");
    }
    if o.layout == "header-body-footer" {
        l.line("layout::{Constraint, Layout},");
    }
    l.line("widgets::{Block, Paragraph},");
    l.dedent();
    l.line("};");
}

fn terminal_type(o: &AppOptions) -> &'static str {
    if modern(o) { "DefaultTerminal" } else { "Terminal<CrosstermBackend<io::Stdout>>" }
}

fn write_main(l: &mut Lines, o: &AppOptions) {
    let name = &o.name;
    l.block("fn main() -> io::Result<()>", |l| {
        if modern(o) {
            l.line("let mut terminal = ratatui::init();");
        } else {
            l.line("let mut terminal = setup_terminal()?;");
        }
        if o.mouse {
            l.line("ratatui::crossterm::execute!(io::stdout(), ratatui::crossterm::event::EnableMouseCapture)?;");
        }
        l.line(format!("let result = {name}::default().run(&mut terminal);"));
        if o.mouse {
            l.line("ratatui::crossterm::execute!(io::stdout(), ratatui::crossterm::event::DisableMouseCapture)?;");
        }
        if modern(o) {
            l.line("ratatui::restore();");
        } else {
            l.line("restore_terminal(&mut terminal)?;");
        }
        l.line("result");
    });
    if !modern(o) {
        l.blank();
        l.block(format!("fn setup_terminal() -> io::Result<{}>", terminal_type(o)), |l| {
            l.line("ratatui::crossterm::terminal::enable_raw_mode()?;");
            l.line("ratatui::crossterm::execute!(io::stdout(), ratatui::crossterm::terminal::EnterAlternateScreen)?;");
            l.line("Terminal::new(CrosstermBackend::new(io::stdout()))");
        });
        l.blank();
        l.block(format!("fn restore_terminal(terminal: &mut {}) -> io::Result<()>", terminal_type(o)), |l| {
            l.line("ratatui::crossterm::terminal::disable_raw_mode()?;");
            l.line("ratatui::crossterm::execute!(terminal.backend_mut(), ratatui::crossterm::terminal::LeaveAlternateScreen)?;");
            l.line("terminal.show_cursor()");
        });
    }
}

fn write_app(l: &mut Lines, o: &AppOptions) {
    let name = &o.name;
    l.line("#[derive(Debug, Default)]");
    l.block(format!("pub struct {name}"), |l| {
        l.line("running: bool,");
        for f in &o.state {
            l.line(format!("{}: {},", f.name, f.ty));
        }
    });
    l.blank();
    l.block(format!("impl {name}"), |l| {
        l.block(format!("pub fn run(&mut self, terminal: &mut {}) -> io::Result<()>", terminal_type(o)), |l| {
            l.line("self.running = true;");
            l.block("while self.running", |l| {
                l.line("terminal.draw(|frame| self.draw(frame))?;");
                l.line("self.handle_events()?;");
            });
            l.line("Ok(())");
        });
        l.blank();
        l.block("fn draw(&self, frame: &mut Frame)", |l| {
            let area = if o.ratatui_minor >= 29 { "frame.area()" } else { "frame.size()" };
            if o.layout == "header-body-footer" {
                l.line(format!("let [header, body, footer] = Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas({area});"));
                l.line(format!("frame.render_widget(Paragraph::new({:?}), header);", o.title));
                l.line("frame.render_widget(Block::bordered(), body);");
                l.line("frame.render_widget(Paragraph::new(\"q: quit\"), footer);");
            } else {
                l.line(format!("frame.render_widget(Paragraph::new(\"Hello\").block(Block::bordered().title({:?})), {area});", o.title));
            }
        });
        l.blank();
        l.block("fn handle_events(&mut self) -> io::Result<()>", |l| {
            let wait = if o.tick_ms > 0 { format!("event::poll(Duration::from_millis({}))?", o.tick_ms) } else { "true".to_string() };
            l.block(format!("if {wait}"), |l| {
                l.block("match event::read()?", |l| {
                    l.line("Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key(key),");
                    if o.mouse {
                        l.line("Event::Mouse(mouse) => self.on_mouse(mouse),");
                    }
                    l.line("_ => {}");
                });
            });
            if o.tick_ms > 0 {
                l.block_after("else", |l| {
                    l.line("self.on_tick();");
                });
            }
            l.line("Ok(())");
        });
        l.blank();
        l.block("fn on_key(&mut self, key: KeyEvent)", |l| {
            l.block("match key.code", |l| {
                l.line("KeyCode::Char('q') | KeyCode::Esc => self.running = false,");
                l.line("_ => {}");
            });
        });
        if o.mouse {
            l.blank();
            l.block("fn on_mouse(&mut self, _mouse: MouseEvent)", |_| {});
        }
        if o.tick_ms > 0 {
            l.blank();
            l.block("fn on_tick(&mut self)", |_| {});
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn text(options: serde_json::Value) -> String {
        RatatuiApp.generate(&options, &sample_context()).unwrap().members.unwrap().text
    }

    #[test]
    fn the_modern_skeleton_uses_init_restore_and_area() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("let mut terminal = ratatui::init();"), "{t}");
        assert!(t.contains("ratatui::restore();"), "{t}");
        assert!(t.contains("pub fn run(&mut self, terminal: &mut DefaultTerminal)"), "{t}");
        assert!(t.contains(".areas(frame.area());"), "{t}");
        assert!(t.contains("counter: u32,"), "{t}");
        assert!(t.contains("} else {\n            self.on_tick();"), "{t}");
    }

    #[test]
    fn older_versions_set_the_terminal_up_by_hand() {
        let t = text(serde_json::json!({ "ratatui_minor": 26 }));
        assert!(t.contains("fn setup_terminal()"), "{t}");
        assert!(t.contains("frame.size()"), "{t}");
        assert!(!t.contains("ratatui::init()"), "{t}");
    }

    #[test]
    fn no_tick_blocks_on_input_and_mouse_adds_capture() {
        let t = text(serde_json::json!({ "tick_ms": 0, "mouse": true, "layout": "single" }));
        assert!(t.contains("if true {"), "{t}");
        assert!(!t.contains("on_tick"), "{t}");
        assert!(t.contains("EnableMouseCapture"), "{t}");
        assert!(t.contains("Event::Mouse(mouse) => self.on_mouse(mouse),"), "{t}");
        assert!(t.contains("Paragraph::new(\"Hello\")"), "{t}");
    }

    #[test]
    fn bad_names_are_refused() {
        assert!(RatatuiApp.generate(&serde_json::json!({ "name": "fn" }), &sample_context()).is_err());
        assert!(RatatuiApp.generate(&serde_json::json!({ "state": [{ "name": "x", "ty": "" }] }), &sample_context()).is_err());
    }
}
