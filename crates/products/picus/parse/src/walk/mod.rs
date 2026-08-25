//! CST → [`ParsedFile`].
//!
//! One traversal per top-level statement collects everything at once: the
//! objects, the DML shapes, the foreign constructs and the errors. The walk
//! deliberately descends into procedural bodies — in a real upgrade script the
//! INSERT that matters is three blocks deep inside `DECLARE … BEGIN … END`, and
//! a walker that stopped at the block would report an empty file.

mod dml;
mod names;
mod projection;
mod select;

use picus_types::prelude::DialectScope;
use tree_sitter::Node;

use crate::dialect::{self, ForeignConstruct};
use crate::dml::DmlShape;
use crate::error::{ParseError, ParseErrorKind, ERROR_TEXT_LIMIT};
use crate::object::{ObjectKind, ObjectRef};
use crate::statement::{ParsedFile, Statement, StatementKind};
use names::{field_ref, leading_keywords, object_kind_from_keywords, object_ref, range_of, text_of};

pub(crate) use projection::projection_of;

pub(crate) fn walk_file(root: Node, source: &str, scope: DialectScope) -> ParsedFile {
    walk_at_depth(root, source, scope, 0)
}

/// The walk, told how many `$$ … $$` bodies deep it already is.
///
/// The depth exists only to bound the re-parse of dollar-quoted bodies (see
/// [`Collector::descend_into_body`]); at depth 0 — every call from outside this
/// module — it changes nothing.
fn walk_at_depth(root: Node, source: &str, scope: DialectScope, depth: u8) -> ParsedFile {
    let mut statements = Vec::new();
    let mut errors = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "statement" | "slash_terminator" | "ERROR" => {
                let statement = statement_of(child, source, scope, depth, &mut errors);
                statements.push(statement);
            }
            // Comments and whitespace: they belong to the gaps.
            _ => {}
        }
    }

    ParsedFile {
        scope,
        source_len: source.len(),
        statements,
        errors,
        line_starts: ParsedFile::index_lines(source),
    }
}

fn statement_of(
    node: Node,
    source: &str,
    scope: DialectScope,
    depth: u8,
    file_errors: &mut Vec<ParseError>,
) -> Statement {
    // The body is the first named child; the `;` is anonymous and the Oracle `/`
    // follows it. A bare `/` at top level is its own statement.
    let body = if node.kind() == "statement" { node.named_child(0) } else { Some(node) };
    let node_kind = body.map(|b| b.kind().to_string()).unwrap_or_else(|| node.kind().to_string());

    // Only a top-level SELECT carries a projection shape; `shape_of` returns `None`
    // for everything else.
    let select = body.and_then(|b| select::shape_of(b, source));

    let mut collector = Collector::new(source, scope, depth);
    collector.visit(node, None);

    let before = file_errors.len();
    file_errors.extend(collector.errors.iter().cloned());
    let has_error = file_errors.len() > before || node.kind() == "ERROR";

    Statement {
        kind: kind_of(&node_kind),
        range: range_of(node),
        node_kind,
        replaces: body.is_some_and(|b| declares_replacement(b, source)),
        defines: collector.defines,
        references: collector.references,
        dml: collector.dml,
        foreign: collector.foreign,
        select,
        has_error,
    }
}

/// Is this dollar-quoted literal in a position where SQL is *expected*?
///
/// The whole of the guard, and it is a question about the grammar rather than
/// about the text. PostgreSQL uses `$$ … $$` for exactly two things: the body of
/// `DO`, and the body of a routine. Everywhere else it is an ordinary string
/// literal, and `INSERT INTO note (testo) VALUES ($$select name from list$$)`
/// holds a sentence — reading that as SQL would invent a reference to a table
/// nobody named, which is worse than missing one.
///
/// Deciding by parent node rather than by inspecting the contents is what makes
/// that impossible rather than unlikely: no sentence, however SQL-shaped, sits
/// under `do_statement` or `routine_body`.
fn is_routine_body(parent: Option<Node>) -> bool {
    matches!(parent.map(|p| p.kind()), Some("do_statement" | "routine_body"))
}

/// How deep a `$$ … $$` body may be re-parsed. One level: a function body holding
/// another dollar-quoted body is legal and vanishingly rare, and an unbounded
/// re-parse driven by the contents of a file is not a thing to leave lying around.
const MAX_BODY_DEPTH: u8 = 1;

