//! The coordinates every Java project reaches for, so completion works on an empty machine.
//!
//! ## Why a table at all, when the repository is right there
//!
//! [`crate::catalog`] answers from what the machine has downloaded, which is the better answer
//! whenever there is one: it is exact, it carries the versions, and it grows by itself. But it can
//! only ever offer what you already have — and the moment you actually need help typing a
//! coordinate is the moment you are adding something you have *never* had. On a fresh clone, a new
//! laptop, or a project that has not been built yet, an autocomplete backed only by `~/.m2` is
//! empty exactly when it is needed.
//!
//! So the two are merged and labelled: a coordinate from the repository is offered with its
//! installed versions, one from this table is offered without and marked as not installed. Nothing
//! here is a version — a hard-coded version list is wrong within a month, and a wrong version is
//! worse than no suggestion. The table carries identity and a line of prose, which do not rot.
//!
//! ## What is in it
//!
//! The libraries a Java project in this codebase's world actually declares: the Spring and Jakarta
//! platforms, the legacy Struts/Tiles/OGNL stack, persistence, the Apache Commons family, logging,
//! JSON, testing, and the build plugins a pom names by coordinate. Roughly the set that would
//! otherwise be typed from memory.

/// `(groupId, artifactId, what it is)`.
pub type Known = (&'static str, &'static str, &'static str);

/// Library coordinates, offered wherever a `<dependency>` is being written.
pub const LIBRARIES: &[Known] = &[
    // ── Spring ───────────────────────────────────────────────────────────────
    ("org.springframework", "spring-core", "Spring core utilities and type conversion"),
    ("org.springframework", "spring-beans", "Spring bean factory and dependency injection"),
    ("org.springframework", "spring-context", "Spring application context"),
    ("org.springframework", "spring-web", "Spring web support (RestTemplate, filters)"),
    ("org.springframework", "spring-webmvc", "Spring MVC — controllers, view resolution"),
    ("org.springframework", "spring-jdbc", "Spring JDBC — JdbcTemplate, DataSource support"),
    ("org.springframework", "spring-orm", "Spring ORM integration (Hibernate, JPA)"),
    ("org.springframework", "spring-tx", "Spring transaction management"),
    ("org.springframework", "spring-aop", "Spring AOP"),
    ("org.springframework", "spring-test", "Spring test context framework"),
    ("org.springframework.boot", "spring-boot-starter", "Spring Boot core starter"),
    ("org.springframework.boot", "spring-boot-starter-web", "Spring Boot web + embedded Tomcat"),
    ("org.springframework.boot", "spring-boot-starter-data-jpa", "Spring Boot JPA + Hibernate"),
    ("org.springframework.boot", "spring-boot-starter-security", "Spring Boot Security"),
    ("org.springframework.boot", "spring-boot-starter-test", "Spring Boot test starter"),
    ("org.springframework.boot", "spring-boot-starter-actuator", "Spring Boot production endpoints"),
    ("org.springframework.boot", "spring-boot-starter-validation", "Bean Validation via Hibernate Validator"),
    ("org.springframework.boot", "spring-boot-maven-plugin", "Spring Boot repackaging plugin"),
    ("org.springframework.boot", "spring-boot-dependencies", "Spring Boot BOM"),
    ("org.springframework.data", "spring-data-jpa", "Spring Data JPA repositories"),
    ("org.springframework.security", "spring-security-core", "Spring Security core"),
    ("org.springframework.security", "spring-security-web", "Spring Security web filters"),
    ("org.springframework.security", "spring-security-config", "Spring Security configuration"),

    // ── Struts / the legacy web stack ────────────────────────────────────────
    ("org.apache.struts", "struts2-core", "Struts 2 action framework"),
    ("org.apache.struts", "struts2-spring-plugin", "Struts 2 ↔ Spring integration"),
    ("org.apache.struts", "struts2-tiles-plugin", "Struts 2 Tiles view composition"),
    ("org.apache.struts", "struts2-json-plugin", "Struts 2 JSON results"),
    ("org.apache.struts", "struts2-convention-plugin", "Struts 2 annotation-driven routing"),
    ("org.apache.tiles", "tiles-core", "Apache Tiles view composition"),
    ("org.apache.tiles", "tiles-jsp", "Apache Tiles JSP tags"),
    ("ognl", "ognl", "OGNL expression language"),
    ("opensymphony", "xwork", "XWork command framework (Struts 2 core)"),
    ("org.apache.velocity", "velocity-engine-core", "Velocity template engine"),
    ("org.freemarker", "freemarker", "FreeMarker template engine"),

    // ── Servlet / Jakarta / Java EE ──────────────────────────────────────────
    ("javax.servlet", "javax.servlet-api", "Servlet API (Java EE)"),
    ("javax.servlet", "jstl", "JSTL tag library"),
    ("javax.servlet.jsp", "javax.servlet.jsp-api", "JSP API"),
    ("jakarta.servlet", "jakarta.servlet-api", "Servlet API (Jakarta EE)"),
    ("javax.annotation", "javax.annotation-api", "Common annotations (@PostConstruct…)"),
    ("jakarta.annotation", "jakarta.annotation-api", "Common annotations (Jakarta EE)"),
    ("javax.validation", "validation-api", "Bean Validation API"),
    ("jakarta.validation", "jakarta.validation-api", "Bean Validation API (Jakarta EE)"),
    ("javax.persistence", "javax.persistence-api", "JPA API"),
    ("jakarta.persistence", "jakarta.persistence-api", "JPA API (Jakarta EE)"),
    ("javax.xml.bind", "jaxb-api", "JAXB API — needed explicitly on JDK 11+"),
    ("javax.mail", "mail", "JavaMail"),
    ("taglibs", "standard", "Jakarta Taglibs standard implementation"),

    // ── Persistence ──────────────────────────────────────────────────────────
    ("org.hibernate", "hibernate-core", "Hibernate ORM"),
    ("org.hibernate", "hibernate-entitymanager", "Hibernate JPA provider (pre-5.2)"),
    ("org.hibernate.validator", "hibernate-validator", "Bean Validation implementation"),
    ("org.mybatis", "mybatis", "MyBatis SQL mapper"),
    ("org.mybatis", "mybatis-spring", "MyBatis ↔ Spring integration"),
    ("com.zaxxer", "HikariCP", "HikariCP connection pool"),
    ("org.apache.commons", "commons-dbcp2", "Commons DBCP connection pool"),
    ("com.oracle.database.jdbc", "ojdbc8", "Oracle JDBC driver"),
    ("org.postgresql", "postgresql", "PostgreSQL JDBC driver"),
    ("com.mysql", "mysql-connector-j", "MySQL JDBC driver"),
    ("com.microsoft.sqlserver", "mssql-jdbc", "SQL Server JDBC driver"),
    ("com.h2database", "h2", "H2 embedded database"),
    ("org.flywaydb", "flyway-core", "Flyway database migrations"),
    ("org.liquibase", "liquibase-core", "Liquibase database migrations"),

    // ── Apache Commons & friends ─────────────────────────────────────────────
    ("org.apache.commons", "commons-lang3", "String, object and reflection utilities"),
    ("org.apache.commons", "commons-collections4", "Extra collection types"),
    ("org.apache.commons", "commons-text", "String similarity, escaping, substitution"),
    ("org.apache.commons", "commons-csv", "CSV reading and writing"),
    ("org.apache.commons", "commons-pool2", "Object pooling"),
    ("commons-io", "commons-io", "File and stream utilities"),
    ("commons-codec", "commons-codec", "Base64, hex, digests"),
    ("commons-beanutils", "commons-beanutils", "Bean property access"),
    ("commons-fileupload", "commons-fileupload", "Multipart file upload"),
    ("commons-logging", "commons-logging", "Logging facade (JCL)"),
    ("commons-digester", "commons-digester", "XML → object rules engine"),
    ("com.google.guava", "guava", "Google core libraries"),

    // ── Logging ──────────────────────────────────────────────────────────────
    ("org.slf4j", "slf4j-api", "SLF4J logging facade"),
    ("ch.qos.logback", "logback-classic", "Logback logging implementation"),
    ("org.apache.logging.log4j", "log4j-api", "Log4j 2 API"),
    ("org.apache.logging.log4j", "log4j-core", "Log4j 2 implementation"),
    ("org.apache.logging.log4j", "log4j-slf4j-impl", "Log4j 2 binding for SLF4J"),
    ("log4j", "log4j", "Log4j 1.x (end of life)"),

    // ── JSON / XML / HTTP ────────────────────────────────────────────────────
    ("com.fasterxml.jackson.core", "jackson-databind", "Jackson object mapping"),
    ("com.fasterxml.jackson.core", "jackson-core", "Jackson streaming API"),
    ("com.fasterxml.jackson.core", "jackson-annotations", "Jackson annotations"),
    ("com.fasterxml.jackson.datatype", "jackson-datatype-jsr310", "Jackson java.time support"),
    ("com.google.code.gson", "gson", "Gson JSON library"),
    ("org.json", "json", "Reference JSON implementation"),
    ("org.apache.httpcomponents", "httpclient", "Apache HttpClient 4"),
    ("org.apache.httpcomponents.client5", "httpclient5", "Apache HttpClient 5"),
    ("com.squareup.okhttp3", "okhttp", "OkHttp client"),
    ("org.dom4j", "dom4j", "DOM4J XML"),
    ("xerces", "xercesImpl", "Xerces XML parser"),
    ("org.jsoup", "jsoup", "HTML parsing and cleaning"),

    // ── Testing ──────────────────────────────────────────────────────────────
    ("junit", "junit", "JUnit 4"),
    ("org.junit.jupiter", "junit-jupiter", "JUnit 5 (aggregate)"),
    ("org.junit.jupiter", "junit-jupiter-api", "JUnit 5 API"),
    ("org.junit.jupiter", "junit-jupiter-engine", "JUnit 5 engine"),
    ("org.mockito", "mockito-core", "Mockito mocking"),
    ("org.mockito", "mockito-junit-jupiter", "Mockito ↔ JUnit 5"),
    ("org.assertj", "assertj-core", "AssertJ fluent assertions"),
    ("org.hamcrest", "hamcrest", "Hamcrest matchers"),
    ("org.testcontainers", "testcontainers", "Throwaway containers for tests"),
    ("io.rest-assured", "rest-assured", "REST API testing DSL"),

    // ── Misc ─────────────────────────────────────────────────────────────────
    ("org.projectlombok", "lombok", "Lombok annotation processor"),
    ("org.mapstruct", "mapstruct", "Compile-time bean mapping"),
    ("io.swagger.core.v3", "swagger-annotations", "OpenAPI annotations"),
    ("org.springdoc", "springdoc-openapi-starter-webmvc-ui", "OpenAPI UI for Spring MVC"),
    ("org.apache.poi", "poi-ooxml", "Excel (xlsx) reading and writing"),
    ("com.itextpdf", "itextpdf", "PDF generation"),
    ("net.sf.jasperreports", "jasperreports", "JasperReports"),
    ("org.quartz-scheduler", "quartz", "Quartz scheduler"),
    ("org.apache.camel", "camel-core", "Apache Camel integration"),
    ("org.apache.kafka", "kafka-clients", "Kafka client"),
    ("org.entando.entando", "entando-core", "Entando platform core"),
];

/// Plugin coordinates, offered inside `<build><plugins>`.
///
/// Kept apart from [`LIBRARIES`] because the two are never both right: a `<plugin>` block wants
/// `maven-compiler-plugin` and never `spring-core`, and mixing them halves the value of both lists.
pub const PLUGINS: &[Known] = &[
    ("org.apache.maven.plugins", "maven-compiler-plugin", "Compile the sources (source/target level)"),
    ("org.apache.maven.plugins", "maven-surefire-plugin", "Run unit tests"),
    ("org.apache.maven.plugins", "maven-failsafe-plugin", "Run integration tests"),
    ("org.apache.maven.plugins", "maven-war-plugin", "Package a war"),
    ("org.apache.maven.plugins", "maven-jar-plugin", "Package a jar"),
    ("org.apache.maven.plugins", "maven-shade-plugin", "Shade dependencies into one jar"),
    ("org.apache.maven.plugins", "maven-assembly-plugin", "Assemble distributions"),
    ("org.apache.maven.plugins", "maven-dependency-plugin", "Copy, unpack and analyse dependencies"),
    ("org.apache.maven.plugins", "maven-resources-plugin", "Copy and filter resources"),
    ("org.apache.maven.plugins", "maven-install-plugin", "Install into the local repository"),
    ("org.apache.maven.plugins", "maven-deploy-plugin", "Deploy to a remote repository"),
    ("org.apache.maven.plugins", "maven-release-plugin", "Prepare and perform a release"),
    ("org.apache.maven.plugins", "maven-enforcer-plugin", "Enforce build rules (versions, JDK)"),
    ("org.apache.maven.plugins", "maven-source-plugin", "Attach the sources jar"),
    ("org.apache.maven.plugins", "maven-javadoc-plugin", "Attach the javadoc jar"),
    ("org.apache.maven.plugins", "maven-antrun-plugin", "Run Ant tasks from a phase"),
    ("org.apache.maven.plugins", "maven-clean-plugin", "Clean the build directory"),
    ("org.springframework.boot", "spring-boot-maven-plugin", "Spring Boot repackaging and run"),
    ("org.apache.tomcat.maven", "tomcat7-maven-plugin", "Run on an embedded Tomcat 7"),
    ("org.eclipse.jetty", "jetty-maven-plugin", "Run on an embedded Jetty"),
    ("org.jacoco", "jacoco-maven-plugin", "Code coverage"),
    ("org.sonarsource.scanner.maven", "sonar-maven-plugin", "SonarQube analysis"),
    ("org.codehaus.mojo", "build-helper-maven-plugin", "Extra source roots, timestamps, ports"),
    ("org.codehaus.mojo", "exec-maven-plugin", "Run a program or a main class from a phase"),
    ("org.codehaus.mojo", "versions-maven-plugin", "Report and update dependency versions"),
    ("org.codehaus.mojo", "flatten-maven-plugin", "Flatten the pom before install"),
    ("org.apache.rat", "apache-rat-plugin", "License header audit"),
    ("com.diffplug.spotless", "spotless-maven-plugin", "Format and check formatting"),
    ("io.fabric8", "docker-maven-plugin", "Build and run Docker images"),
];

/// The known coordinates for a place in the pom: plugins inside a `<plugin>` block, libraries
/// everywhere else.
pub fn table(in_plugin: bool) -> &'static [Known] {
    if in_plugin {
        PLUGINS
    } else {
        LIBRARIES
    }
}

