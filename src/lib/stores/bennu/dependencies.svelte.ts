/**
 * Bennu dependencies — the resolved dependency set for the open Java project,
 * grouped by module, with per-dependency origin (declared directly by the module
 * vs. inherited from a parent pom's `<dependencyManagement>` / parent inheritance).
 *
 * This is the FE seam for a future `bennu-be` classpath/pom resolution pass
 * (Phase-2 `.m2` + effective-POM). The data model below is shaped to map cleanly
 * onto that BE payload field-for-field so swapping the mock for real IPC is a
 * no-op for the panel:
 *
 *   DependencyModule[]                     — one per Maven module (single-module → one)
 *     · module        : string             — module artifactId / display name
 *     · dependencies  : Dependency[]
 *         · groupId    : string
 *         · artifactId : string
 *         · version    : string
 *         · scope      : DependencyScope    — compile | provided | runtime | test | system | import
 *         · origin     : DependencyOrigin
 *             { kind: 'declared' }                        — declared in THIS module's pom
 *           | { kind: 'inherited'; from: string }         — from a parent pom (artifactId)
 *
 * `coord(dep)` renders the `groupId:artifactId` a call-site shows; version + scope
 * + origin are displayed alongside. Keep the shape stable — the BE will provide it.
 *
 * MOCK — replace with real BE dep data (bennu Phase-2 .m2/pom resolution) later.
 */

export type DependencyScope =
  | 'compile'
  | 'provided'
  | 'runtime'
  | 'test'
  | 'system'
  | 'import';

/** Where a dependency entry comes from in the module's effective POM. */
export type DependencyOrigin =
  | { kind: 'declared' }
  | { kind: 'inherited'; from: string };

export interface Dependency {
  groupId: string;
  artifactId: string;
  version: string;
  scope: DependencyScope;
  origin: DependencyOrigin;
}

export interface DependencyModule {
  /** Module artifactId / display name (matches `ProjectInfo.modules` entries). */
  module: string;
  dependencies: Dependency[];
}

/** The `groupId:artifactId` coordinate (version/scope shown separately). */
export function coord(d: Dependency): string {
  return `${d.groupId}:${d.artifactId}`;
}

// ── MOCK data ────────────────────────────────────────────────────────────────
// A realistic MULTI-MODULE set: a `-web` module (WAR) and a `-core` module (JAR),
// each with a few direct deps plus several inherited-from-parent entries pinned by
// the parent's `<dependencyManagement>` (spring-*, commons-*, servlet-api provided,
// junit test). Versions/scopes deliberately mixed to exercise the display.
//
// MOCK — replace with real BE dep data (bennu Phase-2 .m2/pom resolution) later.
const MOCK_MODULES: DependencyModule[] = [
  {
    module: 'portale-web',
    dependencies: [
      { groupId: 'org.apache.struts', artifactId: 'struts2-core', version: '2.5.30', scope: 'compile', origin: { kind: 'declared' } },
      { groupId: 'org.apache.struts', artifactId: 'struts2-json-plugin', version: '2.5.30', scope: 'compile', origin: { kind: 'declared' } },
      { groupId: 'javax.servlet', artifactId: 'javax.servlet-api', version: '4.0.1', scope: 'provided', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'javax.servlet.jsp', artifactId: 'javax.servlet.jsp-api', version: '2.3.3', scope: 'provided', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.springframework', artifactId: 'spring-web', version: '5.3.27', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.springframework', artifactId: 'spring-webmvc', version: '5.3.27', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'commons-fileupload', artifactId: 'commons-fileupload', version: '1.5', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.projectlombok', artifactId: 'lombok', version: '1.18.30', scope: 'provided', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'junit', artifactId: 'junit', version: '4.13.2', scope: 'test', origin: { kind: 'inherited', from: 'portale-parent' } },
    ],
  },
  {
    module: 'portale-core',
    dependencies: [
      { groupId: 'org.hibernate', artifactId: 'hibernate-core', version: '5.6.15.Final', scope: 'compile', origin: { kind: 'declared' } },
      { groupId: 'org.postgresql', artifactId: 'postgresql', version: '42.6.0', scope: 'runtime', origin: { kind: 'declared' } },
      { groupId: 'org.springframework', artifactId: 'spring-context', version: '5.3.27', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.springframework', artifactId: 'spring-tx', version: '5.3.27', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.apache.commons', artifactId: 'commons-lang3', version: '3.12.0', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'commons-io', artifactId: 'commons-io', version: '2.13.0', scope: 'compile', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.projectlombok', artifactId: 'lombok', version: '1.18.30', scope: 'provided', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'junit', artifactId: 'junit', version: '4.13.2', scope: 'test', origin: { kind: 'inherited', from: 'portale-parent' } },
      { groupId: 'org.mockito', artifactId: 'mockito-core', version: '4.11.0', scope: 'test', origin: { kind: 'declared' } },
    ],
  },
];

/**
 * The dependency modules for the open project.
 *
 * MOCK — ignores the actual project and returns the fixed multi-module demo set.
 * When bennu-be lands, this becomes an IPC-backed store keyed by project root.
 */
export function dependencyModules(): DependencyModule[] {
  return MOCK_MODULES;
}
