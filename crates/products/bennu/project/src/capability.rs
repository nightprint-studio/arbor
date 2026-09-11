//! Capability detection — the **Spike D ruleset** (docs §10).
//!
//! Signal tiers:
//! - **A** = a pom dependency coordinate (strongest).
//! - **B** = presence / path of a config file.
//! - **C** = an annotation / package / import / expression pattern in the sources
//!   (corroborating).
//!
//! A capability activates on **≥1 strong signal (A or B)**. A **C-only / transitive**
//! match is a *provisional* activation at low priority (recorded in the hits with
//! `tier="C"`), never a hard-fail — false positives are poison here (docs §7).
//!
//! This module walks the pom (tier A), a bounded set of well-known config paths
//! (tier B), and a bounded sample of source files (tier C). The source scan is
//! capped so opening a huge legacy tree stays responsive (docs §8: reparse-whole
//! stalls); Phase-0 detection only needs *presence*, so a capped scan is faithful.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bennu_proto::prelude::{CapabilityHit, CapabilitySet};

use crate::pom::Pom;

/// Max number of source files scanned for tier-C evidence, and max bytes read per
/// file. Detection only needs presence, so a bounded scan keeps `open_project`
/// responsive on a 1200-file legacy tree.
const MAX_SOURCE_FILES: usize = 400;
const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// How deep the source walk goes below a module's `src/`. `main/java` is two levels before the
/// package starts, and `it/acme/product/module/web/dto` is six more — the old 8 stopped a level short
/// of where a DTO usually lives, which is precisely where `@NotNull` and `@JsonProperty` are written.
const SOURCE_WALK_DEPTH: usize = 16;
/// Deeper than any reactor anybody maintains, and a hard stop on a `<module>` loop.
const MAX_REACTOR_DEPTH: usize = 12;

/// Detect the domain capabilities of the project rooted at `root`, given its parsed root [`Pom`].
///
/// Reads every module of the reactor, but not the resolved dependency tree — a caller that has one
/// uses [`detect_with`]. Pure over (poms + filesystem); never fails (an unreadable file is simply
/// absent evidence).
pub fn detect(root: &Path, pom: &Pom) -> CapabilitySet {
    detect_with(&BuildEvidence::read(root, pom))
}

/// What capability detection reads a build from.
///
/// ## Why not just the root pom
///
/// It used to be exactly that, and it missed most of what a project has. A reactor's root pom is an
/// aggregator: it lists modules and, typically, declares nothing — the dependencies are written in
/// the modules, and so are the config files and the sources. And what a module writes is rarely the
/// artifact a capability is recognised by. Nobody declares `jakarta.validation-api`: they declare
/// `spring-boot-starter-web`, or inherit a company parent, and the API arrives three levels down.
/// Reading the root pom's own `<dependency>` blocks, Bean Validation was off on a project that
/// validates every form it has.
///
/// So the evidence is every pom of the reactor, every module's directories, and — when Maven has
/// resolved it — the dependency tree.
#[derive(Debug, Clone)]
pub struct BuildEvidence {
    /// Every directory of the reactor, the root first.
    modules: Vec<PathBuf>,
    /// `groupId:artifactId`, lowercase: what every pom declares, then what the build resolves to.
    coordinates: Vec<String>,
}

impl BuildEvidence {
    /// The build as its poms declare it: `root`'s own, then each `<module>`, recursively.
    ///
    /// Follows the declaration rather than walking the tree, so a `samples/` directory with a pom of
    /// its own is not read as part of the project.
    pub fn read(root: &Path, pom: &Pom) -> Self {
        let mut build = Self { modules: vec![root.to_path_buf()], coordinates: pom.dependencies.clone() };
        let mut seen: HashSet<PathBuf> = HashSet::from([root.to_path_buf()]);
        build.collect_modules(root, pom, 0, &mut seen);
        build
    }

    /// Add what the build **resolves** to — the transitive tree, as `groupId:artifactId`. Empty
    /// until Maven has resolved it once, which is why it is added rather than required.
    pub fn with_resolved(mut self, coordinates: impl IntoIterator<Item = String>) -> Self {
        self.coordinates.extend(coordinates.into_iter().map(|c| c.to_ascii_lowercase()));
        self
    }

    fn root(&self) -> &Path {
        &self.modules[0]
    }

    /// The same rule as [`Pom::has_dependency`] — a case-insensitive substring of a coordinate —
    /// over the whole build.
    fn has_dependency(&self, needle: &str) -> bool {
        let needle = needle.to_ascii_lowercase();
        self.coordinates.iter().any(|c| c.contains(&needle))
    }

