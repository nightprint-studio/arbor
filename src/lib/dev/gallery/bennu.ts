/** Gallery scene overrides and extra scenes for the "bennu" area — see gallery/registry.ts. */
import { Folder, GitBranch, Play, Bug, Package, ListChecks, Database, Search } from 'lucide-svelte';
import type { SceneOverride, ExtraScene } from './scenes';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuRegexStore } from '$lib/stores/bennu/regex.svelte';
import { bennuSsrStore } from '$lib/stores/bennu/ssr.svelte';
import { mount } from 'svelte';
import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
import { bennuHistoryStore } from '$lib/stores/bennu/history.svelte';
import { bennuRunConfigStore } from '$lib/stores/bennu/run-config.svelte';
import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
import { bennuImberSchedules } from '$lib/stores/bennu/imber-schedules.svelte';
import type { ImberJob } from '$lib/types/imber-schedule';
import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
import type { NamingFixPlan, NamingTarget, RenamedName } from '$lib/ipc/bennu/naming';

const noop = () => {};

/** Load the built-in demo Maven project (core/web/batch) so modals that read the store have data. */
const DEMO = 'demo://GestionaleAtti';
const ACTION = `${DEMO}/web/src/main/java/it/comune/gestionale/web/action/BandoAction.java`;
const BANDO = `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`;
const seedProject = () => { projectStore.loadDemo(); };
/** The code-generator dialogs need a window to read from; the gallery has none, so they open on "no project". */
const generatorModal: SceneOverride = { props: () => ({ host: { site: () => null, applyEdits: noop, openFile: noop }, onClose: noop }) };
const GENERATOR_MODALS = ['Security', 'Converter', 'Jackson', 'HttpClient', 'DtoMapper', 'Builder', 'ValueMembers', 'TestSkeleton', 'TauriCommand', 'Bevy', 'RustHttpClient', 'RatatuiApp', 'RatatuiWidget', 'RatatuiList', 'RatatuiKeymap', 'RatatuiDialog', 'SoapCall', 'CxfRest'];