/// Parse the contents of a dollar-quoted body.
///
/// Separate from [`crate::parser::SqlParser::parse`] only so the depth travels
/// with it; everything else is the same grammar and the same scope.
fn parse_nested(body: &str, scope: DialectScope, depth: u8) -> ParsedFile {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&crate::parser::language()).is_err() {
        return ParsedFile::empty(body, scope);
    }
    let Some(tree) = parser.parse(body.as_bytes(), None) else {
        return ParsedFile::empty(body, scope);
    };
    walk_at_depth(tree.root_node(), body, scope, depth)
}

/// Did the body yield anything worth reporting?
///
/// Deliberately lax, because the real guard is [`is_routine_body`] and it is a
/// question about the *grammar*: only `DO` and a routine body reach here, and no
/// sentence, however SQL-shaped, sits under those nodes. A second guard demanding
/// an error-free parse looked like prudence and was a straitjacket — plpgsql is
/// a procedural language this grammar only partly models, so a body containing an
/// `IF (SELECT …) THEN` it could not fully assemble contributed **nothing at
/// all**, and the version guard inside it was invisible. `VER001` then reported
/// the script as unguarded, which is the opposite of true.
///
/// So: whatever the parser could make out is kept, errors and all. What it could
/// not make out it simply did not report on, which is the same thing that happens
/// to any construct at the top level of a file.
fn looks_like_sql(parsed: &ParsedFile) -> bool {
    !parsed.statements.is_empty()
}

/// Move every position in a statement by `delta`.
///
/// One function rather than a `shift` method on each of the seven types it
/// touches: this is the only caller there will ever be, and spreading it across
/// the public API would invite somebody to shift half a statement.
fn shift_statement(statement: &mut Statement, delta: usize) {
    let shift = |r: &mut crate::range::ByteRange| {
        r.start += delta;
        r.end += delta;
    };
    shift(&mut statement.range);
    if let Some(select) = statement.select.as_mut() {
        select.select_list_end += delta;
    }
    for object in statement.defines.iter_mut().chain(statement.references.iter_mut()) {
        shift(&mut object.range);
    }
    for error in statement.foreign.iter_mut() {
        shift(&mut error.range);
    }
    for dml in &mut statement.dml {
        shift(&mut dml.table.range);
        for column in &mut dml.columns {
            shift(&mut column.range);
        }
        for row in &mut dml.rows {
            shift(&mut row.range);
            for cell in &mut row.values {
                shift(&mut cell.range);
            }
        }
        for assignment in &mut dml.assignments {
            shift(&mut assignment.range);
            shift(&mut assignment.column.range);
            shift(&mut assignment.value.range);
        }
        for range in [&mut dml.where_clause, &mut dml.returning, &mut dml.conflict] {
            if let Some(r) = range.as_mut() {
                shift(r);
            }
        }
    }
}

/// Was this written `CREATE OR REPLACE …`?
///
/// Read from the leading keywords rather than from a grammar field, because the
/// grammar does not carry one — `OR REPLACE` is an anonymous pair of tokens
/// between `CREATE` and the object kind. Reading the words is what the words are
/// for, and the two spellings that matter (`CREATE OR REPLACE FUNCTION`,
/// `CREATE OR REPLACE VIEW`) are both covered by looking at the first three.
fn declares_replacement(body: Node, source: &str) -> bool {
    let words = leading_keywords(body, source, 0);
    let mut upper = words.iter().map(|w| w.to_uppercase());
    upper.next().is_some_and(|first| first == "CREATE")
        && upper.next().is_some_and(|second| second == "OR")
        && upper.next().is_some_and(|third| third == "REPLACE")
}

fn kind_of(node_kind: &str) -> StatementKind {
    match node_kind {
        "select_statement" => StatementKind::Select,
        "insert_statement" => StatementKind::Insert,
        "update_statement" => StatementKind::Update,
        "delete_statement" => StatementKind::Delete,
        "merge_statement" => StatementKind::Merge,
        "truncate_statement" => StatementKind::Truncate,
        "drop_statement" => StatementKind::Drop,
        "comment_statement" => StatementKind::Comment,
        "grant_statement" => StatementKind::Grant,
        "revoke_statement" => StatementKind::Revoke,
        "set_statement" => StatementKind::Set,
        "transaction_statement" => StatementKind::Transaction,
        "call_statement" => StatementKind::Call,
        "plsql_block" | "do_statement" => StatementKind::Block,
        "slash_terminator" => StatementKind::Terminator,
        other if other.starts_with("create_") => StatementKind::Create,
        other if other.starts_with("alter_") => StatementKind::Alter,
        _ => StatementKind::Unknown,
    }
}

