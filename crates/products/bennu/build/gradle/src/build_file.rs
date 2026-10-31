//! One `build.gradle` / `build.gradle.kts`, read for what it declares.
//!
//! Gradle's equivalent of reading a pom — and the equivalent of the pom's three hard questions,
//! too: *which version am I getting*, *who decided that*, *where is it written*. The answer to the
//! first is rarely next to the dependency. It is `"$springVersion"` with the variable defined in an
//! `ext {}` block (or `gradle.properties`), or `libs.spring.web` with the number in the version
//! catalog, or nothing at all because a BOM decides. [`VersionOrigin`] keeps that answer next to
//! the number.
//!
//! ## What is read, and what is not
//!
//! The declarative subset: `plugins {}`, `apply plugin:`, `dependencies {}` (string notation, map
//! notation, `project(…)`, `platform(…)`, `kotlin(…)`, catalog accessors), `allprojects {}` /
//! `subprojects {}` dependency blocks, the Java level, `ext` variables, declared tasks. A
//! dependency produced by a loop, a function or a convention plugin is **not** here — it takes a
//! Gradle run to see — and the dependency panel says so rather than showing a list that looks
//! complete.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::catalog::{Catalog, CatalogLibrary};
use crate::script::{is_ident, literals, paren_inner, split_top_level, Script};

/// Where a dependency's version was decided.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum VersionOrigin {
    /// Written in the notation itself (`"g:a:1.2"`).
    Literal,
    /// A variable: an `ext` entry, a local `val`, or a `gradle.properties` key.
    Property(String),
    /// A version-catalog alias.
    Catalog { catalog: String, alias: String },
    /// Nothing in the build files says — a BOM or the dependency-management plugin supplies it.
    #[default]
    Unversioned,
}

/// One dependency, as the build file writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawDep {
    /// `implementation`, `testImplementation`, `compileOnly`, … as written.
    pub configuration: String,
    pub group: String,
    pub name: String,
    /// With `$variables` expanded where they could be; left as written where they could not.
    pub version: String,
    pub classifier: String,
    pub extension: String,
    /// `:sibling` for a `project(":sibling")` dependency; empty for an artifact.
    pub project: String,
    /// `platform` (a BOM), `test-fixtures`, or empty.
    pub kind: String,
    pub origin: VersionOrigin,
    /// Byte offset of the statement and its 1-based line — what a row jumps to.
    pub offset: usize,
    pub line: u32,
}

/// One entry of `plugins {}` or an `apply plugin:`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginDecl {
    pub id: String,
    pub version: String,
    /// `apply false`: declared so subprojects can use it, not applied here.
    pub apply: bool,
    pub offset: usize,
    pub line: u32,
}

/// The Java level a script declares, and the key it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaLevel {
    pub version: String,
    /// `toolchain`, `release`, `sourceCompatibility`, `targetCompatibility` or `jvmTarget`.
    pub source: String,
}

/// Everything read from one build script.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildInfo {
    pub plugins: Vec<PluginDecl>,
    /// This project's own `dependencies {}`.
    pub dependencies: Vec<RawDep>,
    /// `allprojects { dependencies {} }` — every project, this one included.
    pub all_projects: Vec<RawDep>,
    /// `subprojects { dependencies {} }` — every project below this one.
    pub sub_projects: Vec<RawDep>,
    /// BOMs imported with the dependency-management plugin (`mavenBom "…"`).
    pub boms: Vec<String>,
    pub java: Option<JavaLevel>,
    pub group: String,
    pub version: String,
    pub main_class: String,
    /// `options.encoding = '…'` — the source encoding the compile task is told, which is Gradle's
    /// `sourceEncoding`. Empty when the script says nothing.
    pub encoding: String,
    /// Tasks the script declares itself (`tasks.register("x")`, `task x`).
    pub tasks: Vec<String>,
    /// The variables this script defines, for the subprojects that inherit them.
    pub variables: BTreeMap<String, String>,
}

/// What a script is read against.
pub struct Env<'a> {
    /// `gradle.properties`.
    pub properties: &'a BTreeMap<String, String>,
    /// Variables the parent project defines (`ext {}` in the root build file).
    pub inherited: &'a BTreeMap<String, String>,
    pub catalogs: &'a [Catalog],
}