export const bennuOverrides: Record<string, SceneOverride> = {
  ...Object.fromEntries(GENERATOR_MODALS.map((name) => [`bennu/generate/${name}GenerateModal`, generatorModal])),
  'bennu/BennuCustomizeRailsModal': {
    props: () => ({
      onClose: noop,
      rails: {
        leftTop: [
          { id: 'project', tooltip: 'Project', icon: Folder, shortcut: 'Alt+1' },
          { id: 'git', tooltip: 'Git', icon: GitBranch },
          { id: 'catalog', tooltip: 'Catalog', icon: Package },
        ],
        leftBottom: [{ id: 'problems', tooltip: 'Problems', icon: ListChecks }],
        rightTop: [{ id: 'database', tooltip: 'Database', icon: Database }],
        rightBottom: [
          { id: 'run', tooltip: 'Run', icon: Play },
          { id: 'debug', tooltip: 'Debug', icon: Bug },
          { id: 'search', tooltip: 'Search', icon: Search },
        ],
      },
    }),
  },
  'bennu/settings/ExternalToolModal': {
    props: () => ({
      tool: {
        name: 'lint', description: 'Checks files against the house style. Pass the paths as arguments.', program: '/usr/local/bin/lint',
        args: ['--format', 'json'], extra_args: true, env: { RUST_LOG: 'info' }, env_keys: ['LEVEL'], writes: false,
        approval: 'conversation', timeout_s: 120, enabled: true,
      },
      onSave: noop,
      onClose: noop,
    }),
  },
  'bennu/BennuDebugValueModal': {
    props: () => ({
      inspect: {
        value: { name: 'bando', kind: 'local', type_name: 'Bando', value: 'Bando@1f3c', object: '4711' },
        text: 'Bando(\n  id: 1042,\n  titolo: "Contributi sport 2026",\n  stato: APERTO,\n  scadenza: "2026-12-31",\n  importo: 125000.0,\n  allegati: [\n    Allegato(nome: "regolamento.pdf", size: 218344),\n    Allegato(nome: "modulo.docx", size: 48211),\n  ],\n)',
        nodes: 9,
        truncated: false,
        loading: false,
        error: '',
      },
    }),
  },
  'bennu/BennuEnvVarModal': {
    props: () => ({
      onClose: noop,
      view: {
        key: 'spring.jpa.show-sql',
        value: 'true',
        name: 'SPRING_JPA_SHOWSQL',
        forms: [
          ['.env', 'SPRING_JPA_SHOWSQL=true'],
          ['shell', 'export SPRING_JPA_SHOWSQL=true'],
          ['docker run', '-e SPRING_JPA_SHOWSQL=true'],
          ['compose', 'SPRING_JPA_SHOWSQL: "true"'],
        ],
      },
    }),
  },
  'bennu/BennuHtmlScriptsModal': {
    props: () => ({ path: `${DEMO}/web/src/main/webapp/index.html`, onAllow: noop, onCancel: noop }),
  },
  'bennu/BennuRegexModal': {
    setup: () => bennuRegexStore.open({
      pattern: '(?<year>\\d{4})-(?<month>0[1-9]|1[0-2])-(?<day>[0-3]\\d)', flavour: 'java', flags: '', target: null,
    }),
    props: { onClose: noop },
  },
  'bennu/BennuJpaGenerateModal': {
    setup: seedProject,
    props: () => ({ action: 'jpa.query.list', onClose: noop }),
  },
  'bennu/BennuMacroExpandModal': {
    props: () => ({
      name: 'vec!',
      expansion: '<[_]>::into_vec(\n    #[rustc_box]\n    $crate::boxed::Box::new([1, 2, 3])\n)',
      onReexpand: noop,
      onClose: noop,
    }),
  },
  'bennu/BennuNewFileModal': {
    setup: seedProject,
    props: () => ({ dir: `${DEMO}/core/src/main/java/it/comune/gestionale/core/service`, onClose: noop }),
  },
  'bennu/BennuOverrideModal': {
    props: () => ({
      onClose: noop,
      onGenerate: noop,
      members: [
        { name: 'getId', params: [], return_type: 'Long', visibility: 'public', is_abstract: true, throws: [], declaring_type: 'it.comune.gestionale.core.model.Entity', signature: 'Long getId()', imports: [] },
        { name: 'validate', params: [['Errors', 'errors']], return_type: 'void', visibility: 'public', is_abstract: true, throws: [], declaring_type: 'it.comune.gestionale.core.model.Entity', signature: 'void validate(Errors errors)', imports: [] },
        { name: 'toString', params: [], return_type: 'String', visibility: 'public', is_abstract: false, throws: [], declaring_type: 'java.lang.Object', signature: 'String toString()', imports: [] },
        { name: 'equals', params: [['Object', 'o']], return_type: 'boolean', visibility: 'public', is_abstract: false, throws: [], declaring_type: 'java.lang.Object', signature: 'boolean equals(Object o)', imports: [] },
        { name: 'hashCode', params: [], return_type: 'int', visibility: 'public', is_abstract: false, throws: [], declaring_type: 'java.lang.Object', signature: 'int hashCode()', imports: [] },
        { name: 'clone', params: [], return_type: 'Object', visibility: 'protected', is_abstract: false, throws: ['CloneNotSupportedException'], declaring_type: 'java.lang.Object', signature: 'Object clone() throws CloneNotSupportedException', imports: [] },
      ],
    }),
  },
  'bennu/BennuRenameFileModal': {
    setup: seedProject,
    props: () => ({ path: BANDO, onClose: noop }),
  },
  'bennu/BennuValidationModal': {
    setup: async () => { seedProject(); await projectStore.openFile(ACTION); },
    props: () => ({ onClose: noop }),
  },
};
// Modals that read the open project from the store and render an empty shell without one.
for (const n of [
  'ProjectConfig', 'RunConfig', 'FindInFiles', 'FileStructure', 'Goto', 'ProjectGoto',
  'AddParameter', 'NamingFix', 'GitHistory', 'LocalHistory', 'ModuleGraph', 'IndexInspector',
  'WorkspaceManager', 'TomcatConfig', 'Ssr', 'MojibakeScan', 'Breakpoints', 'Learned',
]) {
  bennuOverrides[`bennu/Bennu${n}Modal`] = { ...bennuOverrides[`bennu/Bennu${n}Modal`], setup: seedProject, settle: 900 };
}

