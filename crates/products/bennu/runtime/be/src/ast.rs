//! `ast` domain — the syntax tree of the buffer in front of the user.
//!
//! What the panel is for: **why did the parser read it that way?** Not what the file declares
//! (that is the Structure view, and it is a summary), but what the grammar actually built — the
//! node kinds, the fields, and the commas and keywords that are so often the answer.
//!
//! The walk itself is [`arbor_syntax`], which knows no Java: everything below is the choice of
//! grammar and the shape of the answer. Picus's syntax-tree panel is the same crate through the
//! same shape, which is why one component draws both.
//!
//! ## One registry, and it tells the truth about what it cannot do
//!
//! Bennu edits Java, XML, JSP, properties, SQL. It *parses* Java and JSP — those are the
//! grammars the workspace links. So this domain answers two things at once: which language a
//! file is in, and whether there is a grammar for it. A file whose language has no grammar is
//! not an error and must not read as one: the panel says "XML has no grammar yet" rather than
//! showing a failure, which is a true statement about the tool rather than an implied one about
//! the file.
//!
//! Adding a language is one arm of [`grammar_for`]. That is deliberately the whole cost: the
//! grammars this workspace links are a build-level fact (`tree-sitter` is a `links` native
//! library — exactly one version, workspace-wide), and every other part of this file is already
//! language-agnostic.
//!
//! ## Why the text comes from the caller
//!
//! A tree of what is *on disk* would be wrong from the first keystroke, and wrong exactly when
//! it matters — the moment you want the tree is the moment you have typed something the parser
//! reads differently than you expected. So the frontend sends the buffer.

use arbor_syntax::prelude::{
    node_path_at, outline, ByteRange, OutlineOptions, SyntaxTree,
};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use tree_sitter::Language;

/// The grammar that reads `ext`, or `None` when this workspace links none for it.
///
/// **This is the extension point.** A new language is one arm here plus its entry in
/// [`language_name`], and nothing else in this file changes.
fn grammar_for(ext: &str) -> Option<Language> {
    match ext {
        "java" => Some(bennu_java::prelude::java_language()),
        // The same generate the editor loads as wasm, so the panel and the colours in front of
        // it can never disagree about what the file is.
        e if is_jsp_ext(e) => Some(bennu_jsp_grammar::prelude::jsp_language()),
        _ => None,
    }
}

/// The JSP family. A `.tag` file is a page written in the same language, and a fragment
/// (`.jspf`) is a page that happens to be included — one list, because every place that asks
/// this question means all of them.
fn is_jsp_ext(ext: &str) -> bool {
    matches!(ext, "jsp" | "jspf" | "jspx" | "tag" | "tagx")
}

/// What to call the language of `ext`, whether or not there is a grammar for it.
///
/// Named even when unparseable, because that is what makes the panel's refusal informative: "no
/// grammar for XML yet" is a sentence, "nothing to show" is a shrug.
fn language_name(ext: &str) -> &'static str {
    match ext {
        "java" => "Java",
        "xml" | "xsd" | "dtd" | "tld" | "pom" => "XML",
        e if is_jsp_ext(e) => "JSP",
        "properties" => "Properties",
        "sql" => "SQL",
        "html" | "htm" => "HTML",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" => "TypeScript",
        "css" => "CSS",
        "json" => "JSON",
        "yml" | "yaml" => "YAML",
        "md" => "Markdown",
        "rs" => "Rust",
        "toml" => "TOML",
        "" => "a file with no extension",
        _ => "this file type",
    }
}

/// What to call the language of the file at `path`.
///
/// The registry, reached by path rather than by extension — [`crate::model_tree`] draws the other
/// half of the same panel and has to name a language it cannot read in exactly the same words.
pub fn language_name_of(path: &str) -> &'static str {
    language_name(&ext_of(path))
}

/// Whether `path` is Java.
pub fn is_java(path: &str) -> bool {
    ext_of(path) == "java"
}

