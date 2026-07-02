/**
 * MOCK — remove when bennu-be serves real data.
 *
 * A self-contained demo project so the Bennu shell is populated for look-and-feel
 * validation WITHOUT a running `bennu-be`. The project store falls back to this
 * (try real IPC → catch → mock) so opening the window "just works" whether or not
 * the backend is attached, and the Command Palette / title-bar expose an explicit
 * "Load demo project" affordance too.
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
// methods, annotations, strings and keywords, and the JSP shows taglib markup.

const BANDO_ACTION_JAVA = `package it.appalti.portale.action;

import java.util.List;
import com.opensymphony.xwork2.ActionSupport;
import org.apache.struts2.convention.annotation.Action;
import org.apache.struts2.convention.annotation.Result;
import it.appalti.portale.model.Bando;
import it.appalti.portale.service.BandoService;

/**
 * Struts 2 action backing the tender ("bando") search + detail pages.
 * Wired by convention (@Action) and via struts.xml for the legacy routes.
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
        @Result(name = "success", location = "/WEB-INF/jsp/bando.jsp")
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

const POM_XML = `<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>

  <groupId>it.appalti</groupId>
  <artifactId>portale-appalti</artifactId>
  <version>3.7.2</version>
  <packaging>war</packaging>
  <name>PortaleAppalti</name>

  <properties>
    <maven.compiler.source>1.8</maven.compiler.source>
    <maven.compiler.target>1.8</maven.compiler.target>
    <project.build.sourceEncoding>Cp1252</project.build.sourceEncoding>
  </properties>

  <dependencies>
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
      <groupId>org.entando</groupId>
      <artifactId>entando-core</artifactId>
      <version>6.3.0</version>
    </dependency>
  </dependencies>
</project>
`;

// Absolute (sentinel) paths for the demo files.
const P_ACTION = j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'action', 'BandoAction.java');
const P_SERVICE = j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'service', 'BandoService.java');
const P_JSP = j(DEMO_ROOT, 'src', 'main', 'webapp', 'WEB-INF', 'jsp', 'bando.jsp');
const P_POM = j(DEMO_ROOT, 'pom.xml');

const BANDO_SERVICE_JAVA = `package it.appalti.portale.service;

import java.util.List;
import it.appalti.portale.model.Bando;

/** Query surface for tenders — implemented over the legacy JDBC DAO. */
public interface BandoService {
    List<Bando> search(String query, int page, int pageSize);
    Bando findById(long id);
}
`;

/** MOCK project manifest — realistic JDK + capability evidence. */
export const DEMO_PROJECT: ProjectInfo = {
  root: DEMO_ROOT,
  name: 'PortaleAppalti (demo)',
  modules: [],
  jdk: { version: '1.8', source: 'maven.compiler.source' },
  capabilities: {
    struts_xml_config: true,
    struts_convention: true,
    jsp_taglib_tld: true,
    ognl_value_stack: true,
    tiles_views: false,
    spring_xml_di: false,
    spring_annotation_di: false,
    spring_data_repo: false,
    jpa_hibernate: false,
    mybatis_mapper: false,
    jdbc_dao: true,
    lombok: false,
    entando_japs: true,
    hits: [
      { capability: 'struts_convention', tier: 'A', detail: 'dependency org.apache.struts:struts2-convention-plugin' },
      { capability: 'jsp_taglib_tld', tier: 'B', detail: 'taglib /struts-tags in WEB-INF/jsp/bando.jsp' },
      { capability: 'jdbc_dao', tier: 'C', detail: 'BandoService implemented over legacy JDBC DAO' },
      { capability: 'entando_japs', tier: 'A', detail: 'dependency org.entando:entando-core' },
    ],
  },
};

const file = (name: string, path: string): TreeNode => ({ name, path, is_dir: false, children: [] });
const dir = (name: string, path: string, children: TreeNode[]): TreeNode => ({ name, path, is_dir: true, children });

/** MOCK file tree — a few realistic folders down to the sample files. */
export const DEMO_TREE: TreeNode = dir('PortaleAppalti', DEMO_ROOT, [
  dir('src', j(DEMO_ROOT, 'src'), [
    dir('main', j(DEMO_ROOT, 'src', 'main'), [
      dir('java', j(DEMO_ROOT, 'src', 'main', 'java'), [
        dir('it', j(DEMO_ROOT, 'src', 'main', 'java', 'it'), [
          dir('appalti', j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti'), [
            dir('portale', j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti', 'portale'), [
              dir('action', j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'action'), [
                file('BandoAction.java', P_ACTION),
              ]),
              dir('service', j(DEMO_ROOT, 'src', 'main', 'java', 'it', 'appalti', 'portale', 'service'), [
                file('BandoService.java', P_SERVICE),
              ]),
            ]),
          ]),
        ]),
      ]),
      dir('webapp', j(DEMO_ROOT, 'src', 'main', 'webapp'), [
        dir('WEB-INF', j(DEMO_ROOT, 'src', 'main', 'webapp', 'WEB-INF'), [
          dir('jsp', j(DEMO_ROOT, 'src', 'main', 'webapp', 'WEB-INF', 'jsp'), [
            file('bando.jsp', P_JSP),
          ]),
        ]),
      ]),
    ]),
  ]),
  file('pom.xml', P_POM),
]);

/** MOCK per-path source + encoding, mirroring `bennu_read_file`. */
const DEMO_SOURCES: Record<string, ReadFileResult> = {
  [P_ACTION]: { text: BANDO_ACTION_JAVA, encoding: 'Cp1252' },
  [P_SERVICE]: { text: BANDO_SERVICE_JAVA, encoding: 'Cp1252' },
  [P_JSP]: { text: BANDO_JSP, encoding: 'UTF-8' },
  [P_POM]: { text: POM_XML, encoding: 'UTF-8' },
};

/** Is this a demo (sentinel) path? Used by the store to serve mock sources. */
export function isDemoPath(path: string): boolean {
  return path.startsWith(DEMO_ROOT);
}

/** MOCK file read for a demo path (falls back to an empty UTF-8 buffer). */
export function demoReadFile(path: string): ReadFileResult {
  return DEMO_SOURCES[path] ?? { text: '', encoding: 'UTF-8' };
}
