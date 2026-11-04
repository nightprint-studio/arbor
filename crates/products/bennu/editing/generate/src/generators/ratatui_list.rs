//! A stateful list or table screen for ratatui: the row type, the selection state with next / previous /
//! first / last (wrapping), the highlighted render and the navigation keys.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::Lines;
use crate::context::GenContext;
use crate::generators::rust_support::{at_caret, check_fields, require_ident, write_struct, Field};
use crate::names::pascal;
use crate::output::{parse_options, Output};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct RatatuiList;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ListOptions {
    /// The screen's name; the row type is `<Name>Row`.
    pub name: String,
    /// `list` | `table`
    pub kind: String,
    /// The row's fields; a table has one column per field, a list shows the first.
    pub columns: Vec<Field>,
    pub title: String,
    /// Keys beyond the arrows: `j`/`k`, `g`/`G`.
    pub vim_keys: bool,
    /// `Enter` calls `on_activate`.
    pub activate: bool,
    pub ratatui_minor: u32,
}

impl Default for ListOptions {
    fn default() -> Self {
        ListOptions {
            name: "Orders".into(),
            kind: "table".into(),
            columns: vec![Field::new("id", "u64"), Field::new("customer", "String"), Field::new("total", "f64")],
            title: "Orders".into(),
            vim_keys: true,
            activate: true,
            ratatui_minor: 29,
        }
    }
}

impl Generator for RatatuiList {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "ratatui-list",
            title: "Generate ratatui list or table screen",
            description: "A selectable List or Table with its row type, wrapping navigation, highlight and key handling.",
            icon: "Table2",
            group: "Rust",
            language: "rust",
            targets: &[Target::Rust],
            related: &["ratatui"],
        }
    }

    fn defaults(&self, _ctx: &GenContext) -> Value {
        serde_json::to_value(ListOptions::default()).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, _ctx: &GenContext) -> Result<Output, String> {
        let o: ListOptions = parse_options(options)?;
        require_ident(&o.name, "Name")?;
        check_fields(&o.columns, "Column")?;
        if o.columns.is_empty() {
            return Err("A row needs at least one field".into());
        }
        if o.kind != "list" && o.kind != "table" {
            return Err(format!("Unknown kind `{}`", o.kind));
        }
        let mut l = Lines::new();
        write(&mut l, &o);
        let uses = if o.kind == "table" {
            "use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::{Constraint, Rect}, style::{Modifier, Style}, widgets::{Block, Row, Table, TableState}, Frame};"
        } else {
            "use ratatui::{crossterm::event::{KeyCode, KeyEvent}, layout::Rect, style::{Modifier, Style}, widgets::{Block, List, ListItem, ListState}, Frame};"
        };
        Ok(at_caret(l.finish()).note(format!("Needs `{uses}`.")))
    }
}

fn write(l: &mut Lines, o: &ListOptions) {
    let row = format!("{}Row", pascal(&o.name));
    let screen = format!("{}Screen", pascal(&o.name));
    let table = o.kind == "table";
    let state_ty = if table { "TableState" } else { "ListState" };
    write_struct(l, &row, &o.columns, &["Debug", "Clone"], &[]);
    l.blank();
    l.line("#[derive(Debug, Default)]");
    l.block(format!("pub struct {screen}"), |l| {
        l.line(format!("pub rows: Vec<{row}>,"));
        l.line(format!("state: {state_ty},"));
    });
    l.blank();
    l.block(format!("impl {screen}"), |l| {
        l.block(format!("pub fn new(rows: Vec<{row}>) -> Self"), |l| {
            l.line(format!("let mut state = {state_ty}::default();"));
            l.block("if !rows.is_empty()", |l| {
                l.line("state.select(Some(0));");
            });
            l.line("Self { rows, state }");
        });
        l.blank();
        l.block(format!("pub fn selected(&self) -> Option<&{row}>"), |l| {
            l.line("self.state.selected().and_then(|i| self.rows.get(i))");
        });
        l.blank();
        l.block("pub fn next(&mut self)", |l| {
            l.block("if self.rows.is_empty()", |l| {
                l.line("return;");
            });
            l.line("let i = self.state.selected().map_or(0, |i| (i + 1) % self.rows.len());");
            l.line("self.state.select(Some(i));");
        });
        l.blank();
        l.block("pub fn previous(&mut self)", |l| {
            l.block("if self.rows.is_empty()", |l| {
                l.line("return;");
            });
            l.line("let i = self.state.selected().map_or(0, |i| (i + self.rows.len() - 1) % self.rows.len());");
            l.line("self.state.select(Some(i));");
        });
        l.blank();
        l.block("pub fn first(&mut self)", |l| {
            l.block("if !self.rows.is_empty()", |l| {
                l.line("self.state.select(Some(0));");
            });
        });
        l.blank();
        l.block("pub fn last(&mut self)", |l| {
            l.block("if !self.rows.is_empty()", |l| {
                l.line("self.state.select(Some(self.rows.len() - 1));");
            });
        });
        l.blank();
        write_key(l, o);
        l.blank();
        write_render(l, o);
    });
}