    fn collect_modules(&mut self, dir: &Path, pom: &Pom, depth: usize, seen: &mut HashSet<PathBuf>) {
        if depth > MAX_REACTOR_DEPTH {
            return;
        }
        for module in &pom.modules {
            let module = module.trim().trim_end_matches('/');
            if module.is_empty() {
                continue;
            }
            // Maven allows a module to name its pom file as well as its directory.
            let base = dir.join(module);
            let (module_dir, pom_path) = match base.is_file() {
                true => (base.parent().map(Path::to_path_buf).unwrap_or_else(|| dir.to_path_buf()), base),
                false => (base.clone(), base.join("pom.xml")),
            };
            if !seen.insert(module_dir.clone()) {
                continue;
            }
            let Ok(xml) = std::fs::read_to_string(&pom_path) else { continue };
            let child = crate::pom::parse(&xml);
            self.coordinates.extend(child.dependencies.iter().cloned());
            self.modules.push(module_dir.clone());
            self.collect_modules(&module_dir, &child, depth + 1, seen);
        }
    }
}

/// Detect the domain capabilities of a build — see [`BuildEvidence`] for what that is read from.
pub fn detect_with(build: &BuildEvidence) -> CapabilitySet {
    let root = build.root();
    let mut set = CapabilitySet::default();
    let mut hits: Vec<CapabilityHit> = Vec::new();

    // Tier B: gather the well-known config-file signals once, from every module.
    let files = ConfigFiles::scan(&build.modules);
    // Tier C: gather a bounded sample of source-pattern signals once, from every module.
    let src = SourceSignals::scan(&build.modules);

    // ── StrutsXmlConfig ──────────────────────────────────────────────────────
    let struts_a = build.has_dependency("struts2-core");
    let struts_b = files.struts_xml || files.struts_plugin_xml;
    activate(
        &mut set.struts_xml_config,
        &mut hits,
        "struts_xml_config",
        struts_a.then_some("dependency struts2-core"),
        struts_b.then(|| files.first_struts_file()),
        src.filter_dispatcher.then_some("FilterDispatcher / <action> in source"),
    );

    // ── StrutsConvention ─────────────────────────────────────────────────────
    let conv_a = build.has_dependency("struts2-convention-plugin");
    activate(
        &mut set.struts_convention,
        &mut hits,
        "struts_convention",
        conv_a.then_some("dependency struts2-convention-plugin"),
        None,
        src.action_annotation.then_some("@Action / @Namespace in source"),
    );

    // ── JspViews ─────────────────────────────────────────────────────────────
    // Presence of pages, nothing more. It gates the JSP-only tooling, so it must be true for a
    // plain page with no taglib and false for a module that has no pages at all.
    activate(
        &mut set.jsp_views,
        &mut hits,
        "jsp_views",
        None,
        src.has_jsp.then_some("*.jsp / *.jspf / *.tag in the project"),
        None,
    );

    // ── JspTaglibTld ─────────────────────────────────────────────────────────
    activate(
        &mut set.jsp_taglib_tld,
        &mut hits,
        "jsp_taglib_tld",
        None,
        files.has_tld.then_some("*.tld under WEB-INF"),
        src.taglib_directive.then_some("<%@ taglib %> directive"),
    );

    // ── OgnlValueStack (follows StrutsXmlConfig) ─────────────────────────────
    let ognl_strong = set.struts_xml_config; // treated as a strong follow-on
    activate(
        &mut set.ognl_value_stack,
        &mut hits,
        "ognl_value_stack",
        None,
        ognl_strong.then_some("follows struts_xml_config"),
        src.ognl_expr.then_some("%{…} / ${…} expressions"),
    );

    // ── TilesViews ───────────────────────────────────────────────────────────
    let tiles_a = build.has_dependency("struts2-tiles-plugin") || build.has_dependency("tiles-");
    activate(
        &mut set.tiles_views,
        &mut hits,
        "tiles_views",
        tiles_a.then_some("dependency struts2-tiles-plugin / tiles-*"),
        files.tiles_xml.then_some("tiles.xml"),
        src.tiles_result.then_some("result type=\"tiles\""),
    );

    // ── SpringXmlDi ──────────────────────────────────────────────────────────
    let spring_a = build.has_dependency("spring-beans")
        || build.has_dependency("spring-context")
        || build.has_dependency("spring-jdbc");
    activate(
        &mut set.spring_xml_di,
        &mut hits,
        "spring_xml_di",
        spring_a.then_some("dependency spring-beans / spring-context / spring-jdbc"),
        files.spring_beans_xml.then_some("root XML <beans>"),
        src.get_bean.then_some("getBean(...) / ContextLoaderListener"),
    );

    // ── SpringAnnotationDi ───────────────────────────────────────────────────
    let spring_ann_a = build.has_dependency("spring-context");
    activate(
        &mut set.spring_annotation_di,
        &mut hits,
        "spring_annotation_di",
        None, // needs component-scan too — keep A conservative (B carries it)
        (spring_ann_a && files.component_scan).then_some("<context:component-scan>"),
        src.spring_stereotype.then_some("@Component / @Service / @Autowired"),
    );

    // ── SpringDataRepo ───────────────────────────────────────────────────────
    activate(
        &mut set.spring_data_repo,
        &mut hits,
        "spring_data_repo",
        build.has_dependency("spring-data-").then_some("dependency spring-data-*"),
        None,
        src.jpa_repository.then_some("extends JpaRepository / CrudRepository"),
    );

    // ── JpaHibernate ─────────────────────────────────────────────────────────
    activate(
        &mut set.jpa_hibernate,
        &mut hits,
        "jpa_hibernate",
        build.has_dependency("hibernate-core").then_some("dependency hibernate-core"),
        (files.persistence_xml || files.hbm_xml).then_some("persistence.xml / *.hbm.xml"),
        src.jpa_entity.then_some("@Entity / @Table / EntityManager"),
    );

    // ── MyBatisMapper ────────────────────────────────────────────────────────
    let mybatis_a = build.has_dependency("mybatis");
    activate(
        &mut set.mybatis_mapper,
        &mut hits,
        "mybatis_mapper",
        mybatis_a.then_some("dependency mybatis / mybatis-spring"),
        files.mapper_xml.then_some("*Mapper.xml / sqlMapConfig.xml"),
        src.mybatis_annotation.then_some("@Mapper / @Select / SqlSession"),
    );

    // ── JdbcDao (dep + ≥1 source hit) ────────────────────────────────────────
    let jdbc_a = build.has_dependency("spring-jdbc")
        || build.has_dependency("commons-dbcp")
        || build.has_dependency("mysql")
        || build.has_dependency("ojdbc")
        || build.has_dependency("postgresql");
    // Per Spike D: JDBC needs the driver/coordinate AND ≥1 source hit.
    activate(
        &mut set.jdbc_dao,
        &mut hits,
        "jdbc_dao",
        (jdbc_a && src.jdbc_usage).then_some("JDBC coordinate + java.sql / JdbcTemplate hit"),
        None,
        src.jdbc_usage.then_some("JdbcTemplate / java.sql / AbstractDAO"),
    );

    // ── Lombok ───────────────────────────────────────────────────────────────
    activate(
        &mut set.lombok,
        &mut hits,
        "lombok",
        build.has_dependency("lombok").then_some("dependency org.projectlombok:lombok"),
        None,
        src.lombok_import.then_some("import lombok.* / @Data / @Getter"),
    );

    // ── BeanValidation ───────────────────────────────────────────────────────
    // The API is what makes constraints mean anything; the engine (`hibernate-validator`) and
    // Spring's starter each drag it in, and a project regularly declares only one of the three.
    let bval_a = ["jakarta.validation-api", "validation-api", "hibernate-validator",
                  "spring-boot-starter-validation"]
        .iter()
        .find(|c| build.has_dependency(c))
        .copied();
    activate(
        &mut set.bean_validation,
        &mut hits,
        "bean_validation",
        bval_a,
        files.validation_signal(),
        src.bean_validation.then_some("import jakarta/javax.validation / @NotNull / @Valid"),
    );

    // ── Scheduling ───────────────────────────────────────────────────────────
    let sched_a = ["quartz", "spring-boot-starter-quartz", "spring-context-support"]
        .iter()
        .find(|c| build.has_dependency(c))
        .copied();
    activate(
        &mut set.scheduling,
        &mut hits,
        "scheduling",
        sched_a,
        None,
        src.scheduled.then_some("@Scheduled / @EnableScheduling in source"),
    );

    // ── Jackson ──────────────────────────────────────────────────────────────
    let jackson_a = ["jackson-databind", "jackson-core", "spring-boot-starter-web"]
        .iter()
        .find(|c| build.has_dependency(c))
        .copied();
    activate(
        &mut set.jackson,
        &mut hits,
        "jackson",
        jackson_a,
        None,
        src.jackson.then_some("com.fasterxml.jackson import / @Json* in source"),
    );

    // ── EntandoJaps ──────────────────────────────────────────────────────────
    let entando_a = build.coordinates.iter().any(|d| {
        d.contains("org.entando") || d.contains("com.agiletec") || d.contains("entando")
    });
    activate(
        &mut set.entando_japs,
        &mut hits,
        "entando_japs",
        entando_a.then_some("Entando/jAPS dependency (org.entando.* / com.agiletec.*)"),
        (files.japs_struts_plugin || files.aps_core_tld)
            .then_some("*japs-struts-plugin.xml / aps-core.tld"),
        src.entando_showlet.then_some("<wp:*> showlet / ControllerServlet"),
    );

    // ── FulcrumI18n ──────────────────────────────────────────────────────────
    //
    // Tier B only, and by **layout** rather than by dependency. The tooling is useful on a project
    // that merely *authors* content — a `.ron` tree with its bundles beside it, the engine nowhere
    // in its own manifest — and the layout is what the engine keys on too: an `i18n/` directory
    // whose `languages.toml` declares what it has been translated into.
    let i18n = find_i18n_root(root);
    activate(
        &mut set.fulcrum_i18n,
        &mut hits,
        "fulcrum_i18n",
        None,
        i18n.as_deref(),
        None,
    );

    // ── Bevy ─────────────────────────────────────────────────────────────────
    //
    // Tier A from the **Cargo** manifest rather than the pom — the first capability whose strong
    // signal is not Maven's, which is what a Rust project's evidence looks like. Corroborated by
    // the source shape so that a workspace member that merely *depends* on a Bevy crate without
    // declaring anything still activates (the tooling is empty there, and an empty panel gated on
    // real evidence is better than a missing one).
    let bevy_dep = find_bevy_dependency(root);
    activate(
        &mut set.bevy,
        &mut hits,
        "bevy",
        bevy_dep.as_deref(),
        None,
        src.bevy_source.then_some("#[derive(Component)] / add_systems in source"),
    );

    set.hits = hits;
    set
}

