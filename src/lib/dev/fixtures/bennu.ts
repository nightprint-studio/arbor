/** Mock answers for the "bennu" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

const vp = (type_name: string, label: string, params: [string, string, boolean][] = [], is_field = true) => ({
  type_name, label, is_field,
  params: params.map(([name, kind, required]) => ({ name, kind, required })),
});

const BASE = 'demo://GestionaleAtti';
const classes = [
  ['core', 'model', 'Bando', 'class'], ['core', 'model', 'Allegato', 'class'], ['core', 'model', 'StatoBando', 'enum'],
  ['core', 'service', 'BandoService', 'interface'], ['core', 'service', 'BandoServiceImpl', 'class'],
  ['core', 'dao', 'BandoDao', 'interface'], ['core', 'dao', 'BandoDaoImpl', 'class'],
  ['web', 'action', 'BandoAction', 'class'], ['web', 'action', 'LoginAction', 'class'],
  ['batch', '', 'ChiusuraScaduti', 'class'],
] as const;
const CLASS_INDEX = classes.map(([mod, pkg, simple, kind], i) => ({
  fqcn: `it.comune.gestionale.${mod === 'batch' ? 'batch' : `${mod}.${pkg}`}.${simple}`,
  simple,
  file: `${BASE}/${mod}/src/main/java/it/comune/gestionale/${mod === 'batch' ? 'batch' : `${mod}/${pkg}`}/${simple}.java`,
  line: 12 + i, kind,
}));

const node = (id: string, kind: string, layer: number, dependents: number, dependencies: number, external: number, reach: number, impact: number) => ({
  id, name: id, manifest: `${BASE}/${id === 'gestionale-parent' ? '' : `${id.replace('gestionale-', '')}/`}pom.xml`,
  kind, layer, dependents, dependencies, external, reach, impact, in_cycle: false,
});
const edge = (from: number, to: number, scope = 'compile') => ({ from, to, scope, optional: false, condition: '', in_cycle: false, structural: true });

const entity = (simple: string, table: string, props: [string, string][], has_repository = true) => ({
  fqcn: `it.comune.gestionale.core.model.${simple}`, simple, file: `${BASE}/core/src/main/java/it/comune/gestionale/core/model/${simple}.java`,
  kind: 'entity', table, has_repository, properties: props.map(([path, type_text]) => ({ path, type_text })),
});

const NOW = Math.floor(Date.now() / 1000);
const rev = (id: string, ago: number, kind: string, size: number, extra: Record<string, unknown> = {}) => ({ id, at: (NOW - ago) * 1000, kind, size, blob: id, ...extra });
const javaRules = { type: 'PascalCase', method: 'camelCase', field: 'camelCase', constant: 'UPPER_SNAKE_CASE', parameter: 'camelCase', local: 'camelCase', package: 'any' };
const emptyNaming = { enabled: false, inherit: true, ignore: [], rules: {}, overrides: [] };
const javaNaming = { enabled: true, inherit: true, ignore: ['**/generated/**'], rules: { java: javaRules }, overrides: [] };

export const bennuFixtures: Record<string, FixtureHandler> = {
  // Must answer: the modal re-scans whenever `result` is null, so a null (or failing) reply makes it loop.
  'bennu.bennu_mojibake_project': {
    total_files_scanned: 214, files_with_hits: 2, total_hits: 3,
    files: [
      { file: `${BASE}/web/src/main/webapp/WEB-INF/messages_it.properties`, hits: [{ start: 120, end: 124, bad: 'Ã ', fix: 'à' }, { start: 310, end: 314, bad: 'Ã¨', fix: 'è' }] },
      { file: `${BASE}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`, hits: [{ start: 842, end: 846, bad: 'Ã¹', fix: 'ù' }] },
    ],
  },
  'bennu.bennu_maven_model': {
    launcher: '/opt/maven/bin/mvn',
    modules: [
      { dir: '', artifact_id: 'gestionale-parent', name: 'Gestionale', packaging: 'pom', pom: `${BASE}/pom.xml`, plugins: [] },
      { dir: 'core', artifact_id: 'gestionale-core', name: '', packaging: 'jar', pom: `${BASE}/core/pom.xml`, plugins: [{ group_id: 'org.apache.maven.plugins', artifact_id: 'maven-compiler-plugin', version: '3.11.0', prefix: 'compiler', goals: ['compile', 'testCompile'], managed_only: false }] },
      { dir: 'web', artifact_id: 'gestionale-web', name: '', packaging: 'war', pom: `${BASE}/web/pom.xml`, plugins: [{ group_id: 'org.apache.maven.plugins', artifact_id: 'maven-war-plugin', version: '3.3.2', prefix: 'war', goals: ['war', 'exploded'], managed_only: false }] },
      { dir: 'batch', artifact_id: 'gestionale-batch', name: '', packaging: 'jar', pom: `${BASE}/batch/pom.xml`, plugins: [] },
    ],
    profiles: [{ id: 'dev', active_by_default: true, module: '' }, { id: 'prod', active_by_default: false, module: '' }],
    lifecycle: [
      { id: 'clean', hint: 'Delete target/', lifecycle: 'clean' }, { id: 'compile', hint: 'Compile the sources', lifecycle: 'default' },
      { id: 'test', hint: 'Run the unit tests', lifecycle: 'default' }, { id: 'package', hint: 'Build the jar / war', lifecycle: 'default' },
      { id: 'install', hint: 'Copy into ~/.m2', lifecycle: 'default' },
    ],
  },
  'bennu.bennu_todos': {
    truncated: false,
    items: [
      { file: `${BASE}/core/src/main/java/it/comune/gestionale/core/service/BandoServiceImpl.java`, line: 57, kind: 'TODO', text: 'validare la scadenza prima di chiudere il bando' },
      { file: `${BASE}/core/src/main/java/it/comune/gestionale/core/dao/BandoDaoImpl.java`, line: 34, kind: 'FIXME', text: 'la query ignora gli stati ARCHIVIATO' },
      { file: `${BASE}/web/src/main/java/it/comune/gestionale/web/action/BandoAction.java`, line: 120, kind: 'HACK', text: 'workaround per il token CSRF duplicato' },
    ],
  },
  'bennu.bennu_maven_status': {
    repository: '/Users/dev/.m2/repository', repository_exists: true, artifacts: 3812, versions: 9120, maven: '/opt/maven/bin/mvn',
    resolved_jars: 41, modules: ['gestionale-core', 'gestionale-web', 'gestionale-batch'],
    missing: [{ coord: 'it.comune:gestionale-legacy-lib:2.1.0', group_id: 'it.comune', artifact_id: 'gestionale-legacy-lib', version: '2.1.0', path: '/Users/dev/.m2/repository/it/comune/gestionale-legacy-lib/2.1.0', other_versions: ['2.0.3'], module: 'web', via: '' }],
    unversioned: [], shortfall: '1 dependency is not in the local repository',
  },
  'bennu.bennu_get_run_config': {
    active_id: 'rc1',
    configs: [
      { id: 'rc1', name: 'Gestionale web (Jetty)', kind: 'application', module: 'web', main_class: 'it.comune.gestionale.web.Launcher', program_args: '--port 8080', vm_args: '-Xmx512m -Dspring.profiles.active=dev', working_dir: '', env: [{ key: 'DB_URL', value: 'jdbc:oracle:thin:@db-test:1521/GEST' }], test_scope: 'class', test_target: '', debug_suspend: false },
      { id: 'rc2', name: 'Chiusura scaduti (batch)', kind: 'application', module: 'batch', main_class: 'it.comune.gestionale.batch.ChiusuraScaduti', program_args: '', vm_args: '', working_dir: '', env: [], test_scope: 'class', test_target: '' },
      { id: 'rc3', name: 'BandoServiceTest', kind: 'junit', module: 'core', main_class: '', program_args: '', vm_args: '', working_dir: '', env: [], test_scope: 'class', test_target: 'it.comune.gestionale.core.service.BandoServiceTest' },
    ],
  },
  'bennu.bennu_get_naming_defaults': emptyNaming,
  'bennu.bennu_get_naming_config': javaNaming,
  'bennu.bennu_naming_catalog': {
    packs: [{ id: 'java', label: 'Java', extensions: ['java'], standard: javaRules, source: 'grammar', supported: ['type', 'method', 'field', 'constant', 'parameter', 'local', 'package'], present: true }],
    targets: ['type', 'method', 'field', 'constant', 'parameter', 'local', 'package'].map((id) => ({ id, label: id[0].toUpperCase() + id.slice(1), fileLocal: id === 'local' || id === 'parameter' })),
    conventions: ['any', 'camelCase', 'PascalCase', 'UPPER_SNAKE_CASE', 'snake_case'],
  },
  'bennu.bennu_get_tomcat_config': { tomcat_root: '', webapp_name: '' },
  'bennu.bennu_detect_tomcat': { valid: true, source_webapp: `${BASE}/web/src/main/webapp`, webapps: ['ROOT', 'gestionale', 'manager'], suggested: 'gestionale', jsp_count: 47 },
  'bennu.bennu_history_file': {
    path: `${BASE}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`, deleted: false,
    revisions: [
      rev('r5', 120, 'saved', 3120),
      rev('r4', 3600, 'refactored', 3098, { title: 'Rename field scadenza', change: 'rn1' }),
      rev('r3', 86400, 'external', 2874, { title: 'Changed outside Arbor' }),
      rev('r2', 90000, 'saved', 2874, { label: 'Before JPA migration' }),
      rev('r1', 260000, 'created', 1210),
    ],
  },
  'bennu.bennu_history_deleted': [
    { path: `${BASE}/core/src/main/java/it/comune/gestionale/core/model/LegacyBando.java`, name: 'LegacyBando.java', at: (NOW - 7200) * 1000, kind: 'deleted', size: 2210, blob: 'd1', revisions: 6 },
  ],
  'bennu.get_bennu_workspaces': {
    active_id: 'w1',
    workspaces: [
      { id: 'w1', name: 'Gestionale', color_idx: 2, active_project: BASE, projects: [{ root: BASE, open_files: [], active_file: '' }, { root: '/Users/dev/projects/anagrafe', open_files: [], active_file: '' }] },
      { id: 'w2', name: 'Strumenti', color_idx: 7, active_project: '/Users/dev/projects/rust-service', projects: [{ root: '/Users/dev/projects/rust-service', open_files: [], active_file: '' }] },
    ],
  },
  'bennu.bennu_module_graph': {
    ecosystem: 'maven',
    nodes: [
      node('gestionale-core', 'jar', 0, 2, 0, 14, 0, 2),
      node('gestionale-web', 'war', 1, 0, 1, 22, 1, 0),
      node('gestionale-batch', 'jar', 1, 0, 1, 6, 1, 0),
    ],
    edges: [edge(1, 0), edge(2, 0)],
    cycles: [], depth: 2, external_total: 38, truncated: false,
  },
  'bennu.bennu_project_modules': [{ kind: 'Maven', dir: '.' }, { kind: 'Maven', dir: 'core' }, { kind: 'Maven', dir: 'web' }, { kind: 'Maven', dir: 'batch' }],
  'bennu.bennu_jpa_form_model': {
    entities: [
      entity('Bando', 'BANDI', [['id', 'Long'], ['titolo', 'String'], ['scadenza', 'Date'], ['stato', 'StatoBando'], ['importo', 'BigDecimal']]),
      entity('Allegato', 'ALLEGATI', [['id', 'Long'], ['nome', 'String'], ['bando.titolo', 'String']], false),
    ],
    repositories: [{ fqcn: 'it.comune.gestionale.core.dao.BandoRepository', simple: 'BandoRepository', entity: 'Bando', file: `${BASE}/core/src/main/java/it/comune/gestionale/core/dao/BandoRepository.java` }],
    subjects: [['find', 'Return the matching rows'], ['count', 'Count the matching rows'], ['exists', 'Whether any row matches'], ['delete', 'Remove the matching rows']],
    keywords: [
      { keyword: '', label: 'equals', args: 1, collection: false }, { keyword: 'Not', label: 'not equal to', args: 1, collection: false },
      { keyword: 'Like', label: 'like', args: 1, collection: false }, { keyword: 'Between', label: 'between', args: 2, collection: false },
      { keyword: 'In', label: 'in', args: 1, collection: true }, { keyword: 'IsNull', label: 'is null', args: 0, collection: false },
    ],
    lifecycle: [['PrePersist', 'Before the row is first saved'], ['PostLoad', 'After the row is read'], ['PreUpdate', 'Before an update is flushed']],
    relations: ['OneToMany', 'ManyToOne', 'ManyToMany', 'OneToOne'],
  },
  'bennu.bennu_class_index': CLASS_INDEX,
  'bennu.bennu_index_stats': { types: 214, members: 1873, jdk_version: '1.8', jar_count: 42, actions: 18, beans: 31, relations: 96, type_names: 28450, ready: true, engine: 'bennu-index', provider_stage: 'dependencies' },
  'bennu.bennu_index_entries': (p) => {
    const kind = String((p.args as { kind?: string })?.kind);
    const byKind: Record<string, unknown[]> = {
      members: [
        { primary: 'chiudiBando', secondary: 'it.comune.gestionale.core.service.BandoService · void chiudiBando(Long)', file: CLASS_INDEX[3].file, line: 18, kind: 'method' },
        { primary: 'findByStato', secondary: 'it.comune.gestionale.core.dao.BandoDao · List<Bando> findByStato(StatoBando)', file: CLASS_INDEX[5].file, line: 22, kind: 'method' },
        { primary: 'scadenza', secondary: 'it.comune.gestionale.core.model.Bando · Date scadenza', file: CLASS_INDEX[0].file, line: 31, kind: 'field' },
      ],
      jars: [
        { primary: 'struts2-core-2.5.30.jar', secondary: '/Users/dev/.m2/repository/org/apache/struts/struts2-core/2.5.30/struts2-core-2.5.30.jar', file: null, line: null, kind: 'jar' },
        { primary: 'spring-context-5.3.27.jar', secondary: '/Users/dev/.m2/repository/org/springframework/spring-context/5.3.27/spring-context-5.3.27.jar', file: null, line: null, kind: 'jar' },
        { primary: 'hibernate-core-5.6.15.Final.jar', secondary: '/Users/dev/.m2/repository/org/hibernate/hibernate-core/5.6.15.Final/hibernate-core-5.6.15.Final.jar', file: null, line: null, kind: 'jar' },
      ],
      jdk: [{ primary: 'Language level', secondary: '1.8 · maven.compiler.source', file: null, line: null, kind: 'module' }],
      beans: [
        { primary: 'bandoService', secondary: CLASS_INDEX[4].fqcn, file: `${BASE}/core/src/main/resources/applicationContext.xml`, line: 14, kind: 'bean' },
        { primary: 'bandoDao', secondary: CLASS_INDEX[6].fqcn, file: `${BASE}/core/src/main/resources/applicationContext.xml`, line: 22, kind: 'bean' },
      ],
      actions: [{ primary: '/bando/list', secondary: CLASS_INDEX[7].fqcn, file: `${BASE}/web/src/main/resources/struts.xml`, line: 9, kind: 'action' }],
      relations: [{ primary: 'bandoService → bandoDao', secondary: 'bean-ref', file: null, line: null, kind: 'relation' }],
    };
    return byKind[kind] ?? [];
  },
  'bennu.bennu_validation_context': {
    action_simple: 'BandoAction',
    action_fqcn: 'it.comune.gestionale.web.action.BandoAction',
    properties: ['titolo', 'descrizione', 'scadenza', 'importo', 'email'],
    existing_fields: ['titolo'],
  },
  'bennu.bennu_validator_catalog': [
    vp('requiredstring', 'Required string', [['trim', 'bool', false]]),
    vp('required', 'Required'),
    vp('stringlength', 'String length', [['minLength', 'int', false], ['maxLength', 'int', false], ['trim', 'bool', false]]),
    vp('int', 'Integer range', [['min', 'int', false], ['max', 'int', false]]),
    vp('double', 'Decimal range', [['minInclusive', 'double', false], ['maxInclusive', 'double', false]]),
    vp('date', 'Date range', [['min', 'date', false], ['max', 'date', false]]),
    vp('email', 'E-mail address'),
    vp('regex', 'Regular expression', [['regex', 'regex', true], ['caseSensitive', 'bool', false]]),
    vp('fieldexpression', 'Field expression', [['expression', 'ognl', true]]),
    vp('expression', 'Expression', [['expression', 'ognl', true]], false),
  ],
  'bennu.bennu_validation_author': (p) => String((p.args as { existing_xml?: string })?.existing_xml ?? ''),
};
