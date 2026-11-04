//! Overlays for a ratatui app: a centred message popup, a yes/no confirmation that reports the answer, and
//! a single-line text input with a cursor — each with the `centered_rect` helper they draw into.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, require_ident};
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RatatuiDialog;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DialogOptions {
    pub name: String,
    /// `popup` | `confirm` | `input`
    pub kind: String,
    pub title: String,
    pub width_percent: u16,
    /// Also write `centered_rect` (leave it out when the file already has one).
    pub helper: bool,
    pub ratatui_minor: u32,
}

impl Default for DialogOptions {
    fn default() -> Self {
        DialogOptions { name: "Dialog".into(), kind: "confirm".into(), title: "Confirm".into(), width_percent: 50, helper: true, ratatui_minor: 29 }
    }
}

impl Generator for RatatuiDialog {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "ratatui-dialog",
            title: "Generate ratatui popup, confirmation or input",
            description: "A centred popup, a yes/no confirmation or a text input with a cursor, drawn over the screen with Clear.",
            icon: "MessageSquare",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["ratatui"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(DialogOptions::default()).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: DialogOptions = parse_options(options)?;
        require_ident(&o.name, "Name")?;
        if o.width_percent == 0 || o.width_percent > 100 {
            return Err("The width is a percentage between 1 and 100".into());
        }
        let mut l = Lines::new();
        if o.helper {
            helper(&mut l);
            l.blank();
        }
        match o.kind.as_str() {
            "popup" => popup(&mut l, &o),
            "confirm" => confirm(&mut l, &o),
            "input" => input(&mut l, &o),
            other => return Err(format!("Unknown kind `{other}`")),
        }
        let uses = match o.kind.as_str() {
            "popup" => "use ratatui::{buffer::Buffer, layout::{Constraint, Flex, Layout, Rect}, widgets::{Block, Clear, Paragraph, Widget, Wrap}};",
            "confirm" => "use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Flex, Layout, Rect}, style::{Modifier, Style}, text::{Line, Span}, widgets::{Block, Clear, Paragraph}, Frame};",
            _ => "use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Flex, Layout, Rect}, widgets::{Block, Clear, Paragraph}, Frame};",
        };
        Ok(at_caret(l.finish()).note(format!("Needs `{uses}`.")))
    }
}

fn helper(l: &mut Lines) {
    l.line("/// A rectangle of the given size centred in `area`.");
    l.block("fn centered_rect(area: Rect, width: Constraint, height: Constraint) -> Rect", |l| {
        l.line("let [area] = Layout::horizontal([width]).flex(Flex::Center).areas(area);");
        l.line("let [area] = Layout::vertical([height]).flex(Flex::Center).areas(area);");
        l.line("area");
    });
}

