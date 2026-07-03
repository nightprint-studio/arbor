/**
 * MOCK — remove when bennu-be serves real data.
 *
 * A self-contained demo project so the Bennu shell is populated for look-and-feel
 * validation WITHOUT a running `bennu-be`. The project store falls back to this
 * (try real IPC → catch → mock) so opening the window "just works" whether or not
 * the backend is attached, and the Command Palette / title-bar expose an explicit
 * "Load demo project" affordance too.
 *
 * This demo is a realistic **multi-module Maven reactor** (a parent POM with
 * `core` / `web` / `batch` modules) so the shell exercises the module-aware tree,
 * multiple `src/main/java` package roots, `src/main/webapp` JSPs, and several
 * detected capabilities (Struts convention, JSP taglibs, OGNL, Spring XML DI,
 * JPA/Hibernate, Tiles, JDBC, Entando). Encodings are deliberately mixed
 * (Cp1252 in the legacy Java sources, UTF-8 in the newer XML/JSP/POM) so the
 * encoding pill has something to show.
 *
 * Everything here mirrors the real BE↔FE contract shapes (`$lib/types/bennu`)
 * field-for-field, so swapping it out for real IPC is a no-op for consumers.
 *
 * To remove: delete this file, drop `DEMO_ROOT` / `loadDemo` / the try-real-catch-mock
 * fallback in `project.svelte.ts`, and the "demo" badge + palette/menu entries that
 * reference `projectStore.isDemo`.
 */

import type { ProjectInfo, TreeNode, ReadFileResult } from '$lib/types/bennu';

/** Sentinel root path for the demo project (never touches the filesystem). */
export const DEMO_ROOT = 'demo://PortaleAppalti';

const SEP = '/';
const j = (...parts: string[]) => parts.join(SEP);

// ── File sources ───────────────────────────────────────────────────────────────
// Real-ish content so the Java tree-sitter highlight actually renders fields,
// methods, annotations, strings and keywords, the JSP shows taglib markup, and the
// Spring/Hibernate/Tiles XML gives the config-graph resolvers something to chew on.

// ── module: core (the domain + persistence layer) ────────────────────────────────

const BANDO_MODEL_JAVA = `package it.appalti.portale.core.model;

import java.io.Serializable;
import javax.persistence.Entity;
import javax.persistence.Table;
import javax.persistence.Id;
import javax.persistence.GeneratedValue;
import javax.persistence.Column;

/**
 * JPA entity for a tender ("bando"). Mapped via Hibernate — see
 * core/src/main/resources/hibernate.cfg.xml. The legacy columns keep their
 * original Italian names for backward compatibility with the old DB.
 */
@Entity
@Table(name = "APP_BANDO")
public class Bando implements Serializable {

    private static final long serialVersionUID = 1L;

    @Id
    @GeneratedValue
    @Column(name = "ID_BANDO")
    private Long id;

    @Column(name = "CODICE", length = 32, nullable = false)
    private String codice;

    @Column(name = "OGGETTO", length = 4000)
    private String oggetto;

    @Column(name = "IMPORTO")
    private Double importo;

    public Long getId() { return id; }
    public void setId(Long id) { this.id = id; }

    public String getCodice() { return codice; }
    public void setCodice(String codice) { this.codice = codice; }

    public String getOggetto() { return oggetto; }
    public void setOggetto(String oggetto) { this.oggetto = oggetto; }

    public Double getImporto() { return importo; }
    public void setImporto(Double importo) { this.importo = importo; }
}
`;

const BANDO_SERVICE_JAVA = `package it.appalti.portale.core.service;

import java.util.List;
import it.appalti.portale.core.model.Bando;

/** Query surface for tenders — implemented over the Hibernate-backed DAO. */
public interface BandoService {
    List<Bando> search(String query, int page, int pageSize);
    Bando findById(long id);
}
`;