/// Whether `path` is a JSP-family page. The second language with a grammar **and** a model, and
/// the reason [`crate::model_tree`] asks rather than assuming Java.
pub fn is_jsp(path: &str) -> bool {
    is_jsp_ext(&ext_of(path))
}

/// The lowercased extension of `path`, or `""` — `pom.xml` reads as `xml`, and a file with no
/// extension as nothing rather than as its own name.
fn ext_of(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// How much of the tree to walk. Every field optional — the defaults are what a panel opening on
/// an unknown file wants.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeRequest {
    #[serde(default)]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub max_nodes: Option<usize>,
    /// Hide the commas and the keywords. Off by default: the panel exists to explain a parse,
    /// and a stray comma is very often the explanation.
    #[serde(default)]
    pub named_only: bool,
}

impl TreeRequest {
    fn options(&self) -> OutlineOptions {
        let defaults = OutlineOptions::default();
        OutlineOptions {
            max_depth: self.max_depth,
            max_nodes: self.max_nodes.or(defaults.max_nodes),
            named_only: self.named_only,
            text_preview: defaults.text_preview,
        }
    }
}

/// The answer: which language, and its tree when there is a grammar.
///
/// Both parts always, rather than an error for the second case, because "no grammar for this
/// language" is a fact about Bennu and "the parse failed" is a fact about the file — and a panel
/// that showed them the same way would be lying about one of them.
#[derive(Debug, Serialize)]
pub struct AstAnswer {
    /// What the file is in — `Java`, `XML`. Always present.
    pub language: String,
    /// The tree, or `None` when no grammar reads that language yet.
    pub tree: Option<SyntaxTree>,
}

/// Args for [`bennu_syntax_tree_of`] and [`bennu_syntax_path_at`].
#[derive(Deserialize)]
pub struct SyntaxArgs {
    /// The buffer as the editor has it — not what is on disk (see the module doc).
    pub text: String,
    /// The file's path. Used only to pick the grammar, so an unsaved buffer can pass the name
    /// it would be saved under.
    pub path: String,
    #[serde(default)]
    pub request: Option<TreeRequest>,
    /// Byte offset, for [`bennu_syntax_path_at`] only.
    #[serde(default)]
    pub offset: usize,
}

/// The tree of `text`, read as whatever `path`'s extension says it is.
///
/// The whole domain, minus the seam — so it is testable without a backend state, which is the
/// line CLAUDE.md draws: the pure part is unit-tested and the handler is the wrapper.
fn tree_of(text: &str, path: &str, request: Option<TreeRequest>) -> Result<AstAnswer, String> {
    let ext = ext_of(path);
    let language = language_name(&ext).to_string();
    let Some(grammar) = grammar_for(&ext) else {
        return Ok(AstAnswer { language, tree: None });
    };
    let tree = outline(&grammar, text, &request.unwrap_or_default().options())
        .map_err(|e| e.to_string())?;
    Ok(AstAnswer { language, tree: Some(tree) })
}

/// The root-to-leaf chain of nodes holding `offset`. Empty for a language with no grammar, which
/// is the same nothing the panel already shows.
fn path_at(text: &str, path: &str, offset: usize) -> Result<Vec<ByteRange>, String> {
    let Some(grammar) = grammar_for(&ext_of(path)) else {
        return Ok(Vec::new());
    };
    node_path_at(&grammar, text, offset).map_err(|e| e.to_string())
}

/// The syntax tree of a buffer.
#[arbor_rpc::handler]
fn bennu_syntax_tree_of(_ctx: &BennuState, args: SyntaxArgs) -> Result<AstAnswer, String> {
    tree_of(&args.text, &args.path, args.request)
}