fn write_key(l: &mut Lines, o: &ListOptions) {
    l.block("pub fn handle_key(&mut self, key: KeyEvent)", |l| {
        l.block("match key.code", |l| {
            let down = if o.vim_keys { "KeyCode::Down | KeyCode::Char('j')" } else { "KeyCode::Down" };
            let up = if o.vim_keys { "KeyCode::Up | KeyCode::Char('k')" } else { "KeyCode::Up" };
            l.line(format!("{down} => self.next(),"));
            l.line(format!("{up} => self.previous(),"));
            if o.vim_keys {
                l.line("KeyCode::Home | KeyCode::Char('g') => self.first(),");
                l.line("KeyCode::End | KeyCode::Char('G') => self.last(),");
            } else {
                l.line("KeyCode::Home => self.first(),");
                l.line("KeyCode::End => self.last(),");
            }
            if o.activate {
                l.line("KeyCode::Enter => self.on_activate(),");
            }
            l.line("_ => {}");
        });
    });
    if o.activate {
        l.blank();
        l.line("/// Called for `Enter` on the selected row.");
        l.block("fn on_activate(&mut self)", |l| {
            l.line("// TODO: open the selected row");
        });
    }
}

fn write_render(l: &mut Lines, o: &ListOptions) {
    let highlight = "Style::default().add_modifier(Modifier::REVERSED)";
    l.block("pub fn render(&mut self, frame: &mut Frame, area: Rect)", |l| {
        l.line(format!("let block = Block::bordered().title({:?});", o.title));
        if o.kind == "table" {
            let cells: Vec<String> = o.columns.iter().map(|c| format!("row.{}.to_string()", c.name)).collect();
            let headers: Vec<String> = o.columns.iter().map(|c| format!("{:?}", pascal(&c.name))).collect();
            l.line("let widths = [");
            l.indent();
            for _ in &o.columns {
                l.line(format!("Constraint::Ratio(1, {}),", o.columns.len()));
            }
            l.dedent();
            l.line("];");
            l.line("let rows = self.rows.iter().map(|row| Row::new([");
            l.indent();
            for c in &cells {
                l.line(format!("{c},"));
            }
            l.dedent();
            l.line("]));");
            l.line("let table = Table::new(rows, widths)");
            l.indent().indent();
            l.line(format!(".header(Row::new([{}]).style(Style::default().add_modifier(Modifier::BOLD)))", headers.join(", ")));
            l.line(".block(block)");
            let method = if o.ratatui_minor >= 29 { "row_highlight_style" } else { "highlight_style" };
            l.line(format!(".{method}({highlight})"));
            l.line(".highlight_symbol(\"> \");");
            l.dedent().dedent();
            l.line("frame.render_stateful_widget(table, area, &mut self.state);");
        } else {
            let label = &o.columns[0].name;
            l.line(format!("let items = self.rows.iter().map(|row| ListItem::new(row.{label}.to_string()));"));
            l.line("let list = List::new(items)");
            l.indent().indent();
            l.line(".block(block)");
            l.line(format!(".highlight_style({highlight})"));
            l.line(".highlight_symbol(\"> \");");
            l.dedent().dedent();
            l.line("frame.render_stateful_widget(list, area, &mut self.state);");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::sample_context;

    fn text(options: serde_json::Value) -> String {
        RatatuiList.generate(&options, &sample_context()).unwrap().members.unwrap().text
    }

    #[test]
    fn a_table_has_a_column_per_field_and_a_highlighted_stateful_render() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("pub struct OrdersRow {\n    pub id: u64,"), "{t}");
        assert!(t.contains("state: TableState,"), "{t}");
        assert!(t.contains("row.customer.to_string(),"), "{t}");
        assert!(t.contains("Constraint::Ratio(1, 3),"), "{t}");
        assert!(t.contains(".header(Row::new([\"Id\", \"Customer\", \"Total\"])"), "{t}");
        assert!(t.contains(".row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))"), "{t}");
        assert!(t.contains("frame.render_stateful_widget(table, area, &mut self.state);"), "{t}");
    }

    #[test]
    fn older_ratatui_names_the_table_highlight_differently() {
        assert!(text(serde_json::json!({ "ratatui_minor": 28 })).contains(".highlight_style("));
    }

    #[test]
    fn a_list_shows_the_first_field() {
        let t = text(serde_json::json!({ "kind": "list" }));
        assert!(t.contains("state: ListState,"), "{t}");
        assert!(t.contains("ListItem::new(row.id.to_string())"), "{t}");
        assert!(!t.contains("Table::new"), "{t}");
    }

    #[test]
    fn navigation_wraps_and_the_keys_are_optional() {
        let t = text(serde_json::json!({}));
        assert!(t.contains("(i + 1) % self.rows.len()"), "{t}");
        assert!(t.contains("(i + self.rows.len() - 1) % self.rows.len()"), "{t}");
        assert!(t.contains("KeyCode::Down | KeyCode::Char('j') => self.next(),"), "{t}");
        assert!(t.contains("KeyCode::Enter => self.on_activate(),"), "{t}");
        let plain = text(serde_json::json!({ "vim_keys": false, "activate": false }));
        assert!(plain.contains("KeyCode::Down => self.next(),"), "{plain}");
        assert!(!plain.contains("on_activate"), "{plain}");
    }

    #[test]
    fn an_empty_row_or_unknown_kind_is_refused() {
        let ctx = sample_context();
        assert!(RatatuiList.generate(&serde_json::json!({ "columns": [] }), &ctx).is_err());
        assert!(RatatuiList.generate(&serde_json::json!({ "kind": "grid" }), &ctx).is_err());
    }
}
