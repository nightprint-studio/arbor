//! Which jar on the classpath is the JDBC driver a data source names.
//!
//! A `<Resource>` is built by Tomcat itself, from Tomcat's own class loader, so the driver has to
//! be in the base's `lib/` — a copy in the application's `WEB-INF/lib` is not where it looks first.
//! The project already depends on the driver (it would not run otherwise); this finds that jar by
//! its file name, because the classpath is a list of paths and nothing more.

/// Jar file-name prefixes that carry a known driver class. Matched case-insensitively against the
/// start of the file name.
const KNOWN: &[(&str, &[&str])] = &[
    ("org.postgresql.Driver", &["postgresql-"]),
    ("com.mysql.cj.jdbc.Driver", &["mysql-connector-"]),
    ("com.mysql.jdbc.Driver", &["mysql-connector-"]),
    ("org.mariadb.jdbc.Driver", &["mariadb-java-client-"]),
    ("oracle.jdbc.OracleDriver", &["ojdbc", "oracle-jdbc"]),
    ("oracle.jdbc.driver.OracleDriver", &["ojdbc", "oracle-jdbc"]),
    ("com.ibm.db2.jcc.DB2Driver", &["db2jcc", "jcc-"]),
    ("com.microsoft.sqlserver.jdbc.SQLServerDriver", &["mssql-jdbc"]),
    ("org.h2.Driver", &["h2-"]),
    ("org.sqlite.JDBC", &["sqlite-jdbc"]),
    ("org.hsqldb.jdbc.JDBCDriver", &["hsqldb"]),
    ("org.apache.derby.jdbc.EmbeddedDriver", &["derby-"]),
];

/// Drivers that can be fetched from Maven Central, as `(driver class, group, artifact)`.
///
/// Only the ones whose newest release runs on the Java a Tomcat 8.5 / 9 does (8): H2 2.x and
/// HSQLDB 2.7 need 11, SQL Server publishes one version per JRE, Derby's newest needs 21 — and
/// Oracle and DB2 are not on Central under a free licence at all. Those are chosen by hand.
const CENTRAL: &[(&str, &str, &str)] = &[
    ("org.postgresql.Driver", "org.postgresql", "postgresql"),
    ("com.mysql.cj.jdbc.Driver", "com.mysql", "mysql-connector-j"),
    ("com.mysql.jdbc.Driver", "mysql", "mysql-connector-java"),
    ("org.mariadb.jdbc.Driver", "org.mariadb.jdbc", "mariadb-java-client"),
    ("org.sqlite.JDBC", "org.xerial", "sqlite-jdbc"),
];

/// Where a driver's artifact lives on Maven Central.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CentralDriver {
    pub group: &'static str,
    pub artifact: &'static str,
}

/// The Central artifact for `driver`; `None` for one that has to be chosen by hand.
pub fn central_driver(driver: &str) -> Option<CentralDriver> {
    CENTRAL.iter().find(|(d, _, _)| *d == driver).map(|(_, group, artifact)| CentralDriver { group, artifact })
}

const CENTRAL_BASE: &str = "https://repo1.maven.org/maven2";

impl CentralDriver {
    fn directory(&self) -> String {
        format!("{CENTRAL_BASE}/{}/{}", self.group.replace('.', "/"), self.artifact)
    }

    /// The artifact's `maven-metadata.xml`, which names its newest release.
    pub fn metadata_url(&self) -> String {
        format!("{}/maven-metadata.xml", self.directory())
    }

    /// The jar of `version`; `None` when the version holds anything a URL path should not.
    pub fn jar_url(&self, version: &str) -> Option<String> {
        safe_version(version).then(|| format!("{0}/{1}/{2}-{1}.jar", self.directory(), version, self.artifact))
    }

    /// The file name a downloaded jar is kept under.
    pub fn file_name(&self, version: &str) -> String {
        format!("{}-{version}.jar", self.artifact)
    }
}