/// The root-to-leaf chain of nodes holding a byte offset — "reveal what the caret is in".
///
/// Answers with ranges rather than node ids: the panel already keys its tree by range, and an id
/// would be a second identity for the same node that could drift from the first.
#[arbor_rpc::handler]
fn bennu_syntax_path_at(_ctx: &BennuState, args: SyntaxArgs) -> Result<Vec<ByteRange>, String> {
    path_at(&args.text, &args.path, args.offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_java_buffer_yields_its_tree() {
        let answer = tree_of("class Order { int total; }", "/p/Order.java", None).expect("an answer");
        assert_eq!(answer.language, "Java");
        let tree = answer.tree.expect("a tree");
        assert_eq!(tree.root.kind, "program");
        assert!(!tree.has_errors);
        assert!(tree.node_count > 1);
    }

    /// The second grammar the workspace links, and the one a legacy project spends its day in.
    #[test]
    fn a_jsp_buffer_yields_its_tree() {
        let source = "<%@ taglib prefix=\"s\" uri=\"/struts-tags\" %>\n<s:property value=\"%{x}\"/>";
        let answer = tree_of(source, "/p/list.jsp", None).expect("an answer");
        assert_eq!(answer.language, "JSP");
        let tree = answer.tree.expect("a tree");
        assert_eq!(tree.root.kind, "document");
        assert!(tree.node_count > 1);
    }

    /// A fragment and a tag file are the same language as the page that includes them.
    #[test]
    fn the_whole_jsp_family_parses() {
        for path in ["/p/a.jspf", "/p/b.jspx", "/p/c.tag", "/p/d.tagx", "/p/E.JSP"] {
            let answer = tree_of("<p>hi</p>", path, None).expect("an answer");
            assert_eq!(answer.language, "JSP", "{path}");
            assert!(answer.tree.is_some(), "{path}");
        }
    }

    /// The case the panel must not render as a failure: Bennu edits XML and does not parse it.
    #[test]
    fn a_language_with_no_grammar_is_named_rather_than_refused() {
        let answer = tree_of("<beans/>", "/p/beans.xml", None).expect("an answer, not an error");
        assert_eq!(answer.language, "XML");
        assert!(answer.tree.is_none());
    }

    /// A file that will not parse still has a tree — that is the whole point of the panel, and
    /// `has_errors` is how it says so.
    #[test]
    fn broken_java_still_parses_and_says_it_is_broken() {
        let answer = tree_of("class Order { int total( }", "/p/Order.java", None).expect("an answer");
        assert!(answer.tree.expect("a tree").has_errors);
    }

    /// `named_only` is the toggle in the panel's header, and it must actually change the walk —
    /// the punctuation is what a reading of "why did it parse that way" usually turns on.
    #[test]
    fn hiding_the_punctuation_yields_a_smaller_tree() {
        let source = "class Order { void a() {} void b() {} }";
        let all = tree_of(source, "/p/Order.java", None).expect("a").tree.expect("t");
        let named = tree_of(
            source,
            "/p/Order.java",
            Some(TreeRequest { max_depth: None, max_nodes: None, named_only: true }),
        )
        .expect("a")
        .tree
        .expect("t");
        assert!(named.node_count < all.node_count);
    }

    #[test]
    fn the_path_to_an_offset_runs_root_to_leaf() {
        let source = "class Order { int total; }";
        let at = source.find("total").expect("the field");
        let path = path_at(source, "/p/Order.java", at).expect("a path");
        assert!(path.len() > 1, "root, and everything down to the identifier");
        // Each step is inside the one before it — that is what makes it a path.
        for pair in path.windows(2) {
            assert!(pair[1].start >= pair[0].start && pair[1].end <= pair[0].end);
        }
    }

    #[test]
    fn a_language_with_no_grammar_has_no_path_rather_than_an_error() {
        assert!(path_at("<beans/>", "/p/beans.xml", 2).expect("no error").is_empty());
    }

    #[test]
    fn the_extension_is_read_from_the_file_name_not_the_path() {
        assert_eq!(ext_of("/p/src/Order.java"), "java");
        assert_eq!(ext_of("C:\\p\\Order.JAVA"), "java", "case does not decide a grammar");
        assert_eq!(ext_of("/p/pom.xml"), "xml");
        assert_eq!(ext_of("/some.dir/Makefile"), "", "a dot in a DIRECTORY is not an extension");
        assert_eq!(ext_of("Makefile"), "");
    }
}
