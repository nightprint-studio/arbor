//! [`ClassSource`] — one trait over every container of `.class` bytes.
//!
//! The three impls differ only in *how* they turn a binary class name
//! (`java/util/Optional`) into bytes; everything downstream (member index via
//! [`crate::meta`]) is identical. Ported from the two proven spike loaders
//! (`read_from_rt_jar` / `read_from_jimage`) unified behind this trait, plus a
//! directory loader for `target/classes`.
//!
//! | Impl            | Container                     | Resource path shape                    |
//! |-----------------|-------------------------------|----------------------------------------|
//! | [`DirSource`]   | dir of `.class` (target/…)    | `<root>/java/util/Optional.class`      |
//! | [`JarSource`]   | ZIP jar (rt.jar / deps)       | `java/util/Optional.class`             |
//! | [`JimageSource`]| jimage (`lib/modules`, JDK 9+)| `/<module>/java/util/Optional.class`   |
//!
//! The jimage path needs a module segment; JDK core classes live in `java.base`,
//! which [`JimageSource`] tries first (docs §10: "the only difference is the path").

use std::cell::RefCell;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::meta::{parse_class_meta, ClassMeta};

/// A container that can yield the bytes of a class by its binary name
/// (slash-separated, no `.class` suffix), and — for the source view — enumerate the
/// classes it holds. `class_names` is optional (the JDK containers are large; a
/// caller usually looks classes up by name), so it defaults to empty.
pub trait ClassSource {
    /// Raw bytes for `binary_name` (e.g. `java/util/Optional`), or `None` when the
    /// class is absent. `Err` only on an I/O / container-format failure — a missing
    /// class is `Ok(None)`, a normal non-fatal state (docs §8).
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String>;

    /// The resolved member index for `binary_name`, or `None` when absent. Default
    /// impl reads [`class_bytes`](Self::class_bytes) and decodes via [`crate::meta`]
    /// — no impl overrides this, it is the single decode path over both formats.
    fn class_meta(&self, binary_name: &str) -> Result<Option<ClassMeta>, String> {
        match self.class_bytes(binary_name)? {
            Some(bytes) => parse_class_meta(&bytes).map(Some),
            None => Ok(None),
        }
    }

    /// Enumerate the **binary class names** (`java/util/List`, slash form, no `.class`) this source
    /// holds — the raw list behind the class-name index that powers "Import class". Inner classes and
    /// `module-info`/`package-info` are NOT filtered here (the index normalises); this just lists what
    /// the container has. Default empty: a source that can't (or needn't) enumerate returns nothing;
    /// the JDK containers and dependency jars override it.
    fn class_names(&self) -> Vec<String> {
        Vec::new()
    }
}

// ── DirSource ────────────────────────────────────────────────────────────────

/// A plain directory of `.class` files, e.g. a Maven `target/classes`. The mutable
/// source view of a project (rebuilt after a compile feeds the index — docs §5 #19).
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    /// Root directory that holds the package tree of `.class` files.
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self { root: root.into() }
    }
}

impl ClassSource for DirSource {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        let rel = format!("{binary_name}.class");
        let path = self.root.join(rel);
        match File::open(&path) {
            Ok(mut f) => {
                let mut bytes = Vec::new();
                f.read_to_end(&mut bytes).map_err(|e| format!("read {}: {e}", path.display()))?;
                Ok(Some(bytes))
            }
            // Not found → absent (normal); any other I/O error is surfaced.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("open {}: {e}", path.display())),
        }
    }
}

// ── JarSource ────────────────────────────────────────────────────────────────

/// A ZIP jar of `.class` files — `rt.jar` on JDK 8, or a dependency jar. The
/// `ZipArchive` needs `&mut self` to read an entry, so it is held in a `RefCell`;
/// `ClassSource` takes `&self`, and the archive is only touched from one place, so a
/// single-threaded borrow is safe (per-source access is serialized by the caller).
pub struct JarSource {
    archive: RefCell<zip::ZipArchive<File>>,
}