/// The first Cargo manifest under `root` that declares a `bevy` / `bevy_*` dependency, as
/// `"bevy (path/to/Cargo.toml)"`.
///
/// Read line by line rather than through a TOML parser, deliberately: detection needs *presence*,
/// the shapes a dependency can take are few and all of them start the line with the crate name,
/// and reaching for a parser here would make the capability layer depend on the manifest crate for
/// one boolean. Bounded like every other signal — a workspace is a handful of manifests, and a
/// vendored tree is not worth walking to find out it has none.
fn find_bevy_dependency(root: &Path) -> Option<String> {
    const MAX_MANIFESTS: usize = 80;
    let mut found: Option<String> = None;
    let mut seen = 0usize;
    walk_shallow(root, 5, &mut |path, name| {
        if found.is_some() || name != "Cargo.toml" || seen >= MAX_MANIFESTS {
            return;
        }
        seen += 1;
        let Some(text) = read_head(path, MAX_SOURCE_BYTES) else { return };
        if !manifest_declares_bevy(&text) {
            return;
        }
        let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/");
        found = Some(format!("bevy ({rel})"));
    });
    found
}

/// Whether a manifest's text declares a dependency on Bevy.
///
/// Only inside a dependency table: `bevy` under `[package]` would be the project's own name, and a
/// crate called `bevy-something` of one's own is not a use of the engine.
fn manifest_declares_bevy(text: &str) -> bool {
    let mut in_deps = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let header = header.trim();
            // `[dependencies.bevy]` is a declaration in itself.
            if let Some(name) = header.rsplit('.').next() {
                if is_bevy_crate(name) && header.contains("dependencies") {
                    return true;
                }
            }
            in_deps = header.contains("dependencies");
            continue;
        }
        if !in_deps {
            continue;
        }
        let name = line.split(['=', '.', ' ']).next().unwrap_or("").trim();
        if is_bevy_crate(name) {
            return true;
        }
    }
    false
}