impl BuildInfo {
    pub fn parse(text: &str, env: &Env) -> BuildInfo {
        let script = Script::new(text);
        let mut info = BuildInfo { variables: collect_variables(&script), ..BuildInfo::default() };

        let mut vars = env.properties.clone();
        vars.extend(env.inherited.iter().map(|(k, v)| (k.clone(), v.clone())));
        vars.extend(info.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
        let lookup = Lookup { vars, catalogs: env.catalogs };

        for block in script.blocks(script.all()) {
            match block.head.as_str() {
                "plugins" => info.plugins.extend(plugins_in(&script, block.body(), &lookup)),
                "dependencies" => info.dependencies.extend(dependencies_in(&script, block.body(), &lookup)),
                "allprojects" | "subprojects" => {
                    let target = block.head.clone();
                    for inner in script.blocks(block.body()) {
                        if inner.head == "dependencies" {
                            let deps = dependencies_in(&script, inner.body(), &lookup);
                            if target == "allprojects" {
                                info.all_projects.extend(deps);
                            } else {
                                info.sub_projects.extend(deps);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        info.plugins.extend(legacy_plugins(&script));
        info.boms = boms_in(&script, &lookup);
        info.java = java_level(script.text(script.all()));
        info.tasks = declared_tasks(&script);
        read_scalars(&script, &mut info);
        fill_kotlin_versions(&mut info);
        info
    }

    /// Whether any applied plugin has this id (or this id as a prefix: `org.jetbrains.kotlin.jvm`
    /// answers `org.jetbrains.kotlin`).
    pub fn applies(&self, id: &str) -> bool {
        self.plugins.iter().any(|p| p.apply && (p.id == id || p.id.starts_with(&format!("{id}."))))
    }
}

// ── variables ──────────────────────────────────────────────────────────────────

/// Variable definitions, line by line: `ext.x = 'v'`, `def x = 'v'`, `val x = "v"`,
/// `val x by extra("v")`, `ext.set("x", "v")`, `versions = [spring: '5', junit: '4']`.
fn collect_variables(script: &Script) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();
    for line in script.text(script.all()).lines() {
        let mut l = line.trim();
        // `ext { springVersion = '5' }` on one line: the assignment is what is inside the braces.
        if let Some(inner) = l.strip_prefix("ext").map(str::trim_start).and_then(|r| r.strip_prefix('{')) {
            l = inner.trim().trim_end_matches('}').trim();
        }
        if l.is_empty() {
            continue;
        }
        if let Some((name, value)) = by_extra(l).or_else(|| set_call(l)) {
            vars.insert(name, value);
            continue;
        }
        let Some(eq) = assignment_at(l) else { continue };
        let (lhs, rhs) = (&l[..eq], l[eq + 1..].trim());
        let Some(name) = variable_name(lhs) else { continue };
        if let Some(inner) = rhs.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            for part in split_top_level(inner) {
                if let Some((key, value)) = part.split_once(':') {
                    if let Some(lit) = literals(value).into_iter().next() {
                        vars.insert(format!("{name}.{}", key.trim().trim_matches(|c: char| c == '"' || c == '\'')), lit.text);
                    }
                }
            }
            continue;
        }
        if let Some(lit) = literals(rhs).into_iter().next() {
            if lit.start == 0 && rhs[lit.end..].trim().is_empty() {
                vars.insert(name, lit.text);
            }
        }
    }
    vars
}

/// The `=` of an assignment — not `==`, `=>`, `<=`, `>=`, `!=`.
fn assignment_at(l: &str) -> Option<usize> {
    let b = l.as_bytes();
    let mut quote: Option<u8> = None;
    for i in 0..b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => {
                if c == b'"' || c == b'\'' {
                    quote = Some(c);
                } else if c == b'=' {
                    let prev = if i > 0 { b[i - 1] } else { b' ' };
                    let next = b.get(i + 1).copied().unwrap_or(b' ');
                    if next != b'=' && next != b'>' && !matches!(prev, b'=' | b'<' | b'>' | b'!') {
                        return Some(i);
                    }
                    return None;
                }
            }
        }
    }
    None
}

/// `val x by extra("v")`
fn by_extra(l: &str) -> Option<(String, String)> {
    let at = l.find(" by extra(")?;
    let name = l[..at].trim().trim_start_matches("val ").trim_start_matches("var ").trim();
    let value = literals(&l[at..]).into_iter().next()?;
    is_plain_name(name).then(|| (name.to_string(), value.text))
}

/// `ext.set("x", "v")` / `extra.set(...)` / `set("x", "v")` / `extra["x"] = "v"` is handled as an assignment.
fn set_call(l: &str) -> Option<(String, String)> {
    let at = l.find("set(")?;
    let lits = literals(&l[at..]);
    if lits.len() == 2 && lits[0].start < 6 {
        Some((lits[0].text.clone(), lits[1].text.clone()))
    } else {
        None
    }
}

/// The variable name on the left of an assignment, or `None` when it is something else
/// (`implementation`, `options.encoding`, a method call…).
fn variable_name(lhs: &str) -> Option<String> {
    let mut name = lhs.trim().to_string();
    for prefix in ["def ", "val ", "var ", "final ", "String ", "static ", "rootProject.ext.", "project.ext.", "ext.", "extra."] {
        while let Some(rest) = name.strip_prefix(prefix) {
            name = rest.trim_start().to_string();
        }
    }
    if name.contains('[') {
        // `extra["x"]`, `ext['x']`
        name = literals(&name).into_iter().next()?.text;
    }
    is_plain_name(&name).then_some(name)
}

fn is_plain_name(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|c| is_ident(c) || c == b'.' || c == b'-') && !s.starts_with('.')
}

// ── variable lookup and `$` expansion ──────────────────────────────────────────

struct Lookup<'a> {
    vars: BTreeMap<String, String>,
    catalogs: &'a [Catalog],
}

impl<'a> Lookup<'a> {
    /// The value of a variable expression — `springVersion`, `rootProject.ext.springVersion`,
    /// `versions.spring`, `libs.versions.spring.get()`.
    fn value_of(&self, expr: &str) -> Option<String> {
        for catalog in self.catalogs {
            if let Some(v) = catalog.version_for(expr) {
                return Some(v.to_string());
            }
        }
        self.vars.get(&variable_key(expr)).cloned()
    }