/** The naming review with a plan in it: a detected convention, renames with uses, a typed file move and refusals of several kinds. */
const namingEdit = (file: string, start: number, to: string, reason: string) => ({
  file, start, end: start + 8, new_text: to, old: '', reason, inferred: false,
});
const namingName = (
  rel: string, line: number, from: string, to: string, target: NamingTarget, uses: [string, number][] = [],
): RenamedName => {
  const file = `${DEMO}/core/src/main/java/it/comune/gestionale/core/${rel}`;
  return {
    file, line, from, to, target, file_rename: null,
    edits: [namingEdit(file, 100 + line, to, 'declaration'), ...uses.map(([r, at]) => namingEdit(`${DEMO}/core/src/main/java/it/comune/gestionale/core/${r}`, at, to, 'use'))],
  };
};
const NAMING_PLAN: NamingFixPlan = {
  renamed: [
    namingName('service/BandoServiceImpl.java', 41, 'get_bando', 'getBando', 'method', [['dao/BandoDaoImpl.java', 220], ['model/Bando.java', 80]]),
    namingName('service/BandoServiceImpl.java', 57, 'lista_atti', 'listaAtti', 'method', [['dao/BandoDaoImpl.java', 310]]),
    namingName('service/BandoServiceImpl.java', 63, 'tmp_count', 'tmpCount', 'local'),
    namingName('service/BandoServiceImpl.java', 64, 'src_dir', 'srcDir', 'local'),
    namingName('dao/BandoDaoImpl.java', 22, 'MaxRows', 'MAX_ROWS', 'constant', [['service/BandoServiceImpl.java', 190]]),
    namingName('dao/BandoDaoImpl.java', 34, 'bando_id', 'bandoId', 'parameter'),
    namingName('model/Bando.java', 12, 'data_scadenza', 'dataScadenza', 'field', [['service/BandoServiceImpl.java', 77], ['dao/BandoDaoImpl.java', 91]]),
    { ...namingName('model/old_atto.java', 3, 'old_atto', 'OldAtto', 'type', [['service/BandoServiceImpl.java', 10]]),
      file_rename: { from: `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/old_atto.java`, to: `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/OldAtto.java` } },
  ],
  refused: [
    { file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/model/Bando.java`, line: 14, name: 'user_name', reason: 'serialisation: @JsonProperty takes the Java name as the key it writes, so renaming it changes the JSON / XML / column other systems read. Give it an explicit name first, then rename' },
    { file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoServiceImpl.java`, line: 120, name: 'to_string', reason: 'more than one name in this file would become \`toString\`' },
    { file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoServiceImpl.java`, line: 140, name: 'get_all', reason: '\`getAll\` already appears in this file' },
    { file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/dao/BandoDaoImpl.java`, line: 5, name: 'it_comune_core', reason: 'a package segment is not renamed by editing text — its name IS the directory holding the file. Move the files (Project tree → Move) and the package declarations follow' },
  ],
  files: [],
  capped: false,
  cancelled: false,
  convention: {
    source: 'detected',
    configured_available: false,
    rules: [
      { pack: 'java', target: 'type', convention: 'PascalCase', origin: 'standard', share: null, votes: 0 },
      { pack: 'java', target: 'method', convention: 'camelCase', origin: 'detected', share: 0.82, votes: 140 },
      { pack: 'java', target: 'field', convention: 'camelCase', origin: 'detected', share: 0.91, votes: 96 },
      { pack: 'java', target: 'constant', convention: 'UPPER_SNAKE_CASE', origin: 'standard', share: null, votes: 0 },
      { pack: 'java', target: 'parameter', convention: 'camelCase', origin: 'detected', share: 0.77, votes: 210 },
      { pack: 'java', target: 'local', convention: 'camelCase', origin: 'detected', share: 0.85, votes: 380 },
      { pack: 'java', target: 'package', convention: 'lowercase', origin: 'standard', share: null, votes: 0 },
    ],
  },
};
NAMING_PLAN.files = [...new Set(NAMING_PLAN.renamed.flatMap((r) => r.edits.map((e) => e.file)))];
bennuOverrides['bennu/BennuNamingFixModal'] = {
  ...bennuOverrides['bennu/BennuNamingFixModal'],
  setup: () => {
    seedProject();
    bennuNamingStore.seedFix(NAMING_PLAN, 'project');
  },
};