/// Node kinds that DEFINE an object, and the field holding its name.
const DEFINES: &[(&str, ObjectKind)] = &[
    ("create_table_statement", ObjectKind::Table),
    ("create_view_statement", ObjectKind::View),
    ("create_materialized_view_statement", ObjectKind::MaterializedView),
    ("create_index_statement", ObjectKind::Index),
    ("create_sequence_statement", ObjectKind::Sequence),
    ("create_schema_statement", ObjectKind::Schema),
    ("create_synonym_statement", ObjectKind::Synonym),
    ("create_trigger_statement", ObjectKind::Trigger),
    ("create_function_statement", ObjectKind::Function),
    ("create_procedure_statement", ObjectKind::Procedure),
    ("create_package_statement", ObjectKind::Package),
    ("create_package_body_statement", ObjectKind::PackageBody),
    ("create_type_statement", ObjectKind::Type),
    ("alter_table_statement", ObjectKind::Table),
    ("alter_sequence_statement", ObjectKind::Sequence),
    ("alter_index_statement", ObjectKind::Index),
    ("alter_view_statement", ObjectKind::View),
    ("alter_trigger_statement", ObjectKind::Trigger),
];

/// Node kinds that REFERENCE an object through a named field.
const FIELD_REFERENCES: &[(&str, &str, ObjectKind)] = &[
    ("table_reference", "name", ObjectKind::Table),
    ("references_clause", "table", ObjectKind::Table),
    ("create_index_statement", "table", ObjectKind::Table),
    ("create_trigger_statement", "table", ObjectKind::Table),
    ("create_synonym_statement", "target", ObjectKind::Table),
    ("insert_statement", "table", ObjectKind::Table),
    ("update_statement", "table", ObjectKind::Table),
    ("delete_statement", "table", ObjectKind::Table),
    ("merge_statement", "target", ObjectKind::Table),
    ("merge_statement", "source", ObjectKind::Table),
    ("comment_statement", "name", ObjectKind::Unknown),
];

struct Collector<'a> {
    source: &'a str,
    scope: DialectScope,
    /// How many dollar-quoted bodies this walk is already inside.
    depth: u8,
    defines: Vec<ObjectRef>,
    references: Vec<ObjectRef>,
    dml: Vec<DmlShape>,
    foreign: Vec<ForeignConstruct>,
    errors: Vec<ParseError>,
}