const BANDO_DAO_JAVA = `package it.appalti.portale.core.dao;

import java.util.List;
import org.hibernate.Session;
import org.hibernate.SessionFactory;
import org.hibernate.query.Query;
import it.appalti.portale.core.model.Bando;
import it.appalti.portale.core.service.BandoService;

/**
 * Hibernate implementation of {@link BandoService}. Wired as a Spring bean in
 * web/src/main/resources/applicationContext.xml (constructor-injected SessionFactory).
 */
public class BandoDaoImpl implements BandoService {

    private final SessionFactory sessionFactory;

    // FIXME: parametrize the HQL to stop building the where-clause by hand.
    public BandoDaoImpl(SessionFactory sessionFactory) {
        this.sessionFactory = sessionFactory;
    }

    @Override
    public List<Bando> search(String query, int page, int pageSize) {
        Session session = sessionFactory.getCurrentSession();
        Query<Bando> q = session.createQuery(
            "from Bando b where lower(b.oggetto) like :q order by b.codice", Bando.class);
        q.setParameter("q", "%" + query.toLowerCase() + "%");
        q.setFirstResult((page - 1) * pageSize);
        q.setMaxResults(pageSize);
        return q.list();
    }

    @Override
    public Bando findById(long id) {
        Session session = sessionFactory.getCurrentSession();
        return session.get(Bando.class, id);
    }
}
`;

const HIBERNATE_CFG_XML = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE hibernate-configuration PUBLIC
  "-//Hibernate/Hibernate Configuration DTD 3.0//EN"
  "http://hibernate.org/dtd/hibernate-configuration-3.0.dtd">
<hibernate-configuration>
  <session-factory>
    <property name="hibernate.dialect">org.hibernate.dialect.Oracle12cDialect</property>
    <property name="hibernate.connection.datasource">java:/comp/env/jdbc/appaltiDS</property>
    <property name="hibernate.current_session_context_class">thread</property>
    <property name="hibernate.show_sql">false</property>
    <mapping class="it.appalti.portale.core.model.Bando"/>
  </session-factory>
</hibernate-configuration>
`;

const CORE_POM_XML = `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>

  <parent>
    <groupId>it.appalti</groupId>
    <artifactId>portale-appalti</artifactId>
    <version>3.7.2</version>
  </parent>

  <artifactId>portale-core</artifactId>
  <packaging>jar</packaging>
  <name>PortaleAppalti :: Core</name>

  <dependencies>
    <dependency>
      <groupId>org.hibernate</groupId>
      <artifactId>hibernate-core</artifactId>
      <version>5.4.32.Final</version>
    </dependency>
    <dependency>
      <groupId>javax.persistence</groupId>
      <artifactId>javax.persistence-api</artifactId>
      <version>2.2</version>
    </dependency>
  </dependencies>
</project>
`;

// ── module: web (the Struts/JSP front-end) ───────────────────────────────────────

const BANDO_ACTION_JAVA = `package it.appalti.portale.web.action;

import java.util.List;
import com.opensymphony.xwork2.ActionSupport;
import org.apache.struts2.convention.annotation.Action;
import org.apache.struts2.convention.annotation.Result;
import it.appalti.portale.core.model.Bando;
import it.appalti.portale.core.service.BandoService;

/**
 * Struts 2 action backing the tender ("bando") search + detail pages.
 * Wired by convention (@Action) and via struts.xml for the legacy routes;
 * the results render through Tiles (see tiles.xml) into the JSP below.
 */
public class BandoAction extends ActionSupport {

    private static final long serialVersionUID = 1L;
    private static final int PAGE_SIZE = 20;

    private BandoService bandoService;

    private String query;
    private int page = 1;
    private List<Bando> results;
    private Bando bando;

    @Action(value = "bando-search", results = {
        @Result(name = "success", type = "tiles", location = "bando.search")
    })
    public String search() {
        if (query == null || query.trim().isEmpty()) {
            addActionError("Inserire un criterio di ricerca");
            return INPUT;
        }
        this.results = bandoService.search(query, page, PAGE_SIZE);
        return SUCCESS;
    }

    public String detail() {
        this.bando = bandoService.findById(getId());
        return bando != null ? SUCCESS : NONE;
    }

    private long getId() {
        String raw = query;
        return raw != null ? Long.parseLong(raw) : -1L;
    }

    // ── getters / setters (Struts value stack) ──────────────────────────────
    public void setBandoService(BandoService s) { this.bandoService = s; }
    public String getQuery() { return query; }
    public void setQuery(String query) { this.query = query; }
    public int getPage() { return page; }
    public void setPage(int page) { this.page = page; }
    public List<Bando> getResults() { return results; }
    public Bando getBando() { return bando; }
}
`;

const BANDO_JSP = `<%@ taglib prefix="s" uri="/struts-tags" %>
<%@ page contentType="text/html; charset=UTF-8" %>
<html>
<head><title>Ricerca bandi</title></head>
<body>
  <h1>Bandi di gara</h1>

  <s:form action="bando-search" method="post">
    <s:textfield name="query" label="Cerca" />
    <s:submit value="Cerca" />
  </s:form>

  <s:if test="results != null && !results.isEmpty">
    <table class="bandi">
      <s:iterator value="results" var="b">
        <tr>
          <td><s:property value="#b.codice" /></td>
          <td><s:property value="#b.oggetto" /></td>
          <td><s:a action="bando-detail"><s:param name="id" value="#b.id" />Dettaglio</s:a></td>
        </tr>
      </s:iterator>
    </table>
  </s:if>
  <s:else>
    <p>Nessun bando trovato.</p>
  </s:else>