fn is_bevy_crate(name: &str) -> bool {
    let name = name.trim().trim_matches('"');
    name == "bevy" || name.starts_with("bevy_") || name.starts_with("bevy-")
}

/// The first `i18n/languages.toml` under `root`, relative and forward-slashed.
///
/// Bounded like every other signal here: presence is all detection needs, and a project keeps its
/// content under a handful of directories rather than eighty levels down. Returns the path so the
/// capability can *say* what convinced it — a classification with no evidence is one nobody can
/// argue with when it is wrong.
fn find_i18n_root(root: &Path) -> Option<String> {
    let mut found: Option<String> = None;
    walk_shallow(root, 7, &mut |path, name| {
        if found.is_some() || name != "languages.toml" {
            return;
        }
        // The file alone is not the convention — it has to sit in a directory called `i18n`.
        let in_i18n = path
            .parent()
            .and_then(|d| d.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "i18n");
        if !in_i18n {
            return;
        }
        found = Some(
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/"),
        );
    });
    found
}

/// Flip a capability on and record the winning evidence. A strong signal (A or B)
/// activates it; a C-only signal activates it *provisionally* (still on, but the hit
/// records `tier="C"` so the FE can mark it low-confidence). At most one hit per tier
/// is recorded (the first present), to keep the evidence terse.
fn activate(
    flag: &mut bool,
    hits: &mut Vec<CapabilityHit>,
    capability: &str,
    tier_a: Option<&str>,
    tier_b: Option<&str>,
    tier_c: Option<&str>,
) {
    let strong = tier_a.is_some() || tier_b.is_some();
    if let Some(detail) = tier_a {
        hits.push(hit(capability, "A", format!("dependency: {detail}")));
    }
    if let Some(detail) = tier_b {
        hits.push(hit(capability, "B", format!("config: {detail}")));
    }
    if let Some(detail) = tier_c {
        hits.push(hit(capability, "C", format!("source: {detail}")));
    }
    // Activate on any strong signal, or provisionally on a C-only signal.
    *flag = strong || tier_c.is_some();
}