    /// `s` with every `$x` / `${x}` that can be resolved replaced, and the name of the first
    /// variable that was. What cannot be resolved stays as written: a `${undefined}` is usually the
    /// bug, and hiding it would hide the bug.
    fn expand(&self, s: &str) -> (String, Option<String>) {
        let mut out = String::with_capacity(s.len());
        let mut used: Option<String> = None;
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] != b'$' || i + 1 >= b.len() {
                let ch_len = s[i..].chars().next().map(char::len_utf8).unwrap_or(1);
                out.push_str(&s[i..i + ch_len]);
                i += ch_len;
                continue;
            }
            let (expr, end) = if b[i + 1] == b'{' {
                match s[i + 2..].find('}') {
                    Some(close) => (s[i + 2..i + 2 + close].to_string(), i + 2 + close + 1),
                    None => {
                        out.push_str(&s[i..]);
                        break;
                    }
                }
            } else {
                let run = s[i + 1..]
                    .bytes()
                    .take_while(|&c| is_ident(c) || c == b'.')
                    .count();
                (s[i + 1..i + 1 + run].trim_end_matches('.').to_string(), i + 1 + run)
            };
            // `$a.b` is ambiguous between a dotted name and `$a` followed by text: take the longest
            // prefix that is a known variable.
            let mut candidate = expr.clone();
            let mut resolved = None;
            loop {
                if let Some(v) = self.value_of(&candidate) {
                    resolved = Some((candidate.clone(), v));
                    break;
                }
                match candidate.rfind('.') {
                    Some(dot) if b[i + 1] != b'{' => candidate.truncate(dot),
                    _ => break,
                }
            }
            match resolved {
                Some((name, value)) => {
                    used.get_or_insert(name.clone());
                    out.push_str(&value);
                    if b[i + 1] == b'{' {
                        i = end;
                    } else {
                        i = i + 1 + name.len();
                    }
                }
                None => {
                    out.push_str(&s[i..end]);
                    i = end;
                }
            }
        }
        (out, used)
    }
}

/// The key a variable expression is stored under: scope prefixes, accessors and call syntax removed.
fn variable_key(expr: &str) -> String {
    let mut e = expr.trim().to_string();
    for suffix in [".get()", ".toString()", ".toString", "!!"] {
        if let Some(rest) = e.strip_suffix(suffix) {
            e = rest.trim().to_string();
        }
    }
    if e.contains('[') || e.contains('(') {
        if let Some(lit) = literals(&e).into_iter().next() {
            return lit.text;
        }
    }
    loop {
        let before = e.len();
        for prefix in ["rootProject.", "project.", "ext.", "extra.", "properties."] {
            if let Some(rest) = e.strip_prefix(prefix) {
                e = rest.to_string();
            }
        }
        if e.len() == before {
            return e;
        }
    }
}

// ── plugins ────────────────────────────────────────────────────────────────────

fn plugins_in(script: &Script, body: Range<usize>, lookup: &Lookup) -> Vec<PluginDecl> {
    let mut out = Vec::new();
    for stmt in script.statements(body) {
        let raw = script.text(stmt.clone());
        let lead = raw.len() - raw.trim_start().len();
        let text = raw.trim().replace('`', "");
        if text.is_empty() {
            continue;
        }
        let offset = stmt.start + lead;
        let line = script.line_of(offset);
        let lits = literals(&text);
        let mut decl = PluginDecl { apply: true, offset, line, ..PluginDecl::default() };
        if text.starts_with("id") && lits.first().is_some() {
            decl.id = lits[0].text.clone();
        } else if text.starts_with("kotlin(") && lits.first().is_some() {
            decl.id = format!("org.jetbrains.kotlin.{}", lits[0].text);
        } else if let Some(inner) = text.strip_prefix("alias(").and_then(|r| r.strip_suffix(')')) {
            let found = lookup.catalogs.iter().find_map(|c| c.plugin_for(inner.trim()));
            match found {
                Some(p) => {
                    decl.id = p.id.clone();
                    decl.version = p.version.clone();
                }
                None => continue,
            }
        } else if text.bytes().all(|c| is_ident(c) || c == b'.' || c == b'-') {
            decl.id = text.clone();
        } else {
            continue;
        }
        if lits.len() >= 2 && text.contains("version") {
            decl.version = lookup.expand(&lits[1].text).0;
        }
        if text.contains("apply false") || text.contains("apply(false)") {
            decl.apply = false;
        }
        out.push(decl);
    }
    out
}