impl<'a> Collector<'a> {
    fn new(source: &'a str, scope: DialectScope, depth: u8) -> Self {
        Self {
            source,
            scope,
            depth,
            defines: Vec::new(),
            references: Vec::new(),
            dml: Vec::new(),
            foreign: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn visit(&mut self, node: Node, parent: Option<Node>) {
        self.record_errors(node, parent);
        self.record_foreign(node);
        self.record_objects(node);

        if let Some(shape) = dml::shape(node, self.source) {
            self.dml.push(shape);
        }

        if node.kind() == "dollar_quoted_string" && is_routine_body(parent) {
            self.descend_into_body(node);
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, Some(node));
        }
    }

    /// Read the SQL inside a `$$ … $$` body.
    ///
    /// Dollar quoting is a **token** to the grammar — the scanner swallows the
    /// whole body, delimiters and all, and hands back one leaf. That is right for
    /// a string literal and disastrous for the two places PostgreSQL actually uses
    /// it: `DO $$ … $$` and a function body. Everything a repository's PostgreSQL
    /// half does inside those was invisible — no objects, no DML, no coverage —
    /// so the same change written as an Oracle `DECLARE … BEGIN … END` (which the
    /// walker reads fine) and as a PostgreSQL function looked like a change one
    /// engine had and the other did not. That is `CONS001` reporting a gap that is
    /// not there, on the file where the two are most alike.
    ///
    /// ## The guard, and which way it errs
    ///
    /// Not every dollar-quoted string is SQL. `INSERT INTO note VALUES ($$see
    /// chapter 4; also 5$$)` is a sentence, and reading it as SQL would invent
    /// references to objects nobody named — a finding about a table that does not
    /// exist, which is far worse than a missed one.
    ///
    /// So the re-parse is **accepted only if it looks like SQL**: at least one
    /// statement, none of them carrying a syntax error, and at least one of a kind
    /// this crate recognises. Prose fails all three. Being wrong here costs a
    /// reference nobody sees — exactly the situation before this existed — and
    /// never an invented one.
    fn descend_into_body(&mut self, node: Node) {
        // One level. A function body holding another dollar-quoted body is legal
        // and vanishingly rare, and an unbounded re-parse driven by the contents
        // of a file is not a thing to leave lying around.
        if self.depth >= MAX_BODY_DEPTH {
            return;
        }

        let literal = text_of(node, self.source);
        let Some((offset, end)) = crate::literal::dollar_body_span(literal) else { return };
        let Some(body) = literal.get(offset..end) else { return };
        if body.trim().is_empty() {
            return;
        }

        let inner = parse_nested(body, self.scope, self.depth + 1);
        if !looks_like_sql(&inner) {
            return;
        }

        // Every range is rewritten to the outer file's coordinates, because every
        // consumer — the inventory's sites, "go to this line", the rewriter that
        // must reproduce the file byte for byte — measures against the file on
        // disk and knows nothing about a nested parse.
        let shift = range_of(node).start + offset;
        for mut statement in inner.statements {
            shift_statement(&mut statement, shift);
            self.defines.append(&mut statement.defines);
            self.references.append(&mut statement.references);
            self.dml.append(&mut statement.dml);
            // Foreign constructs are deliberately NOT carried over. A plpgsql body
            // is PostgreSQL by construction, and every `$$` in the repository would
            // otherwise report the dollar quoting around it as foreign syntax.
        }
    }

    fn record_errors(&mut self, node: Node, parent: Option<Node>) {
        let parent_kind = parent.map(|p| p.kind().to_string()).unwrap_or_default();
        if node.is_missing() {
            self.errors.push(ParseError {
                kind: ParseErrorKind::Missing,
                range: range_of(node),
                parent: parent_kind,
                text: String::new(),
                expected: Some(node.kind().to_string()),
            });
        } else if node.is_error() {
            self.errors.push(ParseError {
                kind: ParseErrorKind::Syntax,
                range: range_of(node),
                parent: parent_kind,
                text: truncate(text_of(node, self.source)),
                expected: None,
            });
        }
    }

    fn record_foreign(&mut self, node: Node) {
        if let Some((belongs_to, construct, message)) = dialect::classify_node(node.kind()) {
            if !self.scope.permits_syntax_of(belongs_to) {
                self.foreign.push(ForeignConstruct {
                    construct,
                    belongs_to,
                    message,
                    range: range_of(node),
                });
            }
        }
        if node.kind() == "function_call" {
            if let Some(name) = node.child_by_field_name("name") {
                if let Some((belongs_to, construct, message)) =
                    dialect::classify_function(text_of(name, self.source))
                {
                    if !self.scope.permits_syntax_of(belongs_to) {
                        self.foreign.push(ForeignConstruct {
                            construct,
                            belongs_to,
                            message,
                            range: range_of(name),
                        });
                    }
                }
            }
        }
        // `SYSDATE` and `now` without parentheses are ordinary names, so the
        // bare-word form has to be checked separately from the call form.
        if node.kind() == "object_name" {
            if let Some((belongs_to, construct, message)) =
                dialect::classify_function(text_of(node, self.source))
            {
                if !self.scope.permits_syntax_of(belongs_to)
                    && node.parent().map(|p| p.kind()) != Some("function_call")
                {
                    self.foreign.push(ForeignConstruct {
                        construct,
                        belongs_to,
                        message,
                        range: range_of(node),
                    });
                }
            }
        }
    }

    fn record_objects(&mut self, node: Node) {
        let kind = node.kind();

        for (k, object_kind) in DEFINES {
            if *k == kind {
                if let Some(r) = field_ref(node, "name", self.source, *object_kind) {
                    self.defines.push(r);
                }
            }
        }
        for (k, field, object_kind) in FIELD_REFERENCES {
            if *k == kind {
                if let Some(r) = field_ref(node, field, self.source, *object_kind) {
                    self.references.push(r);
                }
            }
        }

        match kind {
            // `DROP TABLE a, b` — the object type is a keyword phrase, and the
            // names follow it as a comma-separated list.
            "drop_statement" => {
                let object_kind = object_kind_from_keywords(&leading_keywords(node, self.source, 1));
                self.push_named_children(node, "object_name", object_kind);
            }
            "truncate_statement" => self.push_named_children(node, "object_name", ObjectKind::Table),
            // COMMENT ON <type> <name> — the field reference above got the name;
            // refine its kind from the keywords.
            "comment_statement" => {
                let object_kind = object_kind_from_keywords(&leading_keywords(node, self.source, 2));
                if let Some(last) = self.references.last_mut() {
                    if last.kind == ObjectKind::Unknown {
                        last.kind = object_kind;
                    }
                }
            }
            _ => {}
        }
    }

    fn push_named_children(&mut self, node: Node, child_kind: &str, object_kind: ObjectKind) {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == child_kind {
                if let Some(r) = object_ref(child, self.source, object_kind) {
                    self.references.push(r);
                }
            }
        }
    }
}

/// Cap the error text at a char boundary.
fn truncate(text: &str) -> String {
    if text.len() <= ERROR_TEXT_LIMIT {
        return text.to_string();
    }
    let mut end = ERROR_TEXT_LIMIT;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