fn hit(capability: &str, tier: &str, detail: String) -> CapabilityHit {
    CapabilityHit { capability: capability.to_string(), tier: tier.to_string(), detail }
}

// ── Tier B: config-file presence ─────────────────────────────────────────────

/// Well-known config files, resolved once against a bounded set of conventional
/// paths (never a full-tree walk — presence is all detection needs).
#[derive(Default)]
struct ConfigFiles {
    struts_xml: bool,
    struts_plugin_xml: bool,
    tiles_xml: bool,
    spring_beans_xml: bool,
    component_scan: bool,
    persistence_xml: bool,
    hbm_xml: bool,
    mapper_xml: bool,
    has_tld: bool,
    japs_struts_plugin: bool,
    aps_core_tld: bool,
    /// `META-INF/validation.xml` — the file that can replace the message interpolator, and
    /// therefore the file that decides which bundle a constraint message is read from.
    validation_xml: bool,
    /// A `ValidationMessages.properties` on a resource root — the bundle the spec names, and the
    /// one whose presence says somebody has written custom constraint messages.
    validation_messages: bool,
}

impl ConfigFiles {
    /// The tier-B evidence for Bean Validation, named so the hit says which file was found.
    fn validation_signal(&self) -> Option<&'static str> {
        if self.validation_xml {
            Some("META-INF/validation.xml")
        } else if self.validation_messages {
            Some("ValidationMessages.properties")
        } else {
            None
        }
    }

    /// Each module's conventional places. A reactor keeps its config in the module that reads it —
    /// the web module's `ValidationMessages.properties`, never the aggregator's.
    fn scan(modules: &[PathBuf]) -> Self {
        let mut f = ConfigFiles::default();
        for root in modules {
            Self::scan_module(&mut f, root);
        }
        f
    }

    fn scan_module(f: &mut Self, root: &Path) {
        // struts.xml lives on the classpath: src/main/resources or WEB-INF/classes.
        let struts_candidates = [
            "src/main/resources/struts.xml",
            "src/main/webapp/WEB-INF/classes/struts.xml",
            "WEB-INF/classes/struts.xml",
        ];
        f.struts_xml |= struts_candidates.iter().any(|p| root.join(p).is_file());

        // A shallow walk of the resources + WEB-INF trees for the *-suffix / by-name
        // config files. Bounded depth + count keeps this cheap.
        let roots = [
            root.join("src/main/resources"),
            root.join("src/main/webapp/WEB-INF"),
            root.join("WEB-INF"),
        ];
        for r in roots.iter().filter(|p| p.is_dir()) {
            walk_shallow(r, 4, &mut |path, name| {
                let lname = name.to_ascii_lowercase();
                if lname.ends_with("-struts-plugin.xml") {
                    f.struts_plugin_xml = true;
                    if lname.contains("japs") {
                        f.japs_struts_plugin = true;
                    }
                }
                if lname == "tiles.xml" || lname.ends_with("-tiles.xml") {
                    f.tiles_xml = true;
                }
                if lname.ends_with(".tld") {
                    f.has_tld = true;
                    if lname == "aps-core.tld" {
                        f.aps_core_tld = true;
                    }
                }
                if lname == "persistence.xml" {
                    f.persistence_xml = true;
                }
                if lname == "validation.xml" {
                    f.validation_xml = true;
                }
                // The bundle the Bean Validation spec names, in every locale it is written in:
                // `ValidationMessages.properties`, `ValidationMessages_it.properties`.
                if lname.starts_with("validationmessages") && lname.ends_with(".properties") {
                    f.validation_messages = true;
                }
                if lname.ends_with(".hbm.xml") {
                    f.hbm_xml = true;
                }
                if lname.ends_with("mapper.xml") || lname == "sqlmapconfig.xml" {
                    f.mapper_xml = true;
                }
                // A Spring beans XML: any *.xml whose head contains `<beans`. Read a
                // small prefix to confirm (avoids flagging unrelated XML).
                if lname.ends_with(".xml") && head_contains(path, "<beans") {
                    f.spring_beans_xml = true;
                    if head_contains(path, "component-scan") {
                        f.component_scan = true;
                    }
                }
            });
        }
    }

    fn first_struts_file(&self) -> &'static str {
        if self.struts_xml {
            "struts.xml"
        } else {
            "*-struts-plugin.xml"
        }
    }
}