fn safe_version(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// The newest release in a `maven-metadata.xml`: `<release>`, else the last `<version>` listed.
pub fn parse_release(xml: &str) -> Option<String> {
    let between = |open: &str, close: &str, from_end: bool| {
        let value = if from_end { xml.rfind(open) } else { xml.find(open) }?;
        let rest = &xml[value + open.len()..];
        let end = rest.find(close)?;
        Some(rest[..end].trim().to_string())
    };
    between("<release>", "</release>", false)
        .or_else(|| between("<version>", "</version>", true))
        .filter(|v| safe_version(v))
}

/// The file-name prefixes that identify `driver`'s jar; empty for a driver this does not know.
pub fn driver_jar_names(driver: &str) -> &'static [&'static str] {
    KNOWN.iter().find(|(d, _)| *d == driver).map(|(_, p)| *p).unwrap_or(&[])
}

fn file_name(entry: &str) -> &str {
    entry.rsplit(['/', '\\']).next().unwrap_or(entry)
}

/// The classpath entries that are the jars of `drivers`, each once, in classpath order.
pub fn pick_driver_jars(classpath: &[String], drivers: &[&str]) -> Vec<String> {
    let prefixes: Vec<&str> = drivers.iter().flat_map(|d| driver_jar_names(d).iter().copied()).collect();
    let mut out: Vec<String> = Vec::new();
    for entry in classpath {
        let name = file_name(entry).to_ascii_lowercase();
        if name.ends_with(".jar") && prefixes.iter().any(|p| name.starts_with(p)) && !out.contains(entry) {
            out.push(entry.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_driver_jar_is_found_by_name_whatever_the_separator() {
        let classpath = cp(&[
            "C:\\m2\\org\\postgresql\\postgresql\\42.7.3\\postgresql-42.7.3.jar",
            "/m2/commons-lang3-3.14.jar",
            "/m2/org/postgresql/postgresql/42.7.3/postgresql-42.7.3.jar",
        ]);
        let picked = pick_driver_jars(&classpath, &["org.postgresql.Driver"]);
        assert_eq!(picked.len(), 2, "two different paths are two different entries: {picked:?}");
        assert!(picked.iter().all(|p| p.contains("postgresql-42.7.3.jar")));
    }

    #[test]
    fn an_unknown_driver_or_a_directory_picks_nothing() {
        let classpath = cp(&["/proj/target/classes", "/m2/postgresql-42.jar"]);
        assert!(pick_driver_jars(&classpath, &["com.example.Weird"]).is_empty());
        assert!(pick_driver_jars(&classpath, &["org.postgresql.Driver"]).len() == 1);
        assert!(driver_jar_names("com.example.Weird").is_empty());
    }

    #[test]
    fn a_central_driver_has_its_urls_and_a_file_name() {
        let pg = central_driver("org.postgresql.Driver").unwrap();
        assert_eq!(pg.metadata_url(), "https://repo1.maven.org/maven2/org/postgresql/postgresql/maven-metadata.xml");
        assert_eq!(
            pg.jar_url("42.7.4").unwrap(),
            "https://repo1.maven.org/maven2/org/postgresql/postgresql/42.7.4/postgresql-42.7.4.jar"
        );
        assert_eq!(pg.file_name("42.7.4"), "postgresql-42.7.4.jar");
        assert!(central_driver("oracle.jdbc.OracleDriver").is_none());
    }

    #[test]
    fn a_version_that_could_leave_the_path_is_refused() {
        let pg = central_driver("org.postgresql.Driver").unwrap();
        for bad in ["", "../x", "1.0/../..", "1 0", "a?b"] {
            assert!(pg.jar_url(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn the_release_is_read_from_the_metadata() {
        let xml = "<metadata><versioning><latest>42.7.4</latest><release>42.7.4</release>\
                   <versions><version>42.7.3</version><version>42.7.4</version></versions></versioning></metadata>";
        assert_eq!(parse_release(xml).as_deref(), Some("42.7.4"));
        let no_release = "<versions><version>1.0</version><version>1.1</version></versions>";
        assert_eq!(parse_release(no_release).as_deref(), Some("1.1"));
        assert_eq!(parse_release("<metadata/>"), None);
        assert_eq!(parse_release("<release>../evil</release>"), None);
    }

    #[test]
    fn two_drivers_are_both_picked() {
        let classpath = cp(&["/m2/ojdbc8-21.jar", "/m2/db2jcc4.jar", "/m2/unrelated.jar"]);
        let picked = pick_driver_jars(&classpath, &["oracle.jdbc.OracleDriver", "com.ibm.db2.jcc.DB2Driver"]);
        assert_eq!(picked, ["/m2/ojdbc8-21.jar", "/m2/db2jcc4.jar"]);
    }
}
