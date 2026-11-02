/** Gallery scene overrides and extra scenes for the "picus" area — see gallery/registry.ts. */
import type { SceneOverride, ExtraScene } from './scenes';
import { studioBackend } from '$lib/ipc/studio/studio-format';
import { picusFixtures } from '../fixtures/picus';

/** Attach the sample script repository and the connections to the Picus stores. */
const seedPicus = async () => {
  const { connectionsStore } = await import('$lib/stores/picus/connections.svelte');
  await connectionsStore.load();
  const { picusProjectStore } = await import('$lib/stores/picus/project.svelte');
  await picusProjectStore.open('/Users/dev/work/pos-db');
};

/** Start the Imber store and open the demo conversation (it carries a pending edit proposal). */
const seedAssistant = async () => {
  const { bennuAssistantStore } = await import('$lib/stores/bennu/assistant.svelte');
  await bennuAssistantStore.start();
  await bennuAssistantStore.select('c-demo');
  return bennuAssistantStore;
};

const noop = () => {};
const POS_ROOT = '/Users/dev/work/pos-gateway';
const DTO_FILE = `${POS_ROOT}/src/main/java/it/banca/pos/dto/PagamentoDto.java`;
const YML_FILE = `${POS_ROOT}/src/main/resources/application.yml`;
const seedDemoProject = async () => {
  const { projectStore } = await import('$lib/stores/bennu/project.svelte');
  projectStore.loadDemo();
  return projectStore;
};