</body>
</html>
`;

const LAYOUT_JSP = `<%@ taglib prefix="tiles" uri="http://tiles.apache.org/tags-tiles" %>
<%@ page contentType="text/html; charset=UTF-8" %>
<html>
<head>
  <title><tiles:getAsString name="title" /></title>
</head>
<body>
  <div id="header"><tiles:insertAttribute name="header" /></div>
  <div id="body"><tiles:insertAttribute name="body" /></div>
  <div id="footer">&copy; Comune di Esempio &mdash; Portale Appalti</div>
</body>
</html>
`;

const APPLICATION_CONTEXT_XML = `<?xml version="1.0" encoding="UTF-8"?>
<beans xmlns="http://www.springframework.org/schema/beans"
       xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
       xsi:schemaLocation="http://www.springframework.org/schema/beans
                           http://www.springframework.org/schema/beans/spring-beans.xsd">

  <!-- Hibernate SessionFactory reads core/src/main/resources/hibernate.cfg.xml -->
  <bean id="sessionFactory"
        class="org.springframework.orm.hibernate5.LocalSessionFactoryBean">
    <property name="configLocation" value="classpath:hibernate.cfg.xml"/>
  </bean>

  <!-- DAO gets the SessionFactory constructor-injected -->
  <bean id="bandoService" class="it.appalti.portale.core.dao.BandoDaoImpl">
    <constructor-arg ref="sessionFactory"/>
  </bean>

  <!-- Struts action pulls the service by name off this context -->
  <bean id="bandoAction" class="it.appalti.portale.web.action.BandoAction" scope="prototype">
    <property name="bandoService" ref="bandoService"/>
  </bean>
</beans>
`;

const STRUTS_XML = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE struts PUBLIC
  "-//Apache Software Foundation//DTD Struts Configuration 2.5//EN"
  "http://struts.apache.org/dtds/struts-2.5.dtd">
<struts>
  <constant name="struts.objectFactory" value="spring"/>
  <constant name="struts.convention.result.path" value="/WEB-INF/jsp/"/>

  <package name="appalti" namespace="/" extends="tiles-default">
    <action name="bando-detail" class="bandoAction" method="detail">
      <result name="success" type="tiles">bando.detail</result>
      <result name="none">/WEB-INF/jsp/not-found.jsp</result>
    </action>
  </package>
</struts>
`;

const TILES_XML = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE tiles-definitions PUBLIC
  "-//Apache Software Foundation//DTD Tiles Configuration 3.0//EN"
  "http://tiles.apache.org/dtds/tiles-config_3_0.dtd">
<tiles-definitions>
  <definition name="base.layout" template="/WEB-INF/jsp/layout.jsp">
    <put-attribute name="header" value="/WEB-INF/jsp/header.jsp"/>
  </definition>

  <definition name="bando.search" extends="base.layout">
    <put-attribute name="title" value="Ricerca bandi"/>
    <put-attribute name="body" value="/WEB-INF/jsp/bando.jsp"/>
  </definition>

  <definition name="bando.detail" extends="base.layout">
    <put-attribute name="title" value="Dettaglio bando"/>
    <put-attribute name="body" value="/WEB-INF/jsp/bando-detail.jsp"/>
  </definition>
</tiles-definitions>
`;

const WEB_POM_XML = `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>

  <parent>
    <groupId>it.appalti</groupId>
    <artifactId>portale-appalti</artifactId>
    <version>3.7.2</version>
  </parent>

  <artifactId>portale-web</artifactId>
  <packaging>war</packaging>
  <name>PortaleAppalti :: Web</name>

  <dependencies>
    <dependency>
      <groupId>it.appalti</groupId>
      <artifactId>portale-core</artifactId>
      <version>3.7.2</version>
    </dependency>
    <dependency>
      <groupId>org.apache.struts</groupId>
      <artifactId>struts2-core</artifactId>
      <version>2.5.30</version>
    </dependency>
    <dependency>
      <groupId>org.apache.struts</groupId>
      <artifactId>struts2-convention-plugin</artifactId>
      <version>2.5.30</version>
    </dependency>
    <dependency>
      <groupId>org.apache.struts</groupId>
      <artifactId>struts2-tiles-plugin</artifactId>
      <version>2.5.30</version>
    </dependency>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-web</artifactId>
      <version>5.3.20</version>
    </dependency>
    <dependency>
      <groupId>org.entando</groupId>
      <artifactId>entando-core</artifactId>
      <version>6.3.0</version>
    </dependency>
  </dependencies>