/** Structural search with a query, its results and a replacement — what the modal looks like in use. */
const ssrHit = (rel: string, line: number, start: number, preview: string, enclosing: string) => ({
  file: `${DEMO}/${rel}`, rel, line, range: { start, end: start + preview.length }, preview, enclosing, unresolved: false,
});
bennuOverrides['bennu/BennuSsrModal'] = {
  ...bennuOverrides['bennu/BennuSsrModal'],
  setup: () => {
    seedProject();
    bennuSsrStore.seed({
      query: '$pre...$ final $post...$ $t$ $n$ = $v$;\nas local',
      replacement: '$pre$ $post$ $t$ $n$ = $v$;',
      explained: { error: null, errorLine: 0, alternatives: 1, captures: ['pre', 'post', 't', 'n', 'v'], expansion: [], literals: ['final'] },
      hits: [
        ssrHit('core/src/main/java/it/comune/core/service/BandoServiceImpl.java', 57, 1204, 'final Bando bando = dao.find(id);', 'chiudi'),
        ssrHit('core/src/main/java/it/comune/core/service/BandoServiceImpl.java', 88, 2310, '@SuppressWarnings("unchecked") final List<Atto> atti = lista();', 'aggiorna'),
        ssrHit('core/src/main/java/it/comune/core/dao/BandoDaoImpl.java', 34, 640, 'final String sql = "select * from bando";', 'cerca'),
      ],
      scanned: 214, parsed: 31, elapsedMs: 934, hasPages: true,
    });
  },
  settle: 900,
};
// Same, but with a Java file open — the caret-bound modals (structure, parameters, history) read it.
for (const n of ['FileStructure', 'AddParameter', 'LocalHistory', 'GitHistory']) {
  bennuOverrides[`bennu/Bennu${n}Modal`] = {
    setup: async () => { seedProject(); await projectStore.openFile(BANDO); },
    settle: 1200,
  };
}
const SVC = `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoServiceImpl.java`;
bennuOverrides['bennu/BennuRunConfigModal'] = {
  setup: async () => { seedProject(); await bennuRunConfigStore.load(DEMO); },
  settle: 1200,
};
bennuOverrides['bennu/BennuBreakpointsModal'] = {
  setup: () => {
    seedProject();
    bennuDebugStore.toggleBreakpoint(DEMO, SVC, 57);
    bennuDebugStore.toggleBreakpoint(DEMO, SVC, 112);
    bennuDebugStore.setBreakpointCondition(DEMO, SVC, 112, 'bando.getImporto() > 100000');
    bennuDebugStore.toggleBreakpoint(DEMO, ACTION, 88);
  },
  settle: 900,
};
bennuOverrides['bennu/BennuLocalHistoryModal'] = {
  setup: async () => { seedProject(); bennuHistoryStore.show(DEMO, BANDO); },
  settle: 1200,
};
Object.assign(bennuOverrides, {
  // The settings dialog landing on Java › Formatter: the page needs the dialog's shell (its card and
  // section classes live there), and the demo project gives it a project scope to switch to.
  'bennu/BennuSettingsModal': {
    setup: () => { seedProject(); bennuUiStore.openSettings('formatter'); },
    props: { onClose: noop },
    settle: 1500,
  },
  'bennu/BennuGenerateModal': { setup: seedProject, props: () => ({ mode: 'getters-setters', onClose: noop, onInsert: noop, onTemplates: noop }) },
  'bennu/BennuNewFolderModal': { setup: seedProject, props: () => ({ dir: `${DEMO}/core/src/main/java/it/comune/gestionale/core`, asPackage: true, onClose: noop }) },
  'bennu/BennuNewModuleModal': { setup: seedProject, props: () => ({ fromDir: DEMO, onClose: noop }) },
  'bennu/BennuPasteModal': {
    setup: seedProject,
    props: () => ({ sources: [BANDO, `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoService.java`], targetDir: `${DEMO}/batch/src/main/java/it/comune/gestionale/batch`, onClose: noop }),
  },
  'bennu/BennuRecentLocationsModal': { setup: seedProject, props: () => ({ onClose: noop, onPick: noop }) },
  'bennu/BennuCargoAddModal': { props: () => ({ root: '/Users/dev/projects/rust-service', initialPackage: 'serde', onClose: noop }) },
  'bennu/BennuSafeDeleteModal': {
    setup: seedProject,
    props: () => ({
      loading: false, onConfirm: noop, onClose: noop, onOpenUsage: noop,
      plan: {
        label: 'method BandoService.chiudiBando(Long)', file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/service/BandoService.java`,
        start: 0, end: 10, safe: false, blocked: null, file_delete: null,
        usages: [
          { file: `${DEMO}/web/src/main/java/it/comune/gestionale/web/action/BandoAction.java`, start: 1, end: 2, line: 88, col: 22, preview: 'bandoService.chiudiBando(bando.getId());' },
          { file: `${DEMO}/batch/src/main/java/it/comune/gestionale/batch/ChiusuraScaduti.java`, start: 1, end: 2, line: 41, col: 18, preview: 'service.chiudiBando(id);' },
        ],
      },
    }),
  },
  'bennu/brp/BrpAddModal': { props: () => ({ target: 'component', onClose: noop }) },
} satisfies Record<string, SceneOverride>);

/**
 * Panels assume a parent that gives them a height; the gallery gives none. This wraps a
 * component in a fixed-size, bordered box (a plain function component, so no extra .svelte).
 */
const framed = (
  load: () => Promise<{ default: any }>,
  w: number,
  h: number,
): ExtraScene['load'] => async () => {
  const inner = (await load()).default;
  const Frame = (anchor: Node, props: Record<string, unknown>) => {
    const box = document.createElement('div');
    box.style.cssText = `width:${w}px;height:${h}px;margin:24px;display:flex;flex-direction:column;overflow:hidden;background:var(--bg-base);border:1px solid var(--border-subtle, #333);border-radius:var(--radius-lg, 8px)`;
    anchor.parentNode?.insertBefore(box, anchor);
    mount(inner, { target: box, props });
  };
  return { default: Frame as any };
};

const FILE_DIAGS = [
  { file: `${DEMO}/web/src/main/java/it/comune/gestionale/web/action/BandoAction.java`, diagnostics: [
    { message: "cannot find symbol: method chiudiBando(Long)", severity: 'error', code: 'unknown-member', start: 1200, end: 1212 },
    { message: "unused import 'java.util.Date'", severity: 'warning', code: 'unused-import', start: 80, end: 100 },
  ] },
  { file: `${DEMO}/core/src/main/java/it/comune/gestionale/core/dao/BandoDaoImpl.java`, diagnostics: [
    { message: "incompatible types: String cannot be converted to Long", severity: 'error', code: 'type-mismatch', start: 900, end: 910 },
  ] },
];

/** Imber's schedules in Bennu: read-only runs, awake and prompt only — one that ended well, one skipped while closed, one failed. */
const NOW = Date.now();
const bennuImberJobs: ImberJob[] = [
  { id: 'b1', name: 'Morning review', enabled: true, recurrence: { kind: 'weekly', days: [0, 1, 2, 3, 4], hour: 9, minute: 0 }, missed: 'run_once',
    action: { kind: 'prompt', prompt: 'What changed on main since yesterday? Point out risky changes.' }, root: DEMO, mode: 'manual',
    created_ms: NOW - 9 * 86_400_000, next_run_ms: NOW + 14 * 3_600_000, last_run: { at_ms: NOW - 10 * 3_600_000, ok: true, detail: 'Three new commits; one touches the batch scheduler.' }, runs: 7 },
  { id: 'b2', name: 'Weekly dependency check', enabled: true, recurrence: { kind: 'cron', expr: '0 0 8 * * 1' }, missed: 'skip',
    action: { kind: 'awake', conversation_id: 'c-1', prompt: 'Look for outdated dependencies and list the ones worth upgrading.' }, root: DEMO, mode: 'manual',
    created_ms: NOW - 20 * 86_400_000, next_run_ms: NOW + 3 * 86_400_000, last_run: { at_ms: NOW - 4 * 86_400_000, ok: true, skipped: true, detail: 'Missed while Bennu was closed.' }, runs: 2 },
  { id: 'b3', name: 'Hourly TODO scan', enabled: false, recurrence: { kind: 'interval', secs: 3600, anchor_ms: 0 }, missed: 'skip',
    action: { kind: 'prompt', prompt: 'List the new TODO comments.' }, root: DEMO, mode: 'manual',
    created_ms: NOW - 2 * 86_400_000, next_run_ms: null, last_run: { at_ms: NOW - 3_600_000, ok: false, detail: 'The spending limit for scheduled runs was reached.' }, runs: 4 },
];

export const bennuExtras: ExtraScene[] = [
  { id: 'shared/ImberSchedulesModal[bennu]', load: () => import('$lib/components/shared/ImberSchedulesModal.svelte'), setup: () => { seedProject(); bennuImberSchedules.seed(bennuImberJobs); }, props: { store: bennuImberSchedules, onClose: noop } },
  { id: 'shared/ImberSchedulesModal[bennu new]', load: () => import('$lib/components/shared/ImberSchedulesModal.svelte'), setup: () => { seedProject(); bennuImberSchedules.seed(bennuImberJobs); bennuImberSchedules.edit('new'); }, props: { store: bennuImberSchedules, onClose: noop } },
  { id: 'bennu/panels/BennuProblemsPanel', load: framed(() => import('$lib/components/bennu/BennuProblemsPanel.svelte'), 900, 320), setup: () => { seedProject(); bennuDiagnosticsStore.setProjectDiagnostics(FILE_DIAGS as never); }, settle: 800 },
  { id: 'bennu/panels/BennuBuildProblemsPanel', load: framed(() => import('$lib/components/bennu/BennuBuildProblemsPanel.svelte'), 900, 320), setup: () => { seedProject(); bennuDiagnosticsStore.setProjectDiagnostics(FILE_DIAGS as never); }, settle: 800 },
  { id: 'bennu/panels/BennuSidebar', load: framed(() => import('$lib/components/bennu/BennuSidebar.svelte'), 360, 700), setup: seedProject, settle: 1000 },
  { id: 'bennu/panels/BennuSymbolList', load: framed(() => import('$lib/components/bennu/BennuSymbolList.svelte'), 360, 600), setup: async () => { seedProject(); await projectStore.openFile(BANDO); }, settle: 1000 },
  { id: 'bennu/panels/BennuTodoPanel', load: framed(() => import('$lib/components/bennu/BennuTodoPanel.svelte'), 640, 360), setup: seedProject, settle: 800 },
  { id: 'bennu/panels/BennuMavenPanel', load: framed(() => import('$lib/components/bennu/BennuMavenPanel.svelte'), 420, 600), setup: seedProject, settle: 800 },
  { id: 'bennu/panels/BennuRunPanel', load: framed(() => import('$lib/components/bennu/BennuRunPanel.svelte'), 900, 320), setup: async () => { seedProject(); await bennuRunConfigStore.load(DEMO); }, settle: 800 },
  { id: 'bennu/panels/BennuDependenciesPanel', load: framed(() => import('$lib/components/bennu/BennuDependenciesPanel.svelte'), 560, 600), setup: seedProject, settle: 800 },
];
