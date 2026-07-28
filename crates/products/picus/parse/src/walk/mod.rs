//! CST → [`ParsedFile`].
//!
//! One traversal per top-level statement collects everything at once: the
//! objects, the DML shapes, the foreign constructs and the errors. The walk
//! deliberately descends into procedural bodies — in a real upgrade script the
//! INSERT that matters is three blocks deep inside `DECLARE … BEGIN … END`, and
//! a walker that stopped at the block would report an empty file.

mod dml;
mod names;

use picus_types::prelude::DialectScope;
use tree_sitter::Node;

use crate::dialect::{self, ForeignConstruct};
use crate::dml::DmlShape;
use crate::error::{ParseError, ParseErrorKind, ERROR_TEXT_LIMIT};
use crate::object::{ObjectKind, ObjectRef};
use crate::statement::{ParsedFile, Statement, StatementKind};
use names::{field_ref, leading_keywords, object_kind_from_keywords, object_ref, range_of, text_of};

pub(crate) fn walk_file(root: Node, source: &str, scope: DialectScope) -> ParsedFile {
    let mut statements = Vec::new();
    let mut errors = Vec::new();

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        match child.kind() {
            "statement" | "slash_terminator" | "ERROR" => {
                let statement = statement_of(child, source, scope, &mut errors);
                statements.push(statement);
            }
            // Comments and whitespace: they belong to the gaps.
            _ => {}
        }
    }

    ParsedFile { scope, source_len: source.len(), statements, errors }
}

fn statement_of(
    node: Node,
    source: &str,
    scope: DialectScope,
    file_errors: &mut Vec<ParseError>,
) -> Statement {
    // The body is the first named child; the `;` is anonymous and the Oracle `/`
    // follows it. A bare `/` at top level is its own statement.
    let body = if node.kind() == "statement" { node.named_child(0) } else { Some(node) };
    let node_kind = body.map(|b| b.kind().to_string()).unwrap_or_else(|| node.kind().to_string());

    let mut collector = Collector::new(source, scope);
    collector.visit(node, None);

    let before = file_errors.len();
    file_errors.extend(collector.errors.iter().cloned());
    let has_error = file_errors.len() > before || node.kind() == "ERROR";

    Statement {
        kind: kind_of(&node_kind),
        range: range_of(node),
        node_kind,
        defines: collector.defines,
        references: collector.references,
        dml: collector.dml,
        foreign: collector.foreign,
        has_error,
    }
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
    defines: Vec<ObjectRef>,
    references: Vec<ObjectRef>,
    dml: Vec<DmlShape>,
    foreign: Vec<ForeignConstruct>,
    errors: Vec<ParseError>,
}

impl<'a> Collector<'a> {
    fn new(source: &'a str, scope: DialectScope) -> Self {
        Self {
            source,
            scope,
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

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.visit(child, Some(node));
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