/// `apply plugin: 'java'` / `apply(plugin = "java")`.
fn legacy_plugins(script: &Script) -> Vec<PluginDecl> {
    let mut out = Vec::new();
    for at in script.find_word("apply") {
        let end = script.statement_end(at);
        let text = script.text(at..end);
        if !text.contains("plugin") {
            continue;
        }
        if let Some(lit) = literals(text).into_iter().next() {
            out.push(PluginDecl {
                id: lit.text,
                apply: true,
                offset: at,
                line: script.line_of(at),
                ..PluginDecl::default()
            });
        }
    }
    out
}

fn boms_in(script: &Script, lookup: &Lookup) -> Vec<String> {
    let mut out = Vec::new();
    for at in script.find_word("mavenBom") {
        let end = script.statement_end(at);
        if let Some(lit) = literals(script.text(at..end)).into_iter().next() {
            out.push(lookup.expand(&lit.text).0);
        }
    }
    out
}

// ── dependencies ───────────────────────────────────────────────────────────────

/// Words that open a statement inside `dependencies {}` without being a configuration.
const NOT_CONFIGURATIONS: [&str; 17] = [
    "constraints", "components", "modules", "attributesSchema", "artifactTypes", "dependencySubstitution",
    "imports", "val", "var", "def", "if", "else", "for", "return", "while", "exclude", "mavenBom",
];

fn dependencies_in(script: &Script, body: Range<usize>, lookup: &Lookup) -> Vec<RawDep> {
    let mut out = Vec::new();
    for stmt in script.statements(body) {
        let raw = script.text(stmt.clone());
        let lead = raw.len() - raw.trim_start().len();
        let text = raw.trim_start();
        let Some((configuration, rest)) = split_configuration(text) else { continue };
        if NOT_CONFIGURATIONS.contains(&configuration.as_str()) || rest.trim_start().starts_with('=') {
            continue;
        }
        let args = argument_text(rest);
        if args.trim().is_empty() {
            continue;
        }
        let offset = stmt.start + lead;
        let line = script.line_of(offset);
        for mut dep in parse_args(&args, lookup) {
            dep.configuration = configuration.clone();
            dep.offset = offset;
            dep.line = line;
            out.push(dep);
        }
    }
    out
}

/// `implementation 'x'` → (`implementation`, ` 'x'`); `"implementation"("x")` likewise.
fn split_configuration(t: &str) -> Option<(String, &str)> {
    let first = *t.as_bytes().first()?;
    if first == b'"' || first == b'\'' {
        let lit = literals(t).into_iter().next()?;
        return Some((lit.text, &t[lit.end..]));
    }
    let end = t.bytes().position(|c| !is_ident(c)).unwrap_or(t.len());
    if end == 0 {
        return None;
    }
    let rest = &t[end..];
    match rest.chars().next() {
        Some(c) if c.is_whitespace() || c == '(' || c == '"' || c == '\'' => Some((t[..end].to_string(), rest)),
        _ => None,
    }
}

/// What follows the configuration name, without the parentheses of the call form or the trailing
/// closure of the command form.
fn argument_text(rest: &str) -> String {
    let r = rest.trim_start();
    if let Some(inner) = paren_inner(r) {
        return inner.to_string();
    }
    let b = r.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for i in 0..b.len() {
        let c = b[i];
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth = (depth - 1).max(0),
                b'{' if depth == 0 => return r[..i].to_string(),
                _ => {}
            },
        }
    }
    r.to_string()
}

fn parse_args(args: &str, lookup: &Lookup) -> Vec<RawDep> {
    let parts = split_top_level(args);
    if parts.iter().any(|p| named_key(p).is_some()) {
        return named_dependency(&parts, lookup).into_iter().collect();
    }
    parts.iter().flat_map(|p| parse_one(p.trim(), lookup)).collect()
}

/// The key of a `group: 'x'` / `name = "x"` argument.
fn named_key(part: &str) -> Option<&'static str> {
    let t = part.trim();
    let end = t.bytes().position(|c| !is_ident(c))?;
    let key = &t[..end];
    let after = t[end..].trim_start();
    let named = (after.starts_with(':') && !after.starts_with("::")) || (after.starts_with('=') && !after.starts_with("=="));
    if !named {
        return None;
    }
    ["group", "name", "version", "classifier", "ext", "module"].into_iter().find(|k| *k == key)
}

