//! [`JarBlocks`] — the dependency tier as **one block per jar**, kept on disk and shared by every project.
//!
//! A dependency jar decodes to the same members whichever project asks, so what one project has decoded of `commons-lang3-3.14.0.jar`
//! is written to a file of its own, named after the jar's place in the Maven repository
//! (`<group path>/<artifact>/<version>/<jar>-<size>.members.json`), and a second project that depends on the same jar reads it
//! back as it is. There is no merged index and no stack of tiers: a lookup finds the jar that holds the class (a hash probe against
//! the central directory already in memory), and that jar's block answers.
//!
//! A block grows lazily, as classes of the jar are asked for, and is written at the same checkpoints as the JDK memo. Only resolved
//! classes reach disk. The jar's size is part of the file name, so a rebuilt `SNAPSHOT` starts a new block instead of serving the old
//! decode; blocks are kept under a schema directory, to be bumped when the decoder changes what it produces.

use std::path::{Path, PathBuf};

use bennu_classpath::prelude::{ClassAnnotations, ClassMembers, ClassSource, JarSource, MemberIndex};

use crate::jdk::JdkMemberIndex;

/// Bumped whenever the decoded form of a class changes, so a block written by an older decoder is never read.
pub const BLOCK_SCHEMA: u32 = 1;

/// The dependency jars of one project, each with its shared on-disk block.
pub struct JarBlocks {
    /// In classpath order: the first jar that holds a class answers for it.
    jars: Vec<JdkMemberIndex>,
    names: Vec<String>,
}

impl JarBlocks {
    /// Opens every jar that can be opened, each with the block `dir` keeps for it. A jar that cannot be opened is left out, as
    /// the combined source it replaces left it out.
    pub fn open(jars: &[PathBuf], dir: &Path) -> Self {
        let dir = dir.join(format!("v{BLOCK_SCHEMA}"));
        let sources = jars
            .iter()
            .filter_map(|jar| JarSource::open(jar).ok().map(|s| (block_path(&dir, jar), Box::new(s) as Box<dyn ClassSource>)))
            .collect();
        Self::from_sources(sources)
    }

    /// One block per `(path of its file, source of its classes)`.
    pub fn from_sources(sources: Vec<(PathBuf, Box<dyn ClassSource>)>) -> Self {
        let mut names = Vec::new();
        let jars = sources
            .into_iter()
            .map(|(block, source)| {
                names.extend(source.class_names());
                JdkMemberIndex::persistent(source, block)
            })
            .collect();
        Self { jars, names }
    }

    /// Every class name the jars hold, for the "Import class" list.
    pub fn class_names(&self) -> Vec<String> {
        self.names.clone()
    }

    /// The jar that holds `binary_name`.
    fn holder(&self, binary_name: &str) -> Option<&JdkMemberIndex> {
        self.jars.iter().find(|j| j.origin(binary_name).is_some())
    }

    /// The archive `binary_name` came out of.
    pub fn origin(&self, binary_name: &str) -> Option<PathBuf> {
        self.jars.iter().find_map(|j| j.origin(binary_name))
    }

    /// [`JdkMemberIndex::members_unmemoized`], routed to the jar that holds the class.
    pub fn members_unmemoized(&self, binary_name: &str) -> Option<ClassMembers> {
        self.holder(binary_name)?.members_unmemoized(binary_name)
    }

    /// Writes every block that has learned something since it was read.
    pub fn flush(&self) {
        self.jars.iter().for_each(JdkMemberIndex::flush);
    }

    /// `(classes decoded, estimated bytes)` over all blocks.
    pub fn memory_estimate(&self) -> (usize, usize) {
        self.jars.iter().map(JdkMemberIndex::memory_estimate).fold((0, 0), |(n, b), (cn, cb)| (n + cn, b + cb))
    }
}

impl MemberIndex for JarBlocks {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers> {
        self.holder(binary_name)?.members_of(binary_name)
    }

    fn class_annotations(&self, binary_name: &str) -> Option<ClassAnnotations> {
        self.holder(binary_name)?.class_annotations(binary_name)
    }

    /// Present in any jar is present; absent needs every jar to have said so, and one that cannot list its classes leaves it open.
    fn package_exists(&self, package: &str) -> Option<bool> {
        let mut all_no = true;
        for jar in &self.jars {
            match jar.package_exists(package) {
                Some(true) => return Some(true),
                Some(false) => {}
                None => all_no = false,
            }
        }
        all_no.then_some(false)
    }
}

