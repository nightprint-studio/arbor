/** Gallery scene overrides and extra scenes for the "bennu" area — see gallery/registry.ts. */
import { Folder, GitBranch, Play, Bug, Package, ListChecks, Database, Search } from 'lucide-svelte';
import type { SceneOverride, ExtraScene } from './scenes';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import { bennuRegexStore } from '$lib/stores/bennu/regex.svelte';
import { mount } from 'svelte';
import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
import { bennuHistoryStore } from '$lib/stores/bennu/history.svelte';
import { bennuRunConfigStore } from '$lib/stores/bennu/run-config.svelte';
import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

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

export const bennuExtras: ExtraScene[] = [
  { id: 'bennu/panels/BennuProblemsPanel', load: framed(() => import('$lib/components/bennu/BennuProblemsPanel.svelte'), 900, 320), setup: () => { seedProject(); bennuDiagnosticsStore.setProjectDiagnostics(FILE_DIAGS as never); }, settle: 800 },
  { id: 'bennu/panels/BennuBuildProblemsPanel', load: framed(() => import('$lib/components/bennu/BennuBuildProblemsPanel.svelte'), 900, 320), setup: () => { seedProject(); bennuDiagnosticsStore.setProjectDiagnostics(FILE_DIAGS as never); }, settle: 800 },
  { id: 'bennu/panels/BennuSidebar', load: framed(() => import('$lib/components/bennu/BennuSidebar.svelte'), 360, 700), setup: seedProject, settle: 1000 },
  { id: 'bennu/panels/BennuSymbolList', load: framed(() => import('$lib/components/bennu/BennuSymbolList.svelte'), 360, 600), setup: async () => { seedProject(); await projectStore.openFile(BANDO); }, settle: 1000 },
  { id: 'bennu/panels/BennuTodoPanel', load: framed(() => import('$lib/components/bennu/BennuTodoPanel.svelte'), 640, 360), setup: seedProject, settle: 800 },
  { id: 'bennu/panels/BennuMavenPanel', load: framed(() => import('$lib/components/bennu/BennuMavenPanel.svelte'), 420, 600), setup: seedProject, settle: 800 },
  { id: 'bennu/panels/BennuRunPanel', load: framed(() => import('$lib/components/bennu/BennuRunPanel.svelte'), 900, 320), setup: async () => { seedProject(); await bennuRunConfigStore.load(DEMO); }, settle: 800 },
  { id: 'bennu/panels/BennuDependenciesPanel', load: framed(() => import('$lib/components/bennu/BennuDependenciesPanel.svelte'), 560, 600), setup: seedProject, settle: 800 },
];