</project>
`;

// ── module: batch (a plain JDBC nightly job — no Struts/Spring) ───────────────────

const IMPORT_JOB_JAVA = `package it.appalti.portale.batch;

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.SQLException;

/**
 * Nightly importer that loads tenders from the ministry feed straight through
 * JDBC (no Hibernate here on purpose — this module predates the ORM migration).
 * TODO: fold this into the core service once the entity mapping is stable.
 */
public class ImportJob {

    private static final String URL = "jdbc:oracle:thin:@//db.example:1521/APP";

    public static void main(String[] args) throws SQLException {
        try (Connection c = DriverManager.getConnection(URL, "batch", "batch")) {
            c.setAutoCommit(false);
            try (PreparedStatement ps = c.prepareStatement(
                    "insert into APP_BANDO (CODICE, OGGETTO, IMPORTO) values (?, ?, ?)")) {
                ps.setString(1, args.length > 0 ? args[0] : "N/D");
                ps.setString(2, "Importazione automatica");
                ps.setDouble(3, 0.0d);
                ps.executeUpdate();
            }
            c.commit();
        }
    }
}
`;

const BATCH_POM_XML = `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>

  <parent>
    <groupId>it.appalti</groupId>
    <artifactId>portale-appalti</artifactId>
    <version>3.7.2</version>
  </parent>

  <artifactId>portale-batch</artifactId>
  <packaging>jar</packaging>
  <name>PortaleAppalti :: Batch</name>

  <dependencies>
    <dependency>
      <groupId>com.oracle.database.jdbc</groupId>
      <artifactId>ojdbc8</artifactId>
      <version>21.5.0.0</version>
    </dependency>
  </dependencies>
</project>
`;

// ── parent reactor POM ───────────────────────────────────────────────────────────

const PARENT_POM_XML = `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>

  <groupId>it.appalti</groupId>
  <artifactId>portale-appalti</artifactId>
  <version>3.7.2</version>
  <packaging>pom</packaging>
  <name>PortaleAppalti</name>

  <properties>
    <maven.compiler.source>1.8</maven.compiler.source>
    <maven.compiler.target>1.8</maven.compiler.target>
    <project.build.sourceEncoding>Cp1252</project.build.sourceEncoding>
  </properties>

  <modules>
    <module>core</module>
    <module>web</module>
    <module>batch</module>
  </modules>