// ── Tier C: bounded source-pattern scan ──────────────────────────────────────

/// Source-pattern signals from a bounded sample of `.java` / `.jsp` files.
#[derive(Default)]
struct SourceSignals {
    filter_dispatcher: bool,
    action_annotation: bool,
    taglib_directive: bool,
    ognl_expr: bool,
    tiles_result: bool,
    get_bean: bool,
    spring_stereotype: bool,
    jpa_repository: bool,
    jpa_entity: bool,
    mybatis_annotation: bool,
    jdbc_usage: bool,
    lombok_import: bool,
    /// A source declares a constraint or asks for one to be checked — the corroborating half of
    /// the Bean Validation signal.
    bean_validation: bool,
    /// A source schedules work, or switches scheduling on.
    scheduled: bool,
    /// A source annotates something for Jackson, or imports it.
    jackson: bool,
    entando_showlet: bool,
    /// A Rust source declares an ECS item or registers a system — the corroborating half of the
    /// Bevy signal.
    bevy_source: bool,
    /// A JSP view exists at all. Recorded from the file NAME, before the read budget is
    /// consulted, so a project whose pages sit past the scan cap is still known to have them.
    has_jsp: bool,
}

impl SourceSignals {
    /// One read budget for the whole reactor, spent module by module.
    fn scan(modules: &[PathBuf]) -> Self {
        let mut s = SourceSignals::default();
        let mut scanned = 0usize;
        for root in modules {
            let src_root = root.join("src");
            let scan_root = if src_root.is_dir() {
                src_root
            } else if modules.len() == 1 {
                // Not a Maven layout at all — the sources are wherever they are.
                root.clone()
            } else {
                // A reactor's aggregator, with no sources of its own. Walking it would walk every
                // module a second time and spend the budget on whichever the directory lists first.
                continue;
            };
            Self::scan_module(&mut s, &mut scanned, root, &scan_root);
        }
        s
    }

