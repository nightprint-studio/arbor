//! The project model: the types mappers reference, the configs they name, the build's default
//! policy, the generated implementations, and the panel's rows.
//!
//! ## Not every file is parsed
//!
//! Mappers are found by a substring (`org.mapstruct`). The types they reference are found by name —
//! a file is parsed only when it plausibly declares one of them (`class User`, `record UserDto`) —
//! and then, for a few rounds, the types *those* types' properties and supertypes reference. A
//! thousand-file tree with four mappers parses the four and the dozen DTOs they touch.

use std::collections::{HashMap, HashSet};

use bennu_ext::prelude::{ExtEntry, ProjectScan, ScannedFile};
use bennu_facts::prelude::{mentions_any, scan_java, JavaFacts};

use crate::catalog;
use crate::mapper::{mappers_in, Policy, ANNOTATIONS};
use crate::properties::type_info;
use crate::table::TypeTable;

/// How many rounds of "the types these types reference" are followed. A DTO graph nested deeper
/// than this is left partly unread, which silences the checks on it rather than confusing them.
const MAX_ROUNDS: usize = 6;
const DECLARING: &[&str] = &["class ", "record ", "interface ", "enum "];
/// The annotation-processor option that sets the default policy for the whole build.
const PROCESSOR_OPTION: &str = "mapstruct.unmappedTargetPolicy=";

/// What a `@MapperConfig` class says about unmapped targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigInfo {
    /// `None` when it does not set one — the build's default then applies.
    pub policy: Option<Policy>,
    /// It sets a `mappingInheritanceStrategy`, so prototype mappings may cover anything.
    pub inherits: bool,
}

/// What analysing a mapper needs to know about the rest of the project.
#[derive(Debug)]
pub struct Knowledge {
    pub table: TypeTable,
    pub configs: HashMap<String, ConfigInfo>,
    pub default_policy: Policy,
}