fn named_dependency(parts: &[String], lookup: &Lookup) -> Option<RawDep> {
    let mut dep = RawDep::default();
    let mut version_expr = String::new();
    for part in parts {
        let Some(key) = named_key(part) else { continue };
        let value = part.trim().split_once(|c: char| c == ':' || c == '=').map(|(_, v)| v.trim()).unwrap_or("");
        let text = match literals(value).into_iter().next() {
            Some(lit) if lit.double_quoted => lookup.expand(&lit.text).0,
            Some(lit) => lit.text,
            None => lookup.value_of(value).unwrap_or_else(|| value.to_string()),
        };
        match key {
            "group" => dep.group = text,
            "name" => dep.name = text,
            "version" => {
                version_expr = value.to_string();
                dep.version = text;
            }
            "classifier" => dep.classifier = text,
            "ext" => dep.extension = text,
            "module" => {
                let (g, n) = text.split_once(':').unwrap_or((text.as_str(), ""));
                dep.group = g.to_string();
                dep.name = n.to_string();
            }
            _ => {}
        }
    }
    if dep.name.is_empty() {
        return None;
    }
    dep.origin = origin_of(&dep.version, &version_expr, lookup);
    Some(dep)
}

/// Where a version came from, given the expression that produced it.
fn origin_of(version: &str, expr: &str, lookup: &Lookup) -> VersionOrigin {
    if version.is_empty() {
        return VersionOrigin::Unversioned;
    }
    let (_, used) = lookup.expand(expr.trim_matches(|c: char| c == '"' || c == '\''));
    if let Some(name) = used {
        return VersionOrigin::Property(name);
    }
    if !expr.contains('"') && !expr.contains('\'') && lookup.value_of(expr).is_some() {
        return VersionOrigin::Property(variable_key(expr));
    }
    VersionOrigin::Literal
}

fn parse_one(p: &str, lookup: &Lookup) -> Vec<RawDep> {
    let Some(&first) = p.as_bytes().first() else { return Vec::new() };
    if first == b'"' || first == b'\'' {
        return coordinate_literal(p, lookup).into_iter().collect();
    }
    if let Some((func, inner)) = call_form(p) {
        match func {
            "project" => return project_dependency(inner).into_iter().collect(),
            "platform" | "enforcedPlatform" => return tagged(parse_args(inner, lookup), "platform"),
            "testFixtures" => return tagged(parse_args(inner, lookup), "test-fixtures"),
            "kotlin" => return kotlin_module(inner).into_iter().collect(),
            "files" | "fileTree" | "gradleApi" | "localGroovy" | "gradleTestKit" | "embeddedKotlin" => return Vec::new(),
            _ => {}
        }
    }
    if let Some(path) = p.strip_prefix("projects.") {
        return typesafe_project(path).into_iter().collect();
    }
    catalog_or_variable(p, lookup)
}

fn tagged(mut deps: Vec<RawDep>, kind: &str) -> Vec<RawDep> {
    for d in &mut deps {
        d.kind = kind.to_string();
    }
    deps
}

/// `name(inner)` → (`name`, `inner`).
fn call_form(p: &str) -> Option<(&str, &str)> {
    let end = p.bytes().position(|c| !is_ident(c))?;
    if end == 0 {
        return None;
    }
    let inner = paren_inner(&p[end..])?;
    Some((&p[..end], inner))
}

fn project_dependency(inner: &str) -> Option<RawDep> {
    let lits = literals(inner);
    let path = lits.iter().find(|l| l.text.starts_with(':')).or_else(|| lits.first())?.text.clone();
    let name = path.rsplit(':').next().unwrap_or(&path).to_string();
    Some(RawDep { project: crate::settings::project_path(&path), name, ..RawDep::default() })
}

/// `projects.core.api` → `:core:api`, with camelCase read as kebab-case (`projects.myLib` is
/// `:my-lib`).
fn typesafe_project(path: &str) -> Option<RawDep> {
    let path = path.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.')).next()?;
    let segments: Vec<String> = path.split('.').filter(|s| !s.is_empty()).map(kebab).collect();
    if segments.is_empty() {
        return None;
    }
    Some(RawDep {
        project: format!(":{}", segments.join(":")),
        name: segments.last().cloned().unwrap_or_default(),
        ..RawDep::default()
    })
}

