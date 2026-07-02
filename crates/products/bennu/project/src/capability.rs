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

use std::path::Path;

use bennu_proto::prelude::{CapabilityHit, CapabilitySet};

use crate::pom::Pom;

/// Max number of source files scanned for tier-C evidence, and max bytes read per
/// file. Detection only needs presence, so a bounded scan keeps `open_project`
/// responsive on a 1200-file legacy tree.
const MAX_SOURCE_FILES: usize = 400;
const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Detect the domain capabilities of the project rooted at `root`, given its parsed
/// root [`Pom`]. Pure over (pom + filesystem); never fails (an unreadable file is
/// simply absent evidence).
pub fn detect(root: &Path, pom: &Pom) -> CapabilitySet {
    let mut set = CapabilitySet::default();
    let mut hits: Vec<CapabilityHit> = Vec::new();

    // Tier B: gather the well-known config-file signals once.
    let files = ConfigFiles::scan(root);
    // Tier C: gather a bounded sample of source-pattern signals once.
    let src = SourceSignals::scan(root);

    // ── StrutsXmlConfig ──────────────────────────────────────────────────────
    let struts_a = pom.has_dependency("struts2-core");
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
    let conv_a = pom.has_dependency("struts2-convention-plugin");
    activate(
        &mut set.struts_convention,
        &mut hits,
        "struts_convention",
        conv_a.then_some("dependency struts2-convention-plugin"),
        None,
        src.action_annotation.then_some("@Action / @Namespace in source"),
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
    let tiles_a = pom.has_dependency("struts2-tiles-plugin") || pom.has_dependency("tiles-");
    activate(
        &mut set.tiles_views,
        &mut hits,
        "tiles_views",
        tiles_a.then_some("dependency struts2-tiles-plugin / tiles-*"),
        files.tiles_xml.then_some("tiles.xml"),
        src.tiles_result.then_some("result type=\"tiles\""),
    );

    // ── SpringXmlDi ──────────────────────────────────────────────────────────
    let spring_a = pom.has_dependency("spring-beans")
        || pom.has_dependency("spring-context")
        || pom.has_dependency("spring-jdbc");
    activate(
        &mut set.spring_xml_di,
        &mut hits,
        "spring_xml_di",
        spring_a.then_some("dependency spring-beans / spring-context / spring-jdbc"),
        files.spring_beans_xml.then_some("root XML <beans>"),
        src.get_bean.then_some("getBean(...) / ContextLoaderListener"),
    );

    // ── SpringAnnotationDi ───────────────────────────────────────────────────
    let spring_ann_a = pom.has_dependency("spring-context");
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
        pom.has_dependency("spring-data-").then_some("dependency spring-data-*"),
        None,
        src.jpa_repository.then_some("extends JpaRepository / CrudRepository"),
    );

    // ── JpaHibernate ─────────────────────────────────────────────────────────
    activate(
        &mut set.jpa_hibernate,
        &mut hits,
        "jpa_hibernate",
        pom.has_dependency("hibernate-core").then_some("dependency hibernate-core"),
        (files.persistence_xml || files.hbm_xml).then_some("persistence.xml / *.hbm.xml"),
        src.jpa_entity.then_some("@Entity / @Table / EntityManager"),
    );

    // ── MyBatisMapper ────────────────────────────────────────────────────────
    let mybatis_a = pom.has_dependency("mybatis");
    activate(
        &mut set.mybatis_mapper,
        &mut hits,
        "mybatis_mapper",
        mybatis_a.then_some("dependency mybatis / mybatis-spring"),
        files.mapper_xml.then_some("*Mapper.xml / sqlMapConfig.xml"),
        src.mybatis_annotation.then_some("@Mapper / @Select / SqlSession"),
    );

    // ── JdbcDao (dep + ≥1 source hit) ────────────────────────────────────────
    let jdbc_a = pom.has_dependency("spring-jdbc")
        || pom.has_dependency("commons-dbcp")
        || pom.has_dependency("mysql")
        || pom.has_dependency("ojdbc")
        || pom.has_dependency("postgresql");
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
        pom.has_dependency("lombok").then_some("dependency org.projectlombok:lombok"),
        None,
        src.lombok_import.then_some("import lombok.* / @Data / @Getter"),
    );

    // ── EntandoJaps ──────────────────────────────────────────────────────────
    let entando_a = pom.dependencies.iter().any(|d| {
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

    set.hits = hits;
    set
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
}

impl ConfigFiles {
    fn scan(root: &Path) -> Self {
        let mut f = ConfigFiles::default();
        // struts.xml lives on the classpath: src/main/resources or WEB-INF/classes.
        let struts_candidates = [
            "src/main/resources/struts.xml",
            "src/main/webapp/WEB-INF/classes/struts.xml",
            "WEB-INF/classes/struts.xml",
        ];
        f.struts_xml = struts_candidates.iter().any(|p| root.join(p).is_file());

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
        f
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
    entando_showlet: bool,
}

impl SourceSignals {
    fn scan(root: &Path) -> Self {
        let mut s = SourceSignals::default();
        let mut scanned = 0usize;
        let src_root = root.join("src");
        let scan_root = if src_root.is_dir() { src_root } else { root.to_path_buf() };
        walk_shallow(&scan_root, 8, &mut |path, name| {
            if scanned >= MAX_SOURCE_FILES {
                return;
            }
            let lname = name.to_ascii_lowercase();
            let is_java = lname.ends_with(".java");
            let is_jsp = lname.ends_with(".jsp") || lname.ends_with(".tag");
            if !is_java && !is_jsp {
                return;
            }
            scanned += 1;
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
                s.lombok_import |= text.contains("import lombok.")
                    || text.contains("@Data")
                    || text.contains("@Getter");
            }
            if is_jsp {
                s.taglib_directive |= text.contains("<%@ taglib") || text.contains("<%@taglib");
                s.ognl_expr |= text.contains("%{") || text.contains("${");
                s.entando_showlet |= text.contains("<wp:");
            }
            // These appear in either kind.
            s.tiles_result |= text.contains("type=\"tiles\"");
        });
        s
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

    // A Struts / Entando-flavoured pom (like the reference PortaleAppalti) → the
    // Struts / Spring-XML / JDBC / Tiles capabilities on; MyBatis / JPA / Spring-Data
    // / Lombok OFF (docs §10 validates exactly this profile).
    const STRUTS_POM: &str = r#"
      <project>
        <artifactId>portale-appalti</artifactId>
        <name>Portale Appalti</name>
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