</project>
`;

// ── Sentinel paths ───────────────────────────────────────────────────────────────
// Absolute (sentinel) paths for every demo file, grouped by module so the tree and
// the source map below can share them (single source of truth per file).

const P_POM = j(DEMO_ROOT, 'pom.xml');

// core module
const CORE = j(DEMO_ROOT, 'core');
const P_CORE_POM = j(CORE, 'pom.xml');
const CORE_JAVA = j(CORE, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'core');
const P_MODEL = j(CORE_JAVA, 'model', 'Bando.java');
const P_SERVICE = j(CORE_JAVA, 'service', 'BandoService.java');
const P_DAO = j(CORE_JAVA, 'dao', 'BandoDaoImpl.java');
const P_HIBERNATE_CFG = j(CORE, 'src', 'main', 'resources', 'hibernate.cfg.xml');

// web module
const WEB = j(DEMO_ROOT, 'web');
const P_WEB_POM = j(WEB, 'pom.xml');
const WEB_JAVA = j(WEB, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'web');
const P_ACTION = j(WEB_JAVA, 'action', 'BandoAction.java');
const WEB_RES = j(WEB, 'src', 'main', 'resources');
const P_APP_CTX = j(WEB_RES, 'applicationContext.xml');
const P_STRUTS = j(WEB_RES, 'struts.xml');
const P_TILES = j(WEB_RES, 'tiles.xml');
const WEB_JSP = j(WEB, 'src', 'main', 'webapp', 'WEB-INF', 'jsp');
const P_JSP = j(WEB_JSP, 'bando.jsp');
const P_LAYOUT_JSP = j(WEB_JSP, 'layout.jsp');

// batch module
const BATCH = j(DEMO_ROOT, 'batch');
const P_BATCH_POM = j(BATCH, 'pom.xml');
const BATCH_JAVA = j(BATCH, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'batch');
const P_IMPORT_JOB = j(BATCH_JAVA, 'ImportJob.java');

/** MOCK project manifest — realistic JDK + capability evidence across all modules. */
export const DEMO_PROJECT: ProjectInfo = {
  root: DEMO_ROOT,
  name: 'PortaleAppalti (demo)',
  // Maven `<modules>` — the parent reactor's child module names.
  modules: ['core', 'web', 'batch'],
  jdk: { version: '1.8', source: 'maven.compiler.source' },
  // Legacy target stack: pom-declared Cp1252 source encoding.
  source_encoding: 'Cp1252',
  capabilities: {
    struts_xml_config: true,
    struts_convention: true,
    jsp_taglib_tld: true,
    ognl_value_stack: true,
    tiles_views: true,
    spring_xml_di: true,
    spring_annotation_di: false,
    spring_data_repo: false,
    jpa_hibernate: true,
    mybatis_mapper: false,
    jdbc_dao: true,
    lombok: false,
    entando_japs: true,
    hits: [
      { capability: 'struts_convention', tier: 'A', detail: 'dependency org.apache.struts:struts2-convention-plugin (web)' },
      { capability: 'struts_xml_config', tier: 'B', detail: 'web/src/main/resources/struts.xml' },
      { capability: 'jsp_taglib_tld', tier: 'B', detail: 'taglib /struts-tags in web/.../WEB-INF/jsp/bando.jsp' },
      { capability: 'ognl_value_stack', tier: 'C', detail: 'OGNL expressions (#b.codice) in bando.jsp' },
      { capability: 'tiles_views', tier: 'A', detail: 'dependency org.apache.struts:struts2-tiles-plugin (web)' },
      { capability: 'spring_xml_di', tier: 'B', detail: 'web/src/main/resources/applicationContext.xml' },
      { capability: 'jpa_hibernate', tier: 'A', detail: 'dependency org.hibernate:hibernate-core (core)' },
      { capability: 'jdbc_dao', tier: 'C', detail: 'raw java.sql usage in batch/ImportJob.java' },
      { capability: 'entando_japs', tier: 'A', detail: 'dependency org.entando:entando-core (web)' },
    ],
  },
};

const file = (name: string, path: string): TreeNode => ({ name, path, is_dir: false, children: [] });
const dir = (name: string, path: string, children: TreeNode[]): TreeNode => ({ name, path, is_dir: true, children });

// ── tree builders ────────────────────────────────────────────────────────────────
// A `src/main/java/it/appalti/portale/<module>/<leaf...>` package chain is the same
// shape for every module, so build the nested-directory spine once instead of
// repeating the `it → appalti → portale` boilerplate three times.

/** Nest `children` under a chain of single-child directories named by `segments`,
 *  anchoring each directory's path at `base` + the accumulated segments. */
function nestDirs(base: string, segments: string[], children: TreeNode[]): TreeNode {
  let path = base;
  // Build inner-most first so each wrapper owns its already-built subtree.
  let node: TreeNode | null = null;
  const paths = segments.map((seg) => (path = j(path, seg)));
  for (let i = segments.length - 1; i >= 0; i--) {
    node = dir(segments[i], paths[i], node ? [node] : children);
  }
  return node!;
}

/** The `src/main/java/<packages...>` subtree for a module, holding `leaves`. */
const javaTree = (moduleRoot: string, packages: string[], leaves: TreeNode[]): TreeNode =>
  dir('java', j(moduleRoot, 'src', 'main', 'java'), [nestDirs(j(moduleRoot, 'src', 'main', 'java'), packages, leaves)]);

const IT_APPALTI_PORTALE = ['it', 'appalti', 'portale'];

/** MOCK file tree — the parent reactor with its three modules fully expanded. */
export const DEMO_TREE: TreeNode = dir('PortaleAppalti', DEMO_ROOT, [
  // ── core ──────────────────────────────────────────────────────────────────────
  dir('core', CORE, [
    dir('src', j(CORE, 'src'), [
      dir('main', j(CORE, 'src', 'main'), [
        javaTree(CORE, [...IT_APPALTI_PORTALE, 'core'], [
          dir('dao', j(CORE_JAVA, 'dao'), [file('BandoDaoImpl.java', P_DAO)]),
          dir('model', j(CORE_JAVA, 'model'), [file('Bando.java', P_MODEL)]),
          dir('service', j(CORE_JAVA, 'service'), [file('BandoService.java', P_SERVICE)]),
        ]),
        dir('resources', j(CORE, 'src', 'main', 'resources'), [
          file('hibernate.cfg.xml', P_HIBERNATE_CFG),
        ]),
      ]),
    ]),
    file('pom.xml', P_CORE_POM),
  ]),

  // ── web ───────────────────────────────────────────────────────────────────────
  dir('web', WEB, [
    dir('src', j(WEB, 'src'), [
      dir('main', j(WEB, 'src', 'main'), [
        javaTree(WEB, [...IT_APPALTI_PORTALE, 'web'], [
          dir('action', j(WEB_JAVA, 'action'), [file('BandoAction.java', P_ACTION)]),
        ]),
        dir('resources', WEB_RES, [
          file('applicationContext.xml', P_APP_CTX),
          file('struts.xml', P_STRUTS),
          file('tiles.xml', P_TILES),
        ]),
        dir('webapp', j(WEB, 'src', 'main', 'webapp'), [
          dir('WEB-INF', j(WEB, 'src', 'main', 'webapp', 'WEB-INF'), [
            dir('jsp', WEB_JSP, [
              file('bando.jsp', P_JSP),
              file('layout.jsp', P_LAYOUT_JSP),
            ]),
          ]),
        ]),
      ]),
    ]),
    file('pom.xml', P_WEB_POM),
  ]),

  // ── batch ─────────────────────────────────────────────────────────────────────
  dir('batch', BATCH, [
    dir('src', j(BATCH, 'src'), [
      dir('main', j(BATCH, 'src', 'main'), [
        javaTree(BATCH, [...IT_APPALTI_PORTALE, 'batch'], [
          file('ImportJob.java', P_IMPORT_JOB),
        ]),
      ]),
    ]),
    file('pom.xml', P_BATCH_POM),
  ]),

  file('pom.xml', P_POM),
]);

/** MOCK per-path source + encoding, mirroring `bennu_read_file`.
 *  Legacy `.java` sources are Cp1252 (the declared project encoding); the newer XML,
 *  JSP and POM files are UTF-8 — so the encoding pill shows a realistic mix. */
const DEMO_SOURCES: Record<string, ReadFileResult> = {
  // core
  [P_MODEL]: { text: BANDO_MODEL_JAVA, encoding: 'Cp1252' },
  [P_SERVICE]: { text: BANDO_SERVICE_JAVA, encoding: 'Cp1252' },
  [P_DAO]: { text: BANDO_DAO_JAVA, encoding: 'Cp1252' },
  [P_HIBERNATE_CFG]: { text: HIBERNATE_CFG_XML, encoding: 'UTF-8' },
  [P_CORE_POM]: { text: CORE_POM_XML, encoding: 'UTF-8' },
  // web
  [P_ACTION]: { text: BANDO_ACTION_JAVA, encoding: 'Cp1252' },
  [P_APP_CTX]: { text: APPLICATION_CONTEXT_XML, encoding: 'UTF-8' },
  [P_STRUTS]: { text: STRUTS_XML, encoding: 'UTF-8' },
  [P_TILES]: { text: TILES_XML, encoding: 'UTF-8' },
  [P_JSP]: { text: BANDO_JSP, encoding: 'UTF-8' },
  [P_LAYOUT_JSP]: { text: LAYOUT_JSP, encoding: 'UTF-8' },
  [P_WEB_POM]: { text: WEB_POM_XML, encoding: 'UTF-8' },
  // batch
  [P_IMPORT_JOB]: { text: IMPORT_JOB_JAVA, encoding: 'Cp1252' },
  [P_BATCH_POM]: { text: BATCH_POM_XML, encoding: 'UTF-8' },
  // parent reactor
  [P_POM]: { text: PARENT_POM_XML, encoding: 'UTF-8' },
};

/** Is this a demo (sentinel) path? Used by the store to serve mock sources. */
export function isDemoPath(path: string): boolean {
  return path.startsWith(DEMO_ROOT);
}

/** MOCK file read for a demo path (falls back to an empty UTF-8 buffer). */
export function demoReadFile(path: string): ReadFileResult {
  return DEMO_SOURCES[path] ?? { text: '', encoding: 'UTF-8' };
}