/// What a known coordinate is, for the hover card and the completion detail.
pub fn describe(group_id: &str, artifact_id: &str) -> Option<&'static str> {
    LIBRARIES
        .iter()
        .chain(PLUGINS.iter())
        .find(|(g, a, _)| *g == group_id && *a == artifact_id)
        .map(|(_, _, doc)| *doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A duplicated row would offer the same coordinate twice in the popup, which reads as a bug.
    #[test]
    fn no_coordinate_is_listed_twice_in_one_table() {
        for table in [LIBRARIES, PLUGINS] {
            let mut seen: Vec<(&str, &str)> = table.iter().map(|(g, a, _)| (*g, *a)).collect();
            let before = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), before, "duplicate coordinate in the table");
        }
    }

    /// Every row has to be usable as written — an empty field would complete to nothing.
    #[test]
    fn every_row_is_complete() {
        for (g, a, doc) in LIBRARIES.iter().chain(PLUGINS.iter()) {
            assert!(!g.is_empty() && !a.is_empty() && !doc.is_empty(), "{g}:{a}");
        }
    }

    #[test]
    fn a_plugin_block_is_offered_plugins_and_not_libraries() {
        assert!(table(true).iter().any(|(_, a, _)| *a == "maven-compiler-plugin"));
        assert!(!table(true).iter().any(|(_, a, _)| *a == "spring-core"));
        assert!(table(false).iter().any(|(_, a, _)| *a == "spring-core"));
    }
}