fn popup(l: &mut Lines, o: &DialogOptions) {
    let name = &o.name;
    l.line("#[derive(Debug, Clone, Default)]");
    l.block(format!("pub struct {name}"), |l| {
        l.line("pub title: String,");
        l.line("pub body: String,");
    });
    l.blank();
    l.block(format!("impl Widget for &{name}"), |l| {
        l.block("fn render(self, area: Rect, buf: &mut Buffer)", |l| {
            l.line(format!("let area = centered_rect(area, Constraint::Percentage({}), Constraint::Length(7));", o.width_percent));
            l.line("Clear.render(area, buf);");
            l.line("Paragraph::new(self.body.as_str())");
            l.indent().indent();
            l.line(".wrap(Wrap { trim: true })");
            l.line(".block(Block::bordered().title(self.title.as_str()))");
            l.line(".render(area, buf);");
            l.dedent().dedent();
        });
    });
}
fn confirm(l: &mut Lines, o: &DialogOptions) {
    let name = &o.name;
    let choice = format!("{name}Choice");
    l.line("#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]");
    l.block(format!("pub enum {choice}"), |l| {
        l.line("Yes,");
        l.line("#[default]");
        l.line("No,");
    });
    l.blank();
    l.line("#[derive(Debug, Clone, Default)]");
    l.block(format!("pub struct {name}"), |l| {
        l.line("pub question: String,");
        l.line(format!("pub choice: {choice},"));
    });
    l.blank();
    l.block(format!("impl {name}"), |l| {
        l.line("/// `Some(answer)` once the user has decided, `None` while the dialog stays open.");
        l.block("pub fn handle_key(&mut self, key: KeyEvent) -> Option<bool>", |l| {
            l.block("match key.code", |l| {
                l.line("KeyCode::Char('y') | KeyCode::Char('Y') => Some(true),");
                l.line("KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => Some(false),");
                l.block("KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') =>", |l| {
                    l.line(format!("self.choice = match self.choice {{ {choice}::Yes => {choice}::No, {choice}::No => {choice}::Yes }};"));
                    l.line("None");
                });
                l.line(format!("KeyCode::Enter => Some(self.choice == {choice}::Yes),"));
                l.line("_ => None,");
            });
        });
        l.blank();
        l.block("pub fn render(&self, frame: &mut Frame, area: Rect)", |l| {
            l.line(format!("let area = centered_rect(area, Constraint::Percentage({}), Constraint::Length(5));", o.width_percent));
            l.line("frame.render_widget(Clear, area);");
            l.line(format!("let block = Block::bordered().title({:?});", o.title));
            l.line("let inner = block.inner(area);");
            l.line("frame.render_widget(block, area);");
            l.line("let [question, buttons] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);");
            l.line("frame.render_widget(Paragraph::new(self.question.as_str()), question);");
            l.line("let style = |selected: bool| if selected { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };");
            l.line("let line = Line::from(vec![");
            l.indent();
            l.line(format!("Span::styled(\" Yes \", style(self.choice == {choice}::Yes)),"));
            l.line("Span::raw(\"  \"),");
            l.line(format!("Span::styled(\" No \", style(self.choice == {choice}::No)),"));
            l.dedent();
            l.line("]);");
            l.line("frame.render_widget(Paragraph::new(line).centered(), buttons);");
        });
    });
}