impl Default for Knowledge {
    /// Before a scan: nothing is known, so nothing is reported that needs knowing.
    fn default() -> Self {
        Self { table: TypeTable::default(), configs: HashMap::new(), default_policy: Policy::Unknown }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplMethod {
    pub name: String,
    pub arity: usize,
    pub offset: usize,
}

/// A generated `<Mapper>Impl`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impl {
    pub file: String,
    pub class: String,
    pub methods: Vec<ImplMethod>,
}

#[derive(Debug, Default)]
pub struct Model {
    pub knowledge: Knowledge,
    /// By the fqcn of the mapper they implement.
    pub impls: HashMap<String, Impl>,
    pub catalog: Vec<ExtEntry>,
    pub files_parsed: usize,
}

pub fn build(scan: &ProjectScan<'_>) -> Model {
    let mapper_files: Vec<(usize, JavaFacts)> = scan
        .java
        .iter()
        .enumerate()
        .filter(|(_, f)| mentions_any(&f.text, &["org.mapstruct"]))
        .filter_map(|(i, f)| scan_java(&slash(f), &f.text).map(|facts| (i, facts)))
        .collect();

    let (table, files_parsed) = read_types(scan.java, &mapper_files);
    let knowledge = Knowledge { table, configs: configs(&mapper_files), default_policy: default_policy(scan.xml) };

    let mut rows = Vec::new();
    for (i, facts) in &mapper_files {
        let text = &scan.java[*i].text;
        let mappers = mappers_in(facts, text, &knowledge);
        rows.extend(catalog::rows(&mappers, &knowledge.table, &facts.file, text));
    }
    rows.sort_by(|a, b| a.primary.cmp(&b.primary).then_with(|| a.id.cmp(&b.id)));

    Model { impls: impls(&mapper_files, scan.java), knowledge, catalog: rows, files_parsed }
}

/// The type table: every type in a mapper file, then the types they reference, round by round.
fn read_types(java: &[ScannedFile], mapper_files: &[(usize, JavaFacts)]) -> (TypeTable, usize) {
    let mut table = TypeTable::default();
    let mut parsed: HashSet<usize> = mapper_files.iter().map(|(i, _)| *i).collect();
    let mut declared: HashSet<String> = HashSet::new();
    let mut wanted: HashSet<String> = HashSet::new();

    for (_, facts) in mapper_files {
        for t in &facts.types {
            for m in &t.methods {
                add_names(&m.return_type, &mut wanted);
                for p in &m.params {
                    add_names(&p.type_text, &mut wanted);
                }
            }
            for a in &t.annotations {
                if let Some(config) = a.pair("config") {
                    add_names(config, &mut wanted);
                }
            }
            declared.insert(t.name.clone());
            table.insert(type_info(t, facts));
        }
    }

    for _ in 0..MAX_ROUNDS {
        wanted.retain(|n| !declared.contains(n));
        if wanted.is_empty() {
            break;
        }
        let mut next = HashSet::new();
        for (i, file) in java.iter().enumerate() {
            if parsed.contains(&i) || !declares_any(&file.text, &wanted) {
                continue;
            }
            parsed.insert(i);
            let Some(facts) = scan_java(&slash(file), &file.text) else { continue };
            for t in &facts.types {
                let info = type_info(t, &facts);
                for p in &info.props {
                    add_names(&p.type_text, &mut next);
                }
                add_names(&info.extends, &mut next);
                declared.insert(t.name.clone());
                table.insert(info);
            }
        }
        wanted = next;
    }
    (table, parsed.len())
}

/// The capitalised identifiers in a type as written — the candidates for a declared type's name.
fn add_names(text: &str, into: &mut HashSet<String>) {
    for token in text.split(|c: char| !is_ident(c)) {
        if token.chars().next().is_some_and(|c| c.is_uppercase()) {
            into.insert(token.to_string());
        }
    }
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Whether a file plausibly declares one of `names`. Over-inclusive (`subclass Foo` matches), which
/// costs a parse; under-inclusive would cost a silent feature.
fn declares_any(text: &str, names: &HashSet<String>) -> bool {
    DECLARING.iter().any(|keyword| {
        text.match_indices(keyword).any(|(at, _)| {
            let rest = &text[at + keyword.len()..];
            let end = rest.find(|c: char| !is_ident(c)).unwrap_or(rest.len());
            names.contains(&rest[..end])
        })
    })
}

fn configs(mapper_files: &[(usize, JavaFacts)]) -> HashMap<String, ConfigInfo> {
    let mut out = HashMap::new();
    for (_, facts) in mapper_files {
        for t in &facts.types {
            let Some(ann) = ANNOTATIONS.find(&t.annotations, facts, "MapperConfig") else { continue };
            out.insert(
                t.fqcn.clone(),
                ConfigInfo {
                    policy: ann.pair("unmappedTargetPolicy").map(Policy::parse),
                    inherits: ann.pair("mappingInheritanceStrategy").is_some(),
                },
            );
        }
    }
    out
}

/// The policy a mapper that sets none gets: MapStruct's `WARN`, unless the pom passes the processor
/// option. With no pom at all — a Gradle build, whose compiler arguments are not visible here — it is
/// unknown, and unmapped targets are not reported on mappers that do not say their policy themselves.
fn default_policy(xml: &[ScannedFile]) -> Policy {
    let poms: Vec<&ScannedFile> =
        xml.iter().filter(|f| f.path.file_name().is_some_and(|n| n == "pom.xml")).collect();
    if poms.is_empty() {
        return Policy::Unknown;
    }
    let mut found: Option<Policy> = None;
    for pom in poms {
        for (at, _) in pom.text.match_indices(PROCESSOR_OPTION) {
            let word: String =
                pom.text[at + PROCESSOR_OPTION.len()..].chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            let policy = Policy::parse(&word);
            match found {
                None => found = Some(policy),
                Some(previous) if previous != policy => return Policy::Unknown,
                Some(_) => {}
            }
        }
    }
    found.unwrap_or(Policy::Warn)
}

/// The generated `<Mapper>Impl` classes under `generated-sources`, by the mapper they implement.
fn impls(mapper_files: &[(usize, JavaFacts)], java: &[ScannedFile]) -> HashMap<String, Impl> {
    let mut out = HashMap::new();
    for (i, facts) in mapper_files {
        if !java[*i].path.to_string_lossy().contains("generated-sources") {
            continue;
        }
        for t in &facts.types {
            let Some(mapper) = t.name.strip_suffix("Impl") else { continue };
            let implements =
                t.implements.iter().chain(std::iter::once(&t.extends)).any(|s| simple_base(s) == mapper);
            let Some(fqcn) = t.fqcn.strip_suffix("Impl").filter(|_| implements) else { continue };
            let methods = t
                .methods
                .iter()
                .filter(|m| !m.is_constructor)
                .map(|m| ImplMethod { name: m.name.clone(), arity: m.params.len(), offset: m.name_offset })
                .collect();
            out.insert(fqcn.to_string(), Impl { file: facts.file.clone(), class: t.name.clone(), methods });
        }
    }
    out
}

fn simple_base(written: &str) -> &str {
    let base = written.split('<').next().unwrap_or(written).trim();
    base.rsplit('.').next().unwrap_or(base)
}

fn slash(file: &ScannedFile) -> String {
    file.path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(path: &str, text: &str) -> ScannedFile {
        ScannedFile { path: PathBuf::from(path), text: text.to_string() }
    }

    #[test]
    fn the_default_policy_comes_from_the_pom_or_is_unknown() {
        assert_eq!(default_policy(&[]), Policy::Unknown, "no pom: a build we cannot read");
        assert_eq!(default_policy(&[file("/p/pom.xml", "<project/>")]), Policy::Warn);
        let ignoring = file("/p/pom.xml", "<arg>-Amapstruct.unmappedTargetPolicy=IGNORE</arg>");
        assert_eq!(default_policy(&[ignoring]), Policy::Ignore);
    }

    #[test]
    fn a_file_is_read_when_it_declares_a_wanted_name() {
        let names: HashSet<String> = ["UserDto".to_string()].into_iter().collect();
        assert!(declares_any("public class UserDto {", &names));
        assert!(declares_any("public record UserDto(String a) {}", &names));
        assert!(!declares_any("public class UserDtoMapper {", &names));
        assert!(!declares_any("UserDto dto = new UserDto();", &names));
    }
}