export const picusOverrides: Record<string, SceneOverride> = {
  // ── Bennu — tools (profiler, DTO lab, templates, new project) ───────────
  'bennu/new-project/BennuNewProjectModal': { settle: 900 },
  'bennu/profiler/ProfilerStartModal': { setup: async () => { await seedDemoProject(); } },
  'bennu/profiler/ProfilerThresholdsModal': { setup: async () => { await seedDemoProject(); } },
  'bennu/profiler/SamplyInstallModal': { settle: 700 },
  'bennu/dtolab/BennuValueRuleModal': {
    props: () => ({
      onClose: noop, previous: 'IBAN',
      rule: { name: 'IBAN', fields: ['iban', '*_iban'], constraints: ['Size', 'NotBlank'], value: 'IT60X0542811101000000123456', java: null, invalid: '', invalid_java: null },
    }),
  },
  'bennu/dtolab/DtoLabPreviewModal': {
    setup: async () => {
      const project = await seedDemoProject();
      await project.openFile(DTO_FILE);
      const { bennuDtoLabStore } = await import('$lib/stores/bennu/dtolab.svelte');
      bennuDtoLabStore.bindEditor({ caretContext: () => ({ source: 'public class PagamentoDto {}', offset: 10 }), applyEdits: noop });
      bennuDtoLabStore.openAtCaret();
      await new Promise((r) => setTimeout(r, 400));
      await bennuDtoLabStore.generate();
    },
    settle: 900,
  },
  'bennu/templates/BennuNewTemplateModal': { props: () => ({ kind: 'class', from: 'Getters and setters', onClose: noop, onCreated: noop }), settle: 700 },
  'bennu/templates/BennuTemplateGenerateModal': {
    setup: async () => {
      const store = await seedDemoProject();
      await store.openFile(DTO_FILE);
    },
    props: () => ({ kind: 'class', caretContext: () => ({ source: 'package it.banca.pos.dto;\n\npublic class PagamentoDto {\n    private String iban;\n}\n', offset: 60 }), applyEdits: noop, onClose: noop }),
    settle: 900,
  },
  'bennu/templates/BennuConfigClassModal': {
    setup: async () => {
      const store = await seedDemoProject();
      await store.openFile(YML_FILE);
    },
    props: () => ({ caretContext: () => ({ source: 'pos:\n  gateway:\n    host: 0.0.0.0\n', offset: 20 }), selection: () => null, onClose: noop }),
    settle: 900,
  },
  // ── Bennu — Imber assistant ─────────────────────────────────────────────
  'bennu/assistant/AssistantCompactModal': { setup: async () => (await seedAssistant()).openCompact() },
  'bennu/assistant/AssistantContextModal': { setup: async () => (await seedAssistant()).openContext() },
  'bennu/assistant/AssistantDecisionModal': { setup: async () => (await seedAssistant()).openDecision() },
  'bennu/assistant/BennuSkillsModal': { setup: async () => (await seedAssistant()).openSkills(), settle: 700 },
  'bennu/assistant/AssistantReviewModal': { setup: async () => (await seedAssistant()).openReview('c-demo', 'prop1'), settle: 700 },
  'bennu/assistant/AssistantAgentModal': {
    setup: async () => {
      await seedAssistant();
      const { bennuAgentsStore } = await import('$lib/stores/bennu/assistant-agents.svelte');
      await bennuAgentsStore.open('ag-1');
    },
    settle: 700,
  },
  'bennu/assistant/AssistantWorktreeModal': {
    setup: async () => {
      await seedAssistant();
      const { bennuWorktreeStore } = await import('$lib/stores/bennu/assistant-worktree.svelte');
      await bennuWorktreeStore.refreshInfo('c-demo');
      await bennuWorktreeStore.openDiff();
    },
    settle: 700,
  },
  'bennu/assistant/AssistantDocModal': {
    props: {
      onClose: () => {},
      text: `# OrdineService.totale()

Somma gli importi delle righe d'ordine.

- Input: \`List<RigaOrdine>\` (non null)
- Output: \`BigDecimal\`, scala 2

\`\`\`java
BigDecimal tot = service.totale(righe);
\`\`\`
`,
    },
  },

  // ── Picus (database workbench) ──────────────────────────────────────────
  'picus/AliasOfferModal': {
    props: () => ({
      onClose: () => {},
      offer: {
        kind: 'file', name: 'POS', engine: 'postgres', role: null, origin: 'POS/Init/001_schema.sql',
        folderPaths: ['POS', 'ORACLE/POS', 'Utility/POS'],
        filePaths: ['POS/Init/001_schema.sql', 'ORACLE/Update/POS_2.5.0.sql'],
        alternatives: ['schema', 'Init'],
      },
    }),
  },
  'picus/BindValuesModal': {
    setup: seedPicus,
    props: () => ({
      onClose: () => {},
      prompt: {
        tabId: 'tab-1', connectionId: 'conn-coll', scope: 'statement',
        slots: [{ label: ':CODICE_ABI', index: 1 }, { label: ':DATA_DA', index: 2 }, { label: ':STATO', index: 3 }],
      },
    }),
  },
  'picus/ClassifyFileModal': {
    setup: seedPicus, settle: 700,
    props: { path: 'Migrazione/MIGRAZIONE_2019.sql', onClose: () => {} },
  },
  'picus/ClassifyFolderModal': {
    setup: seedPicus, settle: 700,
    props: { path: 'Migrazione', onClose: () => {} },
  },
  'picus/generate/AddDestinationModal': { setup: seedPicus, settle: 700 },
  'picus/generate/SaveDestinationSetModal': { props: { initialName: 'Rilascio 2.7.0', onClose: () => {} } },
  'picus/panels/LobViewerModal': {
    props: {
      connectionId: 'conn-coll', table: 'POS_APP.DOCUMENTI_SCANSIONE', column: 'CONTENUTO',
      keys: { ID_DOC: '48213' }, onClose: () => {},
    },
  },
  'picus/PicusConnectionDetailsModal': { setup: seedPicus, props: { connectionId: 'conn-coll', onClose: () => {} } },
  'picus/PicusConnectionModal': { setup: seedPicus, props: { connectionId: 'conn-coll', onClose: () => {} } },

  // ── Studio (RON / JSON / TOML / YAML / .properties editors) ─────────────
  // Each wrapper renders only while its store is open, so the scene opens a sample document.
  'studio/JsonStudioModal': {
    setup: async () => { const { jsonStudioStore } = await import('$lib/stores/studio/json-studio.svelte'); await jsonStudioStore.openDoc({}); },
    settle: 900,
  },
  'studio/YamlStudioModal': {
    setup: async () => { const { yamlStudioStore } = await import('$lib/stores/studio/yaml-studio.svelte'); await yamlStudioStore.openDoc({}); },
    settle: 900,
  },
  'studio/TomlStudioModal': {
    setup: async () => { const { tomlStudioStore } = await import('$lib/stores/studio/toml-studio.svelte'); await tomlStudioStore.openDoc({}); },
    settle: 900,
  },
  'studio/PropertiesStudioModal': {
    setup: async () => { const { propertiesStudioStore } = await import('$lib/stores/studio/properties-studio.svelte'); await propertiesStudioStore.openDoc({}); },
    settle: 900,
  },
  'studio/RonStudioModal': {
    setup: async () => { const { ronStudioStore } = await import('$lib/stores/studio/ron-studio.svelte'); await ronStudioStore.openDoc({}); },
    settle: 900,
  },
  'studio/StudioModal': { skip: 'Generic shell (props: backend, snippets, config) — only rendered through the Ron/Json/Toml/Yaml/Properties wrappers, which have their own scenes.' },
  'studio/BindSchemaModal': {
    props: () => ({
      formatId: 'ron', relativePath: 'assets/items/swords.ron', fileName: 'swords.ron', targetKind: 'file',
      initial: { rs_file: 'crates/items/src/item.rs', root_type: 'items::item::ItemDef', reference_fields: ['id'] },
      onSave: () => {}, onClose: () => {},
    }),
  },
  'studio/StudioViewSourceModal': {
    props: () => {
      const h = picusFixtures['studio.studio_schema_view_source'] as (p: Record<string, unknown>) => unknown;
      return { viewSource: h({ canonical_path: 'items::item::ItemDef' }), busy: false, err: null, language: 'rust', onClose: () => {} };
    },
  },
  'studio/StudioConvertPreviewModal': {
    props: {
      mode: 'properties-to-yaml', defaultFilename: 'application.yml', onReplace: () => {}, onClose: () => {},
      sourceText: 'server.port=8443\nserver.servlet.context-path=/pos\nspring.datasource.url=jdbc:oracle:thin:@db-coll.banca.local:1521/ORCLPDB1\nspring.datasource.username=POS_APP\nlogging.level.root=INFO\n',
    },
    settle: 700,
  },
  'studio/StudioRenameModal': {
    props: () => {
      return {
        backend: studioBackend('ron'), tabId: 'tab-1', formatLabel: 'RON', oldValue: 'iron_sword',
        openDocs: [{ doc_id: 'doc-1', source_path: '/Users/dev/work/game/assets/items/swords.ron', dirty: false }],
        onClose: () => {}, onApplied: () => {},
      };
    },
    settle: 700,
  },
  'studio/StudioBulkEditModal': {
    props: () => {
      return {
        backend: studioBackend('ron'), tabId: 'tab-1', docId: 'doc-1', formatLabel: 'RON', query: '$..durability',
        nullPolicy: 'native', openDocs: [], onClose: () => {}, onApplied: () => {},
      };
    },
    settle: 700,
  },

  // ── Tyto (screen recorder) ──────────────────────────────────────────────
  'tyto/TytoSettingsModal': { setup: seedTyto, settle: 700 },
  'tyto/TytoAboutModal': { settle: 500 },

  // ── Sitta (file explorer as a picker) ───────────────────────────────────
  'sitta/FileExplorerModal': {
    props: () => ({ mode: 'file', title: 'Open SQL script', initialPath: '/Users/dev/work/pos-db', extensions: ['sql'], onConfirm: () => {}, onCancel: () => {}, onClose: () => {} }),
    settle: 1200,
  },
};