fn kebab(camel: &str) -> String {
    let mut out = String::new();
    for c in camel.chars() {
        if c.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `kotlin("stdlib")` → `org.jetbrains.kotlin:kotlin-stdlib`, version filled in later from the plugin.
fn kotlin_module(inner: &str) -> Option<RawDep> {
    let lit = literals(inner).into_iter().next()?;
    Some(RawDep {
        group: "org.jetbrains.kotlin".to_string(),
        name: format!("kotlin-{}", lit.text),
        ..RawDep::default()
    })
}

fn coordinate_literal(p: &str, lookup: &Lookup) -> Option<RawDep> {
    let lit = literals(p).into_iter().next()?;
    if lit.start != 0 {
        return None;
    }
    let (text, used) = if lit.double_quoted { lookup.expand(&lit.text) } else { (lit.text.clone(), None) };
    let mut dep = parse_coordinate(&text)?;
    dep.origin = match (&used, dep.version.is_empty()) {
        (_, true) => VersionOrigin::Unversioned,
        (Some(name), false) => VersionOrigin::Property(name.clone()),
        (None, false) => VersionOrigin::Literal,
    };
    Some(dep)
}

/// `group:name[:version[:classifier]][@ext]`.
pub fn parse_coordinate(s: &str) -> Option<RawDep> {
    let (body, extension) = match s.rsplit_once('@') {
        Some((b, e)) if !e.contains(':') => (b, e),
        _ => (s, ""),
    };
    if body.contains(char::is_whitespace) {
        return None;
    }
    let parts: Vec<&str> = body.split(':').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return None;
    }
    Some(RawDep {
        group: parts[0].to_string(),
        name: parts[1].to_string(),
        version: parts.get(2).copied().unwrap_or("").to_string(),
        classifier: parts.get(3).copied().unwrap_or("").to_string(),
        extension: extension.to_string(),
        ..RawDep::default()
    })
}

fn catalog_or_variable(p: &str, lookup: &Lookup) -> Vec<RawDep> {
    for catalog in lookup.catalogs {
        if let Some(lib) = catalog.library_for(p) {
            return vec![from_catalog(lib, &catalog.name)];
        }
        let bundle = catalog.bundle_for(p);
        if !bundle.is_empty() {
            return bundle.into_iter().map(|lib| from_catalog(lib, &catalog.name)).collect();
        }
    }
    match lookup.value_of(p) {
        Some(value) => match parse_coordinate(&value) {
            Some(mut dep) => {
                dep.origin = if dep.version.is_empty() {
                    VersionOrigin::Unversioned
                } else {
                    VersionOrigin::Property(variable_key(p))
                };
                vec![dep]
            }
            None => Vec::new(),
        },
        None => Vec::new(),
    }
}

fn from_catalog(lib: &CatalogLibrary, catalog: &str) -> RawDep {
    RawDep {
        group: lib.group.clone(),
        name: lib.name.clone(),
        version: lib.version.clone(),
        origin: VersionOrigin::Catalog { catalog: catalog.to_string(), alias: lib.alias.clone() },
        ..RawDep::default()
    }
}

/// `kotlin("stdlib")` carries no version: the Kotlin plugin's version is the answer.
fn fill_kotlin_versions(info: &mut BuildInfo) {
    let Some(kotlin) = info
        .plugins
        .iter()
        .find(|p| p.id.starts_with("org.jetbrains.kotlin") && !p.version.is_empty())
        .map(|p| p.version.clone())
    else {
        return;
    };
    for dep in info.dependencies.iter_mut().filter(|d| d.group == "org.jetbrains.kotlin" && d.version.is_empty()) {
        dep.version = kotlin.clone();
        dep.origin = VersionOrigin::Property("kotlin plugin".to_string());
    }
}

// ── the Java level ─────────────────────────────────────────────────────────────

/// The Java level a script declares. Toolchain wins over `release`, which wins over the
/// source/target pair — the same order Gradle itself applies them in.
fn java_level(code: &str) -> Option<JavaLevel> {
    let keys: [(&str, &str); 6] = [
        ("languageVersion", "toolchain"),
        ("jvmToolchain", "toolchain"),
        ("options.release", "release"),
        ("sourceCompatibility", "sourceCompatibility"),
        ("targetCompatibility", "targetCompatibility"),
        ("jvmTarget", "jvmTarget"),
    ];
    for (key, source) in keys {
        let Some(at) = code.find(key) else { continue };
        let line_end = code[at..].find('\n').map(|p| at + p).unwrap_or(code.len());
        if let Some(version) = level_in(&code[at + key.len()..line_end]) {
            return Some(JavaLevel { version, source: source.to_string() });
        }
    }
    None
}

/// The level a fragment such as ` = JavaVersion.VERSION_1_8`, `= '17'` or `.set(JavaLanguageVersion.of(21))` names.
fn level_in(fragment: &str) -> Option<String> {
    for marker in ["VERSION_", "JVM_"] {
        if let Some(at) = fragment.find(marker) {
            let token: String = fragment[at + marker.len()..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '_')
                .collect();
            let level = token.trim_matches('_').replace('_', ".");
            return (!level.is_empty()).then_some(level);
        }
    }
    let start = fragment.find(|c: char| c.is_ascii_digit())?;
    let level: String = fragment[start..].chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let level = level.trim_end_matches('.').to_string();
    (!level.is_empty()).then_some(level)
}

// ── tasks and scalars ──────────────────────────────────────────────────────────

fn declared_tasks(script: &Script) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let code = script.text(script.all());
    for needle in ["tasks.register", "tasks.create"] {
        let mut from = 0;
        while let Some(rel) = code[from..].find(needle) {
            let at = from + rel;
            from = at + needle.len();
            let end = script.statement_end(at);
            if let Some(lit) = literals(script.text(from..end.max(from))).into_iter().next() {
                out.push(lit.text);
            }
        }
    }
    for at in script.find_word("task") {
        let after = code[at + 4..].trim_start();
        let name = match paren_inner(after) {
            Some(inner) => literals(inner).into_iter().next().map(|l| l.text),
            None => {
                let ident: String = after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                (!ident.is_empty()).then_some(ident)
            }
        };
        out.extend(name);
    }
    for line in code.lines() {
        if let Some(at) = line.find(" by tasks.registering") {
            let name = line[..at].trim().trim_start_matches("val ").trim();
            if is_plain_name(name) {
                out.push(name.to_string());
            }
        }
    }
    let mut seen = Vec::new();
    out.retain(|t| {
        let fresh = !seen.contains(t);
        seen.push(t.clone());
        fresh
    });
    out
}