    fn scan_module(s: &mut Self, scanned: &mut usize, root: &Path, scan_root: &Path) {
        walk_shallow(scan_root, SOURCE_WALK_DEPTH, &mut |path, name| {
            let lname = name.to_ascii_lowercase();
            let is_java = lname.ends_with(".java");
            let is_jsp = lname.ends_with(".jsp") || lname.ends_with(".tag");
            let is_rust = lname.ends_with(".rs");
            // Presence is decided by the name alone — no read, no budget. The cap below limits
            // how many files we OPEN, and a project's pages must not become invisible just
            // because they are deep in the walk.
            s.has_jsp |= is_jsp || lname.ends_with(".jspf") || lname.ends_with(".tagx");
            if *scanned >= MAX_SOURCE_FILES {
                return;
            }
            if !is_java && !is_jsp && !is_rust {
                return;
            }
            *scanned += 1;
            let Some(text) = read_head(path, MAX_SOURCE_BYTES) else { return };
            if is_java {
                s.filter_dispatcher |= text.contains("FilterDispatcher");
                s.action_annotation |= text.contains("@Action") || text.contains("@Namespace");
                s.get_bean |= text.contains(".getBean(") || text.contains("ContextLoaderListener");
                s.spring_stereotype |= text.contains("@Service")
                    || text.contains("@Component")
                    || text.contains("@Autowired");
                s.jpa_repository |=
                    text.contains("JpaRepository") || text.contains("CrudRepository");
                s.jpa_entity |= text.contains("@Entity") || text.contains("EntityManager");
                s.mybatis_annotation |= text.contains("@Mapper") || text.contains("SqlSession");
                s.jdbc_usage |= text.contains("JdbcTemplate")
                    || text.contains("import java.sql.")
                    || text.contains("AbstractDAO");
                // `@Valid` and `@NotNull` are the two that appear in essentially every project
                // that validates anything, and the imports are what tell a constraint from a
                // same-named annotation out of some other library.
                s.bean_validation |= text.contains("import jakarta.validation.")
                    || text.contains("import javax.validation.")
                    || text.contains("@Valid")
                    || text.contains("@NotNull");
                s.scheduled |= text.contains("@Scheduled")
                    || text.contains("@EnableScheduling")
                    || text.contains("org.quartz");
                s.jackson |= text.contains("com.fasterxml.jackson")
                    || text.contains("@JsonProperty")
                    || text.contains("@JsonIgnore")
                    || text.contains("ObjectMapper");
                s.lombok_import |= text.contains("import lombok.")
                    || text.contains("@Data")
                    || text.contains("@Getter");
            }
            if is_rust {
                s.bevy_source |= text.contains("#[derive(Component)]")
                    || text.contains("add_systems(")
                    || text.contains("bevy::prelude");
            }
            if is_jsp {
                s.taglib_directive |= text.contains("<%@ taglib") || text.contains("<%@taglib");
                s.ognl_expr |= text.contains("%{") || text.contains("${");
                s.entando_showlet |= text.contains("<wp:");
            }
            // These appear in either kind.
            s.tiles_result |= text.contains("type=\"tiles\"");
        });

        // The walk above starts at `src/` when it exists, and plenty of legacy layouts keep
        // their pages somewhere else entirely (`web/`, `WebContent/`). Those directories are
        // checked for pages only — no reads, so this costs a directory listing.
        if !s.has_jsp {
            for dir in ["web", "webapp", "WebContent", "src/main/webapp"] {
                let candidate = root.join(dir);
                if !candidate.is_dir() {
                    continue;
                }
                walk_shallow(&candidate, 6, &mut |_p, name| {
                    let l = name.to_ascii_lowercase();
                    s.has_jsp |= l.ends_with(".jsp")
                        || l.ends_with(".jspf")
                        || l.ends_with(".tag")
                        || l.ends_with(".tagx");
                });
                if s.has_jsp {
                    break;
                }
            }
        }
    }
}

// ── tiny filesystem helpers ──────────────────────────────────────────────────

/// Depth-bounded directory walk. Calls `f(path, file_name)` for every *file*
/// encountered up to `max_depth` levels below `dir`. Silently skips unreadable dirs.
fn walk_shallow(dir: &Path, max_depth: usize, f: &mut dyn FnMut(&Path, &str)) {
    if max_depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Skip the noisy heavy dirs — never relevant to detection.
            let name = entry.file_name();
            let n = name.to_string_lossy();
            if n == "target" || n == ".git" || n == "node_modules" {
                continue;
            }
            walk_shallow(&path, max_depth - 1, f);
        } else if ft.is_file() {
            let name = entry.file_name();
            f(&path, &name.to_string_lossy());
        }
    }
}