/** Boot the Tyto store against the mocked backend (config, sources, library). */
async function seedTyto() {
  const { recorderStore } = await import('$lib/stores/tyto/recorder.svelte');
  recorderStore.initBackend();
}

const sleepMs = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const picusExtras: ExtraScene[] = [
  {
    id: 'tyto/panels/RecordingsPanel',
    load: () => import('$lib/components/tyto/panels/RecordingsPanel.svelte'),
    setup: async () => { await seedTyto(); await sleepMs(300); }, settle: 900,
  },
  {
    id: 'tyto/panels/CapturePanel',
    load: () => import('$lib/components/tyto/panels/CapturePanel.svelte'),
    setup: seedTyto, settle: 900,
  },
  {
    id: 'sitta/FileExplorerBatchRename',
    load: () => import('$lib/components/sitta/FileExplorerBatchRename.svelte'),
    props: () => ({
      onCancel: noop, onApply: noop,
      items: ['IMG_4821.png', 'IMG_4822.png', 'IMG_4823.png', 'Screenshot 2026-10-30 at 09.14.02.png', 'schema v2 (copy).png'].map((name) => ({ name, path: `/Users/dev/Pictures/${name}` })),
    }),
  },
  {
    id: 'sitta/FileExplorerModal-browse',
    load: () => import('$lib/components/sitta/FileExplorerModal.svelte'),
    props: () => ({ standalone: true, mode: null, initialPath: '/Users/dev/work', onClose: noop }),
    settle: 1500,
  },
  {
    id: 'bennu/profiler/BennuProfilerPanel',
    load: () => import('$lib/components/bennu/profiler/BennuProfilerPanel.svelte'),
    setup: async () => {
      await seedDemoProject();
      const { bennuProfilerStore } = await import('$lib/stores/bennu/profiler.svelte');
      await bennuProfilerStore.refresh();
      await bennuProfilerStore.select('rec-1');
    },
    settle: 1500,
  },
  {
    id: 'bennu/dtolab/BennuDtoLabPanel',
    load: () => import('$lib/components/bennu/dtolab/BennuDtoLabPanel.svelte'),
    setup: async () => {
      const project = await seedDemoProject();
      await project.openFile(DTO_FILE);
      const { bennuDtoLabStore } = await import('$lib/stores/bennu/dtolab.svelte');
      bennuDtoLabStore.bindEditor({ caretContext: () => ({ source: 'public class PagamentoDto {}', offset: 10 }), applyEdits: noop });
      bennuDtoLabStore.openAtCaret();
    },
    settle: 1500,
  },
  {
    id: 'bennu/assistant/BennuAssistantPanel',
    load: () => import('$lib/components/bennu/assistant/BennuAssistantPanel.svelte'),
    setup: async () => { await seedDemoProject(); await seedAssistant(); },
    settle: 1500,
  },
  {
    id: 'bennu/assistant/AssistantHome',
    load: () => import('$lib/components/bennu/assistant/AssistantHome.svelte'),
    setup: async () => { await seedDemoProject(); const { bennuAssistantStore } = await import('$lib/stores/bennu/assistant.svelte'); await bennuAssistantStore.start(); },
    settle: 900,
  },
  {
    id: 'picus/panels/ConnectionsPanel',
    load: () => import('$lib/components/picus/panels/ConnectionsPanel.svelte'),
    setup: seedPicus, settle: 900,
  },
];
