//! The Java scan, and what makes a file worth running it on **for Spring**.
//!
//! The scan itself moved to [`bennu_facts`] the day a second framework needed it: an
//! annotation-shaped pass over Java is not a Spring idea, and JPA wanted the identical thing.
//! What stayed here is the only part that *is* a Spring idea — the relevance markers.
//!
//! The re-exports keep `crate::scan::…` meaning what it always meant inside this crate, so the
//! extraction cost no call site anything. New code outside this crate should reach for
//! `bennu_facts::prelude` directly.

pub use bennu_facts::prelude::{
    mentions_any, scan_java, AnnFacts, AnnString, FieldFacts, JavaFacts, MethodFacts, ParamFacts,
    TypeFacts,
};

/// Substrings that mean a file is worth parsing for Spring facts. A cheap `contains` filter
/// over the raw text, run before the tree-sitter pass — on a legacy tree the majority of files
/// match none of these and are never parsed.
///
/// Deliberately over-inclusive: a false hit costs one parse, a false miss costs a feature
/// silently not working on a file.
pub const SPRING_MARKERS: &[&str] = &[
    "@Value",
    "@Autowired",
    "@Inject",
    "@Resource",
    "@Component",
    "@Service",
    "@Repository",
    "@Controller",
    "@Configuration",
    "@Bean",
    "@Named",
    "@Qualifier",
    "@Mapping", // covers @RequestMapping / @GetMapping / @PostMapping / …
    "@Scheduled",
    "@ConfigurationProperties",
    "@Conditional",
    "@Profile",
    "@Primary",
    "@EventListener",
    "@Cacheable",
    "@PreAuthorize",
    "@Transactional",
    "springframework",
];

/// Whether `source` mentions anything Spring-shaped at all — the pre-filter that keeps the
/// scan proportional to the Spring surface of a project rather than to its size.
pub fn looks_spring_relevant(source: &str) -> bool {
    mentions_any(source, SPRING_MARKERS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefilter_admits_spring_files_and_rejects_plain_ones() {
        assert!(looks_spring_relevant("@Service public class A {}"));
        assert!(looks_spring_relevant("@GetMapping(\"/x\")"));
        assert!(looks_spring_relevant("import org.springframework.stereotype.Service;"));
        assert!(!looks_spring_relevant("public class PlainOldJava { int x; }"));
    }

    /// The marker that carries the most weight on a legacy tree: a properties POJO or a plain
    /// bean often has no annotation at all, and the import is the only signal.
    #[test]
    fn the_package_name_alone_is_enough() {
        assert!(looks_spring_relevant("import org.springframework.boot.SpringApplication;"));
    }
}