/// Read at most `max` bytes of `path` as lossy UTF-8. `None` on an I/O error.
fn read_head(path: &Path, max: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; max];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Whether the first 8 KiB of `path` contains `needle`.
fn head_contains(path: &Path, needle: &str) -> bool {
    read_head(path, 8 * 1024).map(|t| t.contains(needle)).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pom;

    // A Struts / Entando-flavoured pom → the Struts / Spring-XML / JDBC / Tiles
    // capabilities on; MyBatis / JPA / Spring-Data / Lombok OFF (docs §10 validates
    // exactly this profile).
    const STRUTS_POM: &str = r#"
      <project>
        <artifactId>gestionale-web</artifactId>
        <name>Gestionale Web</name>
        <dependencies>
          <dependency><groupId>org.apache.struts</groupId><artifactId>struts2-core</artifactId></dependency>
          <dependency><groupId>org.apache.struts</groupId><artifactId>struts2-tiles-plugin</artifactId></dependency>
          <dependency><groupId>org.springframework</groupId><artifactId>spring-jdbc</artifactId></dependency>
          <dependency><groupId>org.springframework</groupId><artifactId>spring-beans</artifactId></dependency>
          <dependency><groupId>org.entando.entando</groupId><artifactId>entando-core</artifactId></dependency>
        </dependencies>
      </project>
    "#;

    // A MyBatis pom → MyBatis on, Struts / Tiles / Entando OFF.
    const MYBATIS_POM: &str = r#"
      <project>
        <artifactId>orders-service</artifactId>
        <name>Orders</name>
        <dependencies>
          <dependency><groupId>org.mybatis</groupId><artifactId>mybatis</artifactId></dependency>
          <dependency><groupId>org.mybatis</groupId><artifactId>mybatis-spring</artifactId></dependency>
          <dependency><groupId>org.springframework</groupId><artifactId>spring-context</artifactId></dependency>
        </dependencies>
      </project>
    "#;

    #[test]
    fn classifies_struts_entando_pom() {
        // No filesystem evidence — dependency (tier-A) alone must classify. Use a
        // path that doesn't exist so only pom signals fire.
        let root = Path::new("C:/nonexistent-bennu-test-root");
        let pom = pom::parse(STRUTS_POM);
        let caps = detect(root, &pom);

        assert!(caps.struts_xml_config, "struts2-core dep → StrutsXmlConfig");
        assert!(caps.tiles_views, "struts2-tiles-plugin dep → TilesViews");
        assert!(caps.spring_xml_di, "spring-jdbc/spring-beans dep → SpringXmlDi");
        assert!(caps.entando_japs, "entando dep → EntandoJaps");

        // Provably OFF for this stack (docs §10).
        assert!(!caps.mybatis_mapper, "no mybatis dep → MyBatis OFF");
        assert!(!caps.jpa_hibernate, "no hibernate dep → JPA OFF");
        assert!(!caps.spring_data_repo, "no spring-data dep → Spring-Data OFF");
        assert!(!caps.lombok, "no lombok dep → Lombok OFF");

        // The Struts activation must be evidenced by a strong (A) hit.
        assert!(caps
            .hits
            .iter()
            .any(|h| h.capability == "struts_xml_config" && h.tier == "A"));
    }

    /// The API arrives transitively — through Spring's starter, or a company parent — and is written
    /// in no pom at all. The resolved tree is where it is.
    #[test]
    fn a_dependency_only_the_resolved_tree_has_still_counts() {
        let root = Path::new("C:/nonexistent-bennu-test-root");
        let pom = pom::parse(MYBATIS_POM);
        assert!(!detect(root, &pom).bean_validation);

        let build = BuildEvidence::read(root, &pom)
            .with_resolved(["org.hibernate.validator:hibernate-validator".to_string()]);
        let caps = detect_with(&build);
        assert!(caps.bean_validation);
        assert!(caps.hits.iter().any(|h| h.capability == "bean_validation" && h.tier == "A"));
    }

    /// A reactor's root pom lists modules and declares nothing; the dependencies are in the modules.
    #[test]
    fn a_dependency_declared_in_a_module_counts() {
        let root = std::env::temp_dir().join(format!("bennu-caps-reactor-{}", std::process::id()));
        let web = root.join("web");
        std::fs::create_dir_all(&web).unwrap();
        let root_pom = "<project><artifactId>reactor</artifactId>\
                        <modules><module>web</module></modules></project>";
        std::fs::write(root.join("pom.xml"), root_pom).unwrap();
        std::fs::write(
            web.join("pom.xml"),
            "<project><artifactId>web</artifactId><dependencies><dependency>\
             <groupId>org.hibernate.validator</groupId><artifactId>hibernate-validator</artifactId>\
             </dependency></dependencies></project>",
        )
        .unwrap();

        let caps = detect(&root, &pom::parse(root_pom));
        let _ = std::fs::remove_dir_all(&root);
        assert!(caps.bean_validation, "hits: {:?}", caps.hits);
    }

    #[test]
    fn classifies_mybatis_pom() {
        let root = Path::new("C:/nonexistent-bennu-test-root");
        let pom = pom::parse(MYBATIS_POM);
        let caps = detect(root, &pom);

        assert!(caps.mybatis_mapper, "mybatis dep → MyBatisMapper");
        // Struts/Tiles/Entando must NOT trip on a MyBatis project.
        assert!(!caps.struts_xml_config, "no struts dep → StrutsXmlConfig OFF");
        assert!(!caps.tiles_views, "no tiles dep → TilesViews OFF");
        assert!(!caps.entando_japs, "no entando dep → EntandoJaps OFF");
    }
}