/// Where the block of `jar` lives under `dir`: its place in a Maven repository when it has one, `_other` when it does not.
pub fn block_path(dir: &Path, jar: &Path) -> PathBuf {
    let size = std::fs::metadata(jar).map(|m| m.len()).unwrap_or(0);
    let stem = jar.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "jar".into());
    let parts: Vec<String> = jar.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
    let folder = parts
        .iter()
        .rposition(|p| p == "repository")
        .filter(|i| i + 1 < parts.len().saturating_sub(1))
        .map(|i| parts[i + 1..parts.len() - 1].join("/"))
        .unwrap_or_else(|| "_other".into());
    dir.join(folder).join(format!("{stem}-{size}.members.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A source that holds `class` and counts how often its bytes are read.
    struct Jar {
        class: &'static str,
        reads: Arc<AtomicUsize>,
    }

    impl ClassSource for Jar {
        fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
            if binary_name == self.class {
                self.reads.fetch_add(1, Ordering::SeqCst);
            }
            Ok(None)
        }
        fn class_names(&self) -> Vec<String> {
            vec![self.class.to_string()]
        }
        fn origin(&self, binary_name: &str) -> Option<PathBuf> {
            (binary_name == self.class).then(|| PathBuf::from(format!("/m2/{}.jar", self.class.replace('/', "_"))))
        }
    }

    fn blocks(reads: &Arc<AtomicUsize>) -> JarBlocks {
        let dir = std::env::temp_dir().join("bennu-shared-jars-test-unused");
        JarBlocks::from_sources(vec![
            (dir.join("a.json"), Box::new(Jar { class: "a/A", reads: Arc::clone(reads) })),
            (dir.join("b.json"), Box::new(Jar { class: "b/B", reads: Arc::clone(reads) })),
        ])
    }

    #[test]
    fn the_jar_that_holds_a_class_is_the_one_asked_and_the_others_are_not_touched() {
        let reads = Arc::new(AtomicUsize::new(0));
        let b = blocks(&reads);
        assert!(b.members_of("b/B").is_none(), "the fake serves no decodable bytes");
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert!(b.members_of("z/Z").is_none());
        assert_eq!(reads.load(Ordering::SeqCst), 1, "a class no jar holds reads nothing");
        assert_eq!(b.class_names(), vec!["a/A".to_string(), "b/B".to_string()]);
        assert_eq!(b.origin("a/A"), Some(PathBuf::from("/m2/a_A.jar")));
    }

    #[test]
    fn a_block_written_by_one_project_is_read_by_the_next_without_touching_the_jar() {
        let dir = std::env::temp_dir().join(format!("bennu-shared-jars-{}", std::process::id()));
        let path = dir.join("lib-1.json");
        // what a first project left behind: one decoded class
        let mut memo: std::collections::HashMap<String, Option<ClassMembers>> = std::collections::HashMap::new();
        memo.insert("a/A".to_string(), None);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, serde_json::to_vec(&memo).unwrap()).unwrap();

        let reads = Arc::new(AtomicUsize::new(0));
        let second = JarBlocks::from_sources(vec![(path, Box::new(Jar { class: "a/A", reads: Arc::clone(&reads) }))]);
        assert!(second.members_of("a/A").is_none(), "a block that holds a miss still answers");
        assert_eq!(reads.load(Ordering::SeqCst), 0, "the jar's bytes were never read");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_blocks_place_follows_the_jars_place_in_the_repository() {
        let dir = Path::new("/cache");
        let jar = Path::new("/home/u/.m2/repository/org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0.jar");
        assert_eq!(
            block_path(dir, jar),
            Path::new("/cache/org/apache/commons/commons-lang3/3.14.0/commons-lang3-3.14.0-0.members.json")
        );
        let loose = block_path(dir, Path::new("/work/libs/own.jar"));
        assert_eq!(loose, Path::new("/cache/_other/own-0.members.json"));
    }

    #[test]
    fn a_package_is_missing_only_when_every_jar_says_so() {
        let reads = Arc::new(AtomicUsize::new(0));
        let b = blocks(&reads);
        assert_eq!(b.package_exists("b"), Some(true), "one jar has it");
        assert_eq!(b.package_exists("q"), Some(false), "no jar has it");
    }
}