impl JarSource {
    /// Open a jar at `path`. `Err` on a bad/absent archive.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("open jar {}: {e}", path.as_ref().display()))?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("read jar {}: {e}", path.as_ref().display()))?;
        Ok(Self { archive: RefCell::new(archive) })
    }
}

impl ClassSource for JarSource {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        let entry_name = format!("{binary_name}.class");
        let mut archive = self.archive.borrow_mut();
        // Bind the match to a local so the borrowed `ZipFile` in the `Ok` arm is
        // dropped before the guard's `archive` (zip 2.x ties the entry's lifetime to
        // the archive borrow; returning straight from the tail keeps it alive too long).
        let result = match archive.by_name(&entry_name) {
            Ok(mut zf) => {
                let mut bytes = Vec::new();
                zf.read_to_end(&mut bytes).map_err(|e| format!("read {entry_name}: {e}"))?;
                Ok(Some(bytes))
            }
            // `zip` returns FileNotFound as an error; map it to absent.
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(e) => Err(format!("jar entry {entry_name}: {e}")),
        };
        result
    }

    fn class_names(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut archive = self.archive.borrow_mut();
        for i in 0..archive.len() {
            let Ok(entry) = archive.by_index(i) else { continue };
            if let Some(binary) = entry.name().strip_suffix(".class") {
                out.push(binary.to_string());
            }
        }
        out
    }
}

// ── JimageSource ─────────────────────────────────────────────────────────────

/// The JDK 9+ `lib/modules` jimage container. A binary class name must be prefixed
/// with a module segment; JDK core classes live in `java.base`, tried first, then
/// any extra modules the caller registers via [`with_modules`](Self::with_modules).
pub struct JimageSource {
    image: jimage_rs::JImage,
    /// Modules to probe, in order. `java.base` first covers the JDK core (docs §10).
    modules: Vec<String>,
}

impl JimageSource {
    /// Open the jimage at `path` (typically `<jdk>/lib/modules`). Probes `java.base`
    /// by default.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let image = jimage_rs::JImage::open(path.as_ref())
            .map_err(|e| format!("open jimage {}: {e}", path.as_ref().display()))?;
        Ok(Self { image, modules: vec!["java.base".to_string()] })
    }

    /// Replace the probed-module list (e.g. add `java.sql`, `java.xml`). The order is
    /// the probe order; keep `java.base` first for core classes.
    pub fn with_modules(mut self, modules: Vec<String>) -> Self {
        self.modules = modules;
        self
    }
}

impl ClassSource for JimageSource {
    fn class_bytes(&self, binary_name: &str) -> Result<Option<Vec<u8>>, String> {
        for module in &self.modules {
            let resource = format!("/{module}/{binary_name}.class");
            match self.image.find_resource(&resource) {
                // jimage-rs 0.0.4 yields a `Cow<[u8]>`; own it for the `Vec<u8>` API.
                Ok(Some(bytes)) => return Ok(Some(bytes.into_owned())),
                Ok(None) => continue,
                Err(e) => return Err(format!("jimage resource {resource}: {e}")),
            }
        }
        Ok(None)
    }

    fn class_names(&self) -> Vec<String> {
        // Every `.class` resource across ALL modules (a project can reference java.sql / java.xml /
        // … too, not just java.base). `ResourceName` splits the path into parent + base; rebuild the
        // slash-form binary name and keep only class resources.
        let mut out = Vec::new();
        for rn in self.image.resource_names_iter() {
            let Ok(name) = rn else { continue };
            if name.extension.as_ref() != "class" {
                continue;
            }
            let parent = name.parent.as_ref();
            let base = name.base.as_ref();
            if parent.is_empty() {
                out.push(base.to_string());
            } else {
                out.push(format!("{parent}/{base}"));
            }
        }
        out
    }
}