fn input(l: &mut Lines, o: &DialogOptions) {
    let name = &o.name;
    l.line("#[derive(Debug, Clone, Copy, PartialEq, Eq)]");
    l.block("pub enum InputEvent", |l| {
        l.line("Editing,");
        l.line("Submit,");
        l.line("Cancel,");
    });
    l.blank();
    l.line("#[derive(Debug, Clone, Default)]");
    l.block(format!("pub struct {name}"), |l| {
        l.line("pub value: String,");
        l.line("/// In characters, not bytes.");
        l.line("cursor: usize,");
    });
    l.blank();
    l.block(format!("impl {name}"), |l| {
        l.block("pub fn handle_key(&mut self, key: KeyEvent) -> InputEvent", |l| {
            l.block("match key.code", |l| {
                l.line("KeyCode::Enter => return InputEvent::Submit,");
                l.line("KeyCode::Esc => return InputEvent::Cancel,");
                l.block("KeyCode::Char(c) =>", |l| {
                    l.line("let at = self.byte_index();");
                    l.line("self.value.insert(at, c);");
                    l.line("self.cursor += 1;");
                });
                l.block("KeyCode::Backspace if self.cursor > 0 =>", |l| {
                    l.line("self.cursor -= 1;");
                    l.line("let at = self.byte_index();");
                    l.line("self.value.remove(at);");
                });
                l.block("KeyCode::Delete if self.cursor < self.value.chars().count() =>", |l| {
                    l.line("let at = self.byte_index();");
                    l.line("self.value.remove(at);");
                });
                l.line("KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),");
                l.line("KeyCode::Right => self.cursor = (self.cursor + 1).min(self.value.chars().count()),");
                l.line("KeyCode::Home => self.cursor = 0,");
                l.line("KeyCode::End => self.cursor = self.value.chars().count(),");
                l.line("_ => {}");
            });
            l.line("InputEvent::Editing");
        });
        l.blank();
        l.block("fn byte_index(&self) -> usize", |l| {
            l.line("self.value.char_indices().nth(self.cursor).map_or(self.value.len(), |(i, _)| i)");
        });
        l.blank();
        l.block("pub fn render(&self, frame: &mut Frame, area: Rect)", |l| {
            l.line(format!("let area = centered_rect(area, Constraint::Percentage({}), Constraint::Length(3));", o.width_percent));
            l.line("frame.render_widget(Clear, area);");
            l.line(format!("frame.render_widget(Paragraph::new(self.value.as_str()).block(Block::bordered().title({:?})), area);", o.title));
            let cursor = if o.ratatui_minor >= 29 { "set_cursor_position((area.x + 1 + self.cursor as u16, area.y + 1))" } else { "set_cursor(area.x + 1 + self.cursor as u16, area.y + 1)" };
            l.line(format!("frame.{cursor};"));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn text(options: serde_json::Value) -> String {
        RatatuiDialog.generate(&options, &sample_context()).unwrap().members.unwrap().text
    }

    #[test]
    fn the_centering_helper_is_written_once_and_can_be_left_out() {
        let t = text(serde_json::json!({}));
        assert!(t.starts_with("/// A rectangle of the given size centred in `area`.\nfn centered_rect("), "{t}");
        assert!(t.contains("Flex::Center"), "{t}");
        assert!(!text(serde_json::json!({ "helper": false })).contains("fn centered_rect"));
    }

    #[test]
    fn a_confirmation_reports_the_answer_and_toggles_the_choice() {
        let t = text(serde_json::json!({ "name": "Quit" }));
        assert!(t.contains("pub enum QuitChoice {\n    Yes,\n    #[default]\n    No,\n}"), "{t}");
        assert!(t.contains("pub fn handle_key(&mut self, key: KeyEvent) -> Option<bool> {"), "{t}");
        assert!(t.contains("KeyCode::Enter => Some(self.choice == QuitChoice::Yes),"), "{t}");
        assert!(t.contains("self.choice = match self.choice { QuitChoice::Yes => QuitChoice::No, QuitChoice::No => QuitChoice::Yes };"), "{t}");
        assert!(t.contains("Constraint::Percentage(50), Constraint::Length(5)"), "{t}");
    }

    #[test]
    fn a_popup_clears_what_is_under_it() {
        let t = text(serde_json::json!({ "kind": "popup" }));
        assert!(t.contains("impl Widget for &Dialog {"), "{t}");
        assert!(t.contains("Clear.render(area, buf);"), "{t}");
        assert!(t.contains(".wrap(Wrap { trim: true })"), "{t}");
    }

    #[test]
    fn an_input_edits_by_characters_and_places_the_cursor() {
        let t = text(serde_json::json!({ "kind": "input", "title": "Name" }));
        assert!(t.contains("pub enum InputEvent {\n    Editing,"), "{t}");
        assert!(t.contains("KeyCode::Backspace if self.cursor > 0 => {"), "{t}");
        assert!(t.contains("self.value.char_indices().nth(self.cursor)"), "{t}");
        assert!(t.contains("frame.set_cursor_position((area.x + 1 + self.cursor as u16, area.y + 1));"), "{t}");
        assert!(text(serde_json::json!({ "kind": "input", "ratatui_minor": 27 })).contains("frame.set_cursor(area.x + 1"));
    }

    #[test]
    fn a_bad_kind_name_or_width_is_refused() {
        let ctx = sample_context();
        assert!(RatatuiDialog.generate(&serde_json::json!({ "kind": "toast" }), &ctx).is_err());
        assert!(RatatuiDialog.generate(&serde_json::json!({ "name": "let" }), &ctx).is_err());
        assert!(RatatuiDialog.generate(&serde_json::json!({ "width_percent": 0 }), &ctx).is_err());
    }
}