/// `group`, `version`, `mainClass` — the single-literal assignments worth showing.
fn read_scalars(script: &Script, info: &mut BuildInfo) {
    for line in script.text(script.all()).lines() {
        let l = line.trim();
        let Some(lit) = literals(l).into_iter().next() else { continue };
        if l.starts_with("group") && assignment_at(l).is_some() && info.group.is_empty() {
            info.group = lit.text;
        } else if l.starts_with("version") && assignment_at(l).is_some() && info.version.is_empty() {
            info.version = lit.text;
        } else if (l.starts_with("mainClass") || l.starts_with("mainClassName")) && info.main_class.is_empty() {
            info.main_class = lit.text;
        } else if (l.starts_with("options.encoding") || l.contains(".options.encoding") || l.starts_with("encoding"))
            && assignment_at(l).is_some()
            && info.encoding.is_empty()
        {
            info.encoding = lit.text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> BuildInfo {
        parse_with(text, &[])
    }

    fn parse_with(text: &str, catalogs: &[Catalog]) -> BuildInfo {
        let none = BTreeMap::new();
        BuildInfo::parse(text, &Env { properties: &none, inherited: &none, catalogs })
    }

    fn gav(d: &RawDep) -> String {
        format!("{}:{}:{}", d.group, d.name, d.version)
    }

    #[test]
    fn groovy_and_kotlin_notations_read_the_same_dependencies() {
        let groovy = parse("dependencies {\n  implementation 'org.a:one:1.0'\n  testImplementation(\"org.b:two:2.0\")\n}");
        let kotlin = parse("dependencies {\n  implementation(\"org.a:one:1.0\")\n  testImplementation(\"org.b:two:2.0\")\n}");
        let g: Vec<String> = groovy.dependencies.iter().map(gav).collect();
        let k: Vec<String> = kotlin.dependencies.iter().map(gav).collect();
        assert_eq!(g, ["org.a:one:1.0", "org.b:two:2.0"]);
        assert_eq!(g, k);
        assert_eq!(groovy.dependencies[1].configuration, "testImplementation");
        assert_eq!(groovy.dependencies[0].line, 2);
    }

    #[test]
    fn map_notation_and_a_trailing_closure() {
        let info = parse("dependencies {\n  implementation group: 'org.a', name: 'one', version: '1.0'\n  api(\"org.b:two:2\") { exclude group: 'x', module: 'y' }\n}");
        let deps: Vec<String> = info.dependencies.iter().map(gav).collect();
        assert_eq!(deps, ["org.a:one:1.0", "org.b:two:2"]);
    }

    #[test]
    fn a_variable_is_expanded_and_remembered() {
        let info = parse("ext { springVersion = '5.3.10' }\ndependencies {\n  implementation \"org.springframework:spring-web:$springVersion\"\n  api \"org.x:y:${springVersion}\"\n}");
        assert_eq!(info.dependencies[0].version, "5.3.10");
        assert_eq!(info.dependencies[0].origin, VersionOrigin::Property("springVersion".to_string()));
        assert_eq!(info.dependencies[1].version, "5.3.10");
    }

    #[test]
    fn an_undefined_variable_stays_as_written() {
        let info = parse("dependencies { implementation \"org.x:y:$nowhere\" }");
        assert_eq!(info.dependencies[0].version, "$nowhere");
        assert_eq!(info.dependencies[0].origin, VersionOrigin::Literal);
    }

    #[test]
    fn a_catalog_accessor_resolves_and_says_where_from() {
        let catalog = Catalog::parse(
            "libs",
            "",
            "[versions]\nspring = \"5.3.10\"\n[libraries]\nspring-web = { module = \"org.springframework:spring-web\", version.ref = \"spring\" }\n",
        );
        let info = parse_with("dependencies {\n  implementation(libs.spring.web)\n  implementation libs.spring.web\n}", &[catalog]);
        assert_eq!(info.dependencies.len(), 2);
        assert_eq!(gav(&info.dependencies[0]), "org.springframework:spring-web:5.3.10");
        assert_eq!(
            info.dependencies[0].origin,
            VersionOrigin::Catalog { catalog: "libs".to_string(), alias: "spring-web".to_string() }
        );
    }

    #[test]
    fn project_and_platform_dependencies() {
        let info = parse("dependencies {\n  implementation project(':core')\n  api(project(\":web:client\"))\n  implementation(platform(\"org.s:bom:3.0\"))\n  implementation(projects.myLib)\n}");
        let projects: Vec<&str> = info.dependencies.iter().map(|d| d.project.as_str()).collect();
        assert_eq!(projects, [":core", ":web:client", "", ":my-lib"]);
        assert_eq!(info.dependencies[2].kind, "platform");
        assert_eq!(gav(&info.dependencies[2]), "org.s:bom:3.0");
    }

    #[test]
    fn a_commented_out_dependency_is_not_one() {
        let info = parse("dependencies {\n  // implementation 'a:b:1'\n  /* api 'c:d:2' */\n  implementation 'e:f:3'\n}");
        assert_eq!(info.dependencies.len(), 1);
        assert_eq!(info.dependencies[0].name, "f");
    }

    #[test]
    fn buildscript_dependencies_are_not_project_dependencies() {
        let info = parse("buildscript {\n  dependencies { classpath 'com.x:plugin:1' }\n}\ndependencies { implementation 'a:b:1' }");
        assert_eq!(info.dependencies.len(), 1);
    }

    #[test]
    fn allprojects_and_subprojects_are_kept_apart() {
        let info = parse("allprojects { dependencies { compileOnly 'a:one:1' } }\nsubprojects { dependencies { testImplementation 'b:two:2' } }");
        assert_eq!(info.all_projects.len(), 1);
        assert_eq!(info.sub_projects.len(), 1);
        assert!(info.dependencies.is_empty());
    }

    #[test]
    fn plugins_in_both_syntaxes() {
        let info = parse("plugins {\n  id 'java-library'\n  id(\"org.springframework.boot\") version \"3.1.0\"\n  id 'x.y' version '1' apply false\n  `maven-publish`\n  kotlin(\"jvm\") version \"1.9.0\"\n}\napply plugin: 'application'");
        let ids: Vec<&str> = info.plugins.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["java-library", "org.springframework.boot", "x.y", "maven-publish", "org.jetbrains.kotlin.jvm", "application"]);
        assert_eq!(info.plugins[1].version, "3.1.0");
        assert!(!info.plugins[2].apply);
        assert!(info.applies("org.jetbrains.kotlin"));
        assert!(!info.applies("x.y"));
    }

    #[test]
    fn kotlin_stdlib_takes_the_plugin_version() {
        let info = parse("plugins { kotlin(\"jvm\") version \"1.9.0\" }\ndependencies { implementation(kotlin(\"stdlib\")) }");
        assert_eq!(gav(&info.dependencies[0]), "org.jetbrains.kotlin:kotlin-stdlib:1.9.0");
    }

    #[test]
    fn the_java_level_in_each_spelling() {
        let level = |s: &str| parse(s).java.map(|j| (j.version, j.source));
        assert_eq!(level("sourceCompatibility = '1.8'"), Some(("1.8".into(), "sourceCompatibility".into())));
        assert_eq!(level("java { sourceCompatibility = JavaVersion.VERSION_17 }"), Some(("17".into(), "sourceCompatibility".into())));
        assert_eq!(level("sourceCompatibility = JavaVersion.VERSION_1_8"), Some(("1.8".into(), "sourceCompatibility".into())));
        assert_eq!(
            level("java { toolchain { languageVersion.set(JavaLanguageVersion.of(21)) } }\nsourceCompatibility = 8"),
            Some(("21".into(), "toolchain".into()))
        );
        assert_eq!(level("kotlin { jvmToolchain(17) }"), Some(("17".into(), "toolchain".into())));
        assert_eq!(level("tasks.withType(JavaCompile) { options.release = 11 }"), Some(("11".into(), "release".into())));
        assert_eq!(level("dependencies {}"), None);
    }

    #[test]
    fn declared_tasks_in_each_spelling() {
        let info = parse("task hello { }\ntasks.register(\"docs\") { }\ntasks.register<Copy>(\"stage\")\nval zip by tasks.registering { }");
        assert_eq!(info.tasks, ["docs", "stage", "hello", "zip"]);
    }

    #[test]
    fn the_source_encoding_is_read_from_the_compile_options() {
        let info = parse("tasks.withType(JavaCompile) {\n  options.encoding = 'ISO-8859-1'\n}\ngroup = 'x'");
        assert_eq!(info.encoding, "ISO-8859-1");
        assert_eq!(info.group, "x");
    }

    #[test]
    fn map_literal_variables() {
        let info = parse("ext.versions = [spring: '5.0', junit: '4.13']\ndependencies { testImplementation \"junit:junit:${versions.junit}\" }");
        assert_eq!(info.dependencies[0].version, "4.13");
    }
}
