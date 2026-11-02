/** Mock answers for the "picus" area — see fixtures/index.ts. Keys: Tauri command, or `program.method` for rpc. */
import type { FixtureHandler } from './index';

// ── Picus: engines, connections, a script repository ────────────────────────
const field = (id: string, label: string, type: string, extra: Record<string, unknown> = {}) => ({
  id, label, type, required: false, ...extra,
});

const caps = (oracle: boolean) => ({
  connect: true, sequences: true, materializedViews: true, packages: oracle, insteadOfTriggers: true,
  bitmapIndexes: oracle, expressionIndexes: true, cancelQuery: true, estimatedRows: true, schemas: !oracle,
  sessionActivity: true, explain: true, validate: true, bindParameters: true, dependencyGraph: true,
  transactions: { supported: true, transactionalDdl: !oracle, savepoints: true },
});

const PROVIDERS = [
  {
    kind: 'oracle', label: 'Oracle Database', shortLabel: 'ORA', colorVar: '--picus-oracle', defaultPort: 1521,
    fields: [
      field('host', 'Host', 'text', { required: true, placeholder: 'db-coll.banca.local' }),
      field('port', 'Port', 'number', { default: '1521', min: 1, max: 65535 }),
      field('database', 'Service name', 'text', { required: true, placeholder: 'ORCLPDB1' }),
      field('user', 'Username', 'text', { required: true, placeholder: 'POS_APP' }),
      field('password', 'Password', 'secret'),
      field('schema', 'Schema', 'text', { default: 'POS_APP', help: 'Where unqualified names resolve.' }),
      field('tls', 'Require TLS', 'toggle'),
      field('nls_lang', 'NLS_LANG', 'text', { placeholder: 'ITALIAN_ITALY.WE8MSWIN1252', help: 'Client character set.' }),
    ],
    capabilities: caps(true),
    emission: {
      blockOpen: 'BEGIN', blockClose: 'END;', statementTerminator: ';', nowFunction: 'SYSDATE',
      upsertForm: 'MERGE', objectExistsCheck: 'user_objects', identifierCase: 'upper', ddlCommitsImplicitly: true,
    },
    schemaGroups: ['tables', 'views', 'sequences', 'triggers'],
  },
  {
    kind: 'postgres', label: 'PostgreSQL', shortLabel: 'PG', colorVar: '--picus-postgres', defaultPort: 5432,
    fields: [
      field('host', 'Host', 'text', { required: true, placeholder: 'localhost' }),
      field('port', 'Port', 'number', { default: '5432', min: 1, max: 65535 }),
      field('database', 'Database', 'text', { required: true, placeholder: 'appdb' }),
      field('user', 'Username', 'text', { required: true, placeholder: 'postgres' }),
      field('password', 'Password', 'secret'),
      field('schema', 'Schema', 'text', { default: 'public' }),
      field('tls', 'Require TLS', 'toggle'),
      field('application_name', 'Application name', 'text', { default: 'arbor-picus' }),
    ],
    capabilities: caps(false),
    emission: {
      blockOpen: 'DO $$ BEGIN', blockClose: 'END $$;', statementTerminator: ';', nowFunction: 'now()',
      upsertForm: 'ON CONFLICT', objectExistsCheck: 'to_regclass', identifierCase: 'lower', ddlCommitsImplicitly: false,
    },
    schemaGroups: ['tables', 'views', 'sequences', 'triggers'],
  },
];

const CONNECTIONS = [
  {
    id: 'conn-coll', name: 'Collaudo ISP', alias: 'coll', engine: 'oracle', host: 'db-coll.banca.local', port: 1521,
    database: 'ORCLPDB1', user: 'POS_APP', schema: 'POS_APP', colorIdx: 2, readOnly: false, tls: false,
    scriptRoot: '/Users/dev/work/pos-db', params: { nls_lang: 'ITALIAN_ITALY.WE8MSWIN1252' },
    state: 'connected', serverVersion: 'Oracle Database 19c Enterprise Edition 19.21.0.0.0', hasSecret: true,
  },
  {
    id: 'conn-prod', name: 'Produzione (sola lettura)', alias: 'prod', engine: 'oracle', host: 'db-prod.banca.local',
    port: 1521, database: 'PRODPDB', user: 'POS_RO', schema: 'POS_APP', colorIdx: 0, readOnly: true, tls: true,
    scriptRoot: '/Users/dev/work/pos-db', params: {},
    state: 'read-only', serverVersion: 'Oracle Database 19c Enterprise Edition 19.21.0.0.0', hasSecret: true,
  },
  {
    id: 'conn-local', name: 'Locale (docker)', alias: 'loc', engine: 'postgres', host: 'localhost', port: 5432,
    database: 'pos', user: 'postgres', schema: 'public', colorIdx: 5, readOnly: false, tls: false, params: {},
    state: 'disconnected', serverVersion: '', hasSecret: false,
  },
];

const sqlFile = (path: string, size: number, engine: string | null, extra: Record<string, unknown> = {}) => ({
  path, name: path.split('/').pop()!, size, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'CRLF',
  expectedEncoding: 'windows-1252', engine, effectiveEngine: engine ?? 'oracle', excluded: null,
  effectiveExcluded: false, ...extra,
});
const folder = (path: string, over: Record<string, unknown> = {}) => ({
  path, name: path.split('/').pop()!, engine: null, role: null, effectiveEngine: 'oracle', effectiveRole: 'update',
  excluded: null, effectiveExcluded: false, product: null, effectiveProduct: null, children: [], files: [], ...over,
});

const PROJECT_TREE = [
  folder('ORACLE', {
    engine: 'oracle', effectiveEngine: 'oracle',
    children: [
      folder('ORACLE/Init', {
        role: 'init', effectiveRole: 'init', engine: 'oracle',
        files: [
          sqlFile('ORACLE/Init/001_create_tables.sql', 48211, null),
          sqlFile('ORACLE/Init/002_sequences.sql', 3120, null),
          sqlFile('ORACLE/Init/003_indexes.sql', 9876, null),
        ],
      }),
      folder('ORACLE/Update', {
        role: 'update', effectiveRole: 'update',
        files: [
          sqlFile('ORACLE/Update/2.4.0_to_2.5.0.sql', 12044, null, { status: 'modified' }),
          sqlFile('ORACLE/Update/2.5.0_to_2.6.0.sql', 8120, null, { status: 'new' }),
          sqlFile('ORACLE/Update/2.6.0_to_2.7.0.sql', 15390, null),
        ],
      }),
      folder('ORACLE/Procedure', {
        role: 'routines', effectiveRole: 'routines',
        files: [sqlFile('ORACLE/Procedure/PKG_POS_CALC.sql', 22100, null), sqlFile('ORACLE/Procedure/PRC_CHIUSURA.sql', 6402, null)],
      }),
    ],
  }),
  folder('POS', {
    engine: 'postgres', effectiveEngine: 'postgres',
    children: [
      folder('POS/Init', { role: 'init', effectiveRole: 'init', effectiveEngine: 'postgres',
        files: [sqlFile('POS/Init/001_schema.sql', 30211, 'postgres', { effectiveEngine: 'postgres', encoding: 'UTF-8', encodingSource: 'utf8', eol: 'LF', expectedEncoding: 'UTF-8' })] }),
      folder('POS/Update', { role: 'update', effectiveRole: 'update', effectiveEngine: 'postgres',
        files: [sqlFile('POS/Update/2.6.0_to_2.7.0.sql', 7340, null, { effectiveEngine: 'postgres', encoding: 'UTF-8', encodingSource: 'utf8', eol: 'LF', expectedEncoding: 'UTF-8' })] }),
    ],
  }),
  folder('Migrazione', { effectiveEngine: null, effectiveRole: 'ignored', files: [sqlFile('Migrazione/MIGRAZIONE_2019.sql', 98211, null, { effectiveEngine: null })] }),
  folder('Utility', { effectiveEngine: 'generic', engine: 'generic', files: [sqlFile('Utility/pulizia_log.sql', 1220, null, { effectiveEngine: 'generic' })] }),
];

const SAMPLE_PDF_B64 = btoa('%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF');

// ── Studio: a tiny in-memory document backend ───────────────────────────────
// One JS value per format drives the whole tree (`get_root` / `get_children` / `get_value`),
// so every Studio modal shows a believable document without a real parser behind it.
type Fmt = 'json' | 'yaml' | 'toml' | 'ron' | 'properties';

const SAMPLES: Record<Fmt, { path: string; text: string; value: unknown }> = {
  json: {
    path: '/Users/dev/work/pos-gateway/config/gateway.json',
    value: {
      service: 'pos-gateway', version: '2.7.0', debug: false,
      server: { host: '0.0.0.0', port: 8443, tls: { enabled: true, keystore: 'certs/gateway.p12', protocols: ['TLSv1.3', 'TLSv1.2'] } },
      datasource: { url: 'jdbc:oracle:thin:@db-coll.banca.local:1521/ORCLPDB1', username: 'POS_APP', poolSize: 24, timeoutMs: 30000 },
      routes: [
        { id: 'authorize', path: '/v1/authorize', upstream: 'http://auth:8080', retries: 2 },
        { id: 'settle', path: '/v1/settle', upstream: 'http://settle:8080', retries: 0 },
      ],
      features: { fraudCheck: true, instantRefund: null, maxAmount: 25000.5 },
    },
    text: '',
  },
  yaml: {
    path: '/Users/dev/work/pos-gateway/.github/workflows/ci.yml',
    value: {
      name: 'CI', on: { push: { branches: ['main', 'release/*'] }, pull_request: null },
      env: { JAVA_VERSION: '17', MAVEN_OPTS: '-Xmx2g' },
      jobs: {
        build: {
          'runs-on': 'ubuntu-22.04', 'timeout-minutes': 20,
          steps: [
            { uses: 'actions/checkout@v4' },
            { name: 'Set up JDK', uses: 'actions/setup-java@v4', with: { 'java-version': '17', distribution: 'temurin' } },
            { name: 'Test', run: 'mvn -B verify' },
          ],
        },
      },
    },
    text: `name: CI
on:
  push:
    branches: [main, "release/*"]
  pull_request:
env:
  JAVA_VERSION: "17"
  MAVEN_OPTS: -Xmx2g
jobs:
  build:
    runs-on: ubuntu-22.04
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - name: Set up JDK
        uses: actions/setup-java@v4
        with:
          java-version: "17"
          distribution: temurin
      - name: Test
        run: mvn -B verify
`,
  },
  toml: {
    path: '/Users/dev/work/pos-engine/Cargo.toml',
    value: {
      package: { name: 'pos-engine', version: '0.9.3', edition: '2021', authors: ['Team POS'] },
      dependencies: { serde: { version: '1', features: ['derive'] }, tokio: { version: '1.38', features: ['rt-multi-thread', 'macros'] }, anyhow: '1' },
      profile: { release: { lto: true, 'opt-level': 3, 'codegen-units': 1 } },
    },
    text: `[package]
name = "pos-engine"
version = "0.9.3"
edition = "2021"
authors = ["Team POS"]

[dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1.38", features = ["rt-multi-thread", "macros"] }
anyhow = "1"

[profile.release]
lto = true
opt-level = 3
codegen-units = 1
`,
  },
  properties: {
    path: '/Users/dev/work/pos-gateway/src/main/resources/application.properties',
    value: {
      server: { port: '8443', servlet: { 'context-path': '/pos' } },
      spring: { datasource: { url: 'jdbc:oracle:thin:@db-coll.banca.local:1521/ORCLPDB1', username: 'POS_APP', password: '${DB_PASSWORD}' },
        jpa: { 'show-sql': 'false', 'hibernate.ddl-auto': 'validate' } },
      logging: { level: { root: 'INFO', 'it.banca.pos': 'DEBUG' } },
    },
    text: `server.port=8443
server.servlet.context-path=/pos
spring.datasource.url=jdbc:oracle:thin:@db-coll.banca.local:1521/ORCLPDB1
spring.datasource.username=POS_APP
spring.datasource.password=\${DB_PASSWORD}
spring.jpa.show-sql=false
spring.jpa.hibernate.ddl-auto=validate
logging.level.root=INFO
logging.level.it.banca.pos=DEBUG
`,
  },
  ron: {
    path: '/Users/dev/work/game/assets/items/swords.ron',
    value: {
      id: 'iron_sword', display_name: 'Iron Sword', rarity: 'Common', damage: 12.5, durability: 240, stackable: false,
      tags: ['weapon', 'melee'], effects: [{ kind: 'Bleed', chance: 0.15 }, { kind: 'Stun', chance: 0.05 }],
      requirements: { level: 3, strength: 8 }, icon: null,
    },
    text: `(
    id: "iron_sword",
    display_name: "Iron Sword",
    rarity: Common,
    damage: 12.5,
    durability: 240,
    stackable: false,
    tags: ["weapon", "melee"],
    effects: [
        (kind: Bleed, chance: 0.15),
        (kind: Stun, chance: 0.05),
    ],
    requirements: (level: 3, strength: 8),
    icon: None,
)
`,
  },
};
SAMPLES.json.text = JSON.stringify(SAMPLES.json.value, null, 2) + '\n';

const isObj = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const childKeys = (v: unknown): string[] =>
  Array.isArray(v) ? v.map((_, i) => String(i)) : isObj(v) ? Object.keys(v) : [];

function kindOf(fmt: Fmt, v: unknown): string {
  if (v === null) return fmt === 'ron' ? 'option' : 'null';
  if (Array.isArray(v)) {
    return fmt === 'ron' ? 'list' : fmt === 'toml' ? (v.length && v.every(isObj) ? 'array_of_tables' : 'array') : 'array';
  }
  if (isObj(v)) return fmt === 'ron' ? 'named_struct' : fmt === 'toml' ? 'table' : 'object';
  if (typeof v === 'boolean') return 'bool';
  if (typeof v === 'number') {
    if (fmt === 'json' || fmt === 'ron') return 'number';
    return Number.isInteger(v) ? 'integer' : 'float';
  }
  return 'string';
}

const previewOf = (v: unknown): string => {
  if (Array.isArray(v)) return `[${v.length}]`;
  if (isObj(v)) return `{${Object.keys(v).length}}`;
  return v === null ? 'null' : typeof v === 'string' ? JSON.stringify(v) : String(v);
};

const at = (root: unknown, path: string[]): unknown =>
  path.reduce<unknown>((cur, k) => (Array.isArray(cur) ? cur[Number(k)] : isObj(cur) ? cur[k] : undefined), root);

const view = (fmt: Fmt, root: unknown, key: string, path: string[]) => {
  const v = at(root, path);
  return {
    key, path, kind: kindOf(fmt, v), preview: previewOf(v), child_count: childKeys(v).length,
    variant_tag: fmt === 'ron' && isObj(v) && typeof v.kind === 'string' ? v.kind : null,
  };
};

const docs = new Map<string, { fmt: Fmt; text: string }>();
let docSeq = 1;

const walk = (fmt: Fmt, root: unknown, path: string[], out: ReturnType<typeof view>[]) => {
  const v = at(root, path);
  for (const k of childKeys(v)) {
    out.push(view(fmt, root, k, [...path, k]));
    walk(fmt, root, [...path, k], out);
  }
};

const FORMAT_LABEL: Record<Fmt, string> = { json: 'JSON', yaml: 'YAML', toml: 'TOML', ron: 'RON', properties: 'Properties' };
const KIND_TONE: Record<string, string> = {
  object: 'accent', array: 'accent', table: 'accent', inline_table: 'accent', array_of_tables: 'accent',
  struct: 'accent', named_struct: 'accent', list: 'accent', map: 'accent', tuple: 'accent', named_tuple: 'accent',
  string: 'success', number: 'info', integer: 'info', float: 'info', bool: 'warning', null: 'muted', option: 'muted', unit: 'muted',
};

const describe = (fmt: Fmt) => ({
  id: fmt, label: FORMAT_LABEL[fmt], file_extensions: [fmt === 'yaml' ? 'yml' : fmt],
  icon: { type: 'iconify', name: 'lucide:file-code' },
  supports_lossless_edit: fmt !== 'json', supports_comments: fmt !== 'json', supports_anchors: fmt === 'yaml',
  null_handling: fmt === 'properties' ? 'as_delete' : 'native',
  supports_streaming_mode: fmt === 'json', streaming_threshold_kb: fmt === 'json' ? 5120 : null,
  streaming_setting_key: null, query_syntax: 'json_path',
  cross_ref_default_fields: ['id'], cross_ref_scopes: fmt === 'properties' ? ['key', 'value'] : ['value'],
  schema_sources: fmt === 'json' ? ['json_schema'] : fmt === 'ron' ? ['rust_struct'] : ['rust_struct', 'json_schema'],
  kind_palette: Object.fromEntries(Object.entries(KIND_TONE).map(([k, tone]) => [k, { label: k.replace('_', ' '), tone }])),
  save_warnings: fmt === 'json' ? ['jsonc_comments_in_json'] : [], save_behavior_setting_key: null,
  convert_to_json_supported: true, supports_external_files: true,
  supports_rename_reference: true, supports_bulk_edit: true,
});

/** Resolve the doc a call talks about, defaulting to a fresh sample of the format. */
const docOf = (p: Record<string, unknown>) => {
  const d = docs.get(String(p.doc_id));
  const fmt = (d?.fmt ?? p.format_id ?? 'json') as Fmt;
  return { fmt, text: d?.text ?? SAMPLES[fmt].text, value: SAMPLES[fmt].value };
};

const mutateResult = (p: Record<string, unknown>) => {
  const { fmt, text, value } = docOf(p);
  return { text, parse_error: null, root_kind: kindOf(fmt, value), child_count: childKeys(value).length, can_undo: false, can_redo: false, has_jsonc_features: false };
};

const studioFixtures: Record<string, FixtureHandler> = {
  'studio.studio_describe': (p) => describe(p.format_id as Fmt),
  'studio.studio_list_formats': (['json', 'yaml', 'toml', 'ron', 'properties'] as Fmt[]).map(describe),
  'studio.studio_parse': (p) => {
    const fmt = p.format_id as Fmt;
    const s = SAMPLES[fmt];
    const id = `doc-${docSeq++}`;
    const text = typeof p.text === 'string' && p.text ? p.text : s.text;
    docs.set(id, { fmt, text });
    return {
      doc_id: id, size_bytes: new TextEncoder().encode(text).length, source_path: (p.path as string) ?? s.path, original: text,
      parse_error: null, root_kind: kindOf(fmt, s.value), child_count: childKeys(s.value).length, schema_hint: null,
      encoding: { label: 'UTF-8', had_bom: false }, stream_mode: false, is_jsonc: false, has_jsonc_features: false,
    };
  },
  'studio.studio_close': null,
  'studio.studio_get_encoding': { label: 'UTF-8', had_bom: false },
  'studio.studio_get_root': (p) => { const { fmt, value } = docOf(p); return view(fmt, value, '', []); },
  'studio.studio_get_children': (p) => {
    const { fmt, value } = docOf(p);
    const path = (p.path as string[]) ?? [];
    return childKeys(at(value, path)).map((k) => view(fmt, value, k, [...path, k]));
  },
  'studio.studio_get_value': (p) => {
    const { value } = docOf(p);
    const v = at(value, (p.path as string[]) ?? []);
    return typeof v === 'string' ? v : JSON.stringify(v, null, 2);
  },
  'studio.studio_query': (p) => {
    const { fmt, value } = docOf(p);
    const all: ReturnType<typeof view>[] = [];
    walk(fmt, value, [], all);
    const needle = String(p.expr ?? '').replace(/[$.*\[\]'"]/g, '').toLowerCase();
    return all
      .filter((n) => !needle || n.key.toLowerCase().includes(needle) || n.preview.toLowerCase().includes(needle))
      .slice(0, 40)
      .map(({ path, kind, preview, variant_tag }) => ({ path, kind, preview, variant_tag }));
  },
  'studio.studio_raw_original': (p) => docOf(p).text,
  'studio.studio_raw_current': (p) => docOf(p).text,
  'studio.studio_format': (p) => docOf(p).text,
  'studio.studio_to_json': (p) => JSON.stringify(docOf(p).value, null, 2),
  'studio.studio_get_indent': (p) => (docOf(p).fmt === 'ron' ? '    ' : '  '),
  'studio.studio_set_indent': null,
  'studio.studio_set_text': (p) => ({ ...mutateResult(p), text: undefined }),
  'studio.studio_apply_mutation': mutateResult,
  'studio.studio_strip_features': mutateResult,
  'studio.studio_undo': mutateResult,
  'studio.studio_redo': mutateResult,
  'studio.studio_history_state': [false, false],
  'studio.studio_source_path': (p) => SAMPLES[docOf(p).fmt].path,
  'studio.studio_diff': [],
  'studio.studio_snapshot': (p) => {
    const { fmt, text, value } = docOf(p);
    return {
      doc_id: p.doc_id, source_path: SAMPLES[fmt].path, size_bytes: text.length, original: text, current: text,
      parse_error: null, root_kind: kindOf(fmt, value), child_count: childKeys(value).length, can_undo: false, can_redo: false,
      indent: fmt === 'ron' ? '    ' : '  ',
    };
  },
  'studio.studio_save': null,
  'studio.studio_list_files': (p) => {
    const f = (p.folder as string) ?? '/Users/dev/work/game/assets';
    return ['swords.ron', 'armor.ron', 'potions.ron', 'monsters.ron'].map((n, i) => ({
      absolute_path: `${f}/${n}`, relative_path: `items/${n}`, name: n, size_bytes: 1800 + i * 740,
    }));
  },
  'studio.studio_schema_probe': {
    crate_manifest: '/Users/dev/work/game/crates/items/Cargo.toml', crate_name: 'items',
    root_candidates: [
      { name: 'ItemDef', canonical_path: 'items::item::ItemDef', kind: 'struct' },
      { name: 'Rarity', canonical_path: 'items::item::Rarity', kind: 'enum' },
      { name: 'LootTable', canonical_path: 'items::loot::LootTable', kind: 'struct' },
    ],
  },
  'studio.studio_schema_view_source': (p) => ({
    canonical_path: p.canonical_path, name: String(p.canonical_path).split('::').pop(), kind: 'struct',
    source: `/// A single item as authored in assets/items/*.ron.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDef {
    pub id: String,
    pub display_name: String,
    pub rarity: Rarity,
    pub damage: f32,
    pub durability: u32,
    #[serde(default)]
    pub stackable: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    pub effects: Vec<Effect>,
    pub requirements: Requirements,
    pub icon: Option<String>,
}
`,
  }),
  'studio.studio_rename_preview': (p) => ({
    sites: [
      { absolute_path: '/Users/dev/work/game/assets/items/swords.ron', relative_path: 'items/swords.ron', file_name: 'swords.ron', field_path: ['id'], key_name: 'id', scope: 'definition', preview: `id: "${p.old_value}",` },
      { absolute_path: '/Users/dev/work/game/assets/loot/chest_tier1.ron', relative_path: 'loot/chest_tier1.ron', file_name: 'chest_tier1.ron', field_path: ['entries', '0', 'item'], key_name: 'item', scope: 'reference', preview: `(item: "${p.old_value}", weight: 40),` },
      { absolute_path: '/Users/dev/work/game/assets/loot/chest_tier1.ron', relative_path: 'loot/chest_tier1.ron', file_name: 'chest_tier1.ron', field_path: ['entries', '3', 'item'], key_name: 'item', scope: 'reference', preview: `(item: "${p.old_value}", weight: 5),` },
      { absolute_path: '/Users/dev/work/game/assets/shops/blacksmith.ron', relative_path: 'shops/blacksmith.ron', file_name: 'blacksmith.ron', field_path: ['stock', '1'], key_name: 'stock', scope: 'reference', preview: `"${p.old_value}",` },
    ],
    dirty_blockers: [], collisions: [],
  }),
  'studio.studio_rename_apply': (p) => ({ written_files: [...new Set(((p.sites as { absolute_path: string }[]) ?? []).map((s) => s.absolute_path))], failed_files: [] }),
  'studio.studio_bulk_edit_preview': () => ({
    sites: [
      { absolute_path: '/Users/dev/work/game/assets/items/swords.ron', relative_path: 'items/swords.ron', file_name: 'swords.ron', field_path: ['durability'], kind: 'number', old_preview: '240', new_preview: '300', will_skip: false },
      { absolute_path: '/Users/dev/work/game/assets/items/armor.ron', relative_path: 'items/armor.ron', file_name: 'armor.ron', field_path: ['durability'], kind: 'number', old_preview: '180', new_preview: '300', will_skip: false },
      { absolute_path: '/Users/dev/work/game/assets/items/potions.ron', relative_path: 'items/potions.ron', file_name: 'potions.ron', field_path: ['durability'], kind: 'number', old_preview: '1', new_preview: '300', will_skip: true, skip_reason: 'stackable item: durability is ignored' },
      { absolute_path: '/Users/dev/work/game/assets/items/monsters.ron', relative_path: 'items/monsters.ron', file_name: 'monsters.ron', field_path: ['durability'], kind: 'number', old_preview: '95', new_preview: '300', will_skip: false },
    ],
    dirty_blockers: [], expression_error: null,
  }),
  'studio.studio_bulk_edit_apply': { written_files: [], failed_files: [], applied_sites: 3, skipped_sites: 1, active_doc_state: null },
  'studio.studio_properties_to_yaml': {
    yaml_text: `server:
  port: "8443"
  servlet:
    context-path: /pos
spring:
  datasource:
    url: jdbc:oracle:thin:@db-coll.banca.local:1521/ORCLPDB1
    username: POS_APP
    password: \${DB_PASSWORD}
logging:
  level:
    root: INFO
    it.banca.pos: DEBUG
`,
    warnings: ['3 numeric-looking values were kept as strings (server.port, spring.jpa.show-sql, logging.level.root).'],
  },
  'studio.studio_yaml_to_properties': {
    properties_text: SAMPLES.properties.text,
    warnings: ['Sequence "on.push.branches" was flattened to indexed keys.'],
  },
};

// ── Bennu "Imber" assistant ─────────────────────────────────────────────────
const NOW_MS = Date.now();
const ln = (kind: 'context' | 'add' | 'del', text: string, o?: number, n?: number) => ({ kind, text, old: o, new: n });

const DEMO_HUNKS = [
  {
    old_start: 41, new_start: 41,
    lines: [
      ln('context', '    public BigDecimal totale(List<RigaOrdine> righe) {', 41, 41),
      ln('del', '        BigDecimal t = BigDecimal.ZERO;', 42),
      ln('del', '        for (RigaOrdine r : righe) t = t.add(r.getImporto());', 43),
      ln('add', '        return righe.stream()', undefined, 42),
      ln('add', '                .map(RigaOrdine::getImporto)', undefined, 43),
      ln('add', '                .reduce(BigDecimal.ZERO, BigDecimal::add);', undefined, 44),
      ln('del', '        return t;', 44),
      ln('context', '    }', 45, 45),
    ],
  },
];

const DEMO_CONVERSATION = {
  id: 'c-demo', title: 'Semplifica il calcolo del totale ordine', updatedAt: NOW_MS - 120000, model: 'claude-sonnet-4-5',
  cost: 0.4132, context: { system: 6200, project: 14800, conversation: 9100, tools: 3400, instructions: 800, skills: 1200 },
  messages: [
    { id: 'm1', role: 'user', at: NOW_MS - 600000, blocks: [{ type: 'text', text: 'Il metodo totale() in OrdineService è un po\' datato: puoi semplificarlo e controllare chi lo chiama?' }] },
    {
      id: 'm2', role: 'assistant', at: NOW_MS - 590000, model: 'claude-sonnet-4-5',
      blocks: [
        { type: 'text', text: 'Guardo prima **OrdineService** e i chiamanti, poi propongo la modifica.' },
        { type: 'tool', call: { id: 't1', kind: 'read', label: 'Read OrdineService.java', detail: 'src/main/java/it/banca/pos/OrdineService.java · 214 lines', ok: true } },
        { type: 'tool', call: { id: 't2', kind: 'search', label: 'Find callers of totale()', detail: '7 references in 4 files', ok: true } },
        { type: 'plan', plan: { id: 'p1', steps: [
          { text: 'Sostituire il ciclo con uno stream', status: 'completed' },
          { text: 'Verificare i 7 chiamanti', status: 'completed' },
          { text: 'Eseguire i test di OrdineServiceTest', status: 'in_progress' },
        ] } },
        { type: 'text', text: 'Ho trovato 7 chiamanti, nessuno dipende dall\'ordine di iterazione. Ecco la modifica:' },
        { type: 'proposal', proposal: { id: 'prop1', status: 'pending', files: [
          { path: 'src/main/java/it/banca/pos/OrdineService.java', added: 3, removed: 3, hunks: DEMO_HUNKS },
          { path: 'src/test/java/it/banca/pos/OrdineServiceTest.java', added: 12, removed: 0, hunks: DEMO_HUNKS },
        ] } },
        { type: 'notice', level: 'warning', text: 'Rate limit raggiunto, nuovo tentativo tra 8 s', detail: '429 Too Many Requests', retryable: true },
      ],
    },
  ],
};

const DEMO_AGENT = {
  id: 'ag-1', conversationId: 'c-demo', runId: 'run-1', title: 'Cerca i chiamanti di totale()', status: 'done', tools: 'read',
  model: 'claude-haiku-4-5', cost: 0.0214, startedAt: NOW_MS - 500000, endedAt: NOW_MS - 470000,
  task: 'Trova tutti i chiamanti di OrdineService.totale() e segnala quelli che dipendono dall\'ordine di iterazione.',
  summary: '7 chiamanti in 4 file. Nessuno dipende dall\'ordine: OrdineController (2), FatturaBatch (3), RiepilogoJob (1), OrdineServiceTest (1).', error: null,
};

const assistantFixtures: Record<string, FixtureHandler> = {
  'bennu.assistant_models': {
    models: [
      { id: 'claude-sonnet-4-5', name: 'Sonnet 4.5', tagline: 'Balanced', contextMax: 200000, efforts: ['low', 'medium', 'high'], inputPrice: 3, outputPrice: 15 },
      { id: 'claude-haiku-4-5', name: 'Haiku 4.5', tagline: 'Fast & cheap', contextMax: 200000, efforts: [], inputPrice: 1, outputPrice: 5 },
      { id: 'claude-opus-4-5', name: 'Opus 4.5', tagline: 'Most capable', contextMax: 200000, efforts: ['medium', 'high', 'max'], inputPrice: 5, outputPrice: 25 },
    ],
    file: '/Users/dev/.config/arbor/bennu-models.toml', error: null,
  },
  'bennu.assistant_list': [
    { id: 'c-demo', title: DEMO_CONVERSATION.title, updatedAt: DEMO_CONVERSATION.updatedAt, model: 'claude-sonnet-4-5', cost: 0.4132, context: DEMO_CONVERSATION.context },
    { id: 'c-old', title: 'Migrazione Struts 1 → Spring MVC', updatedAt: NOW_MS - 86400000 * 2, model: 'claude-opus-4-5', cost: 2.88, context: { system: 6200, project: 22000, conversation: 41000 } },
  ],
  'bennu.assistant_load': DEMO_CONVERSATION,
  'bennu.assistant_spend': {
    meters: [
      { period: 'session', spentEur: 0.41, limitEur: null, level: 'ok' },
      { period: 'day', spentEur: 3.12, limitEur: 5, level: 'warn' },
      { period: 'month', spentEur: 21.4, limitEur: 80, level: 'ok' },
    ],
    warning: null, rate: 0.92,
  },
  'bennu.assistant_instructions': [{ name: 'CLAUDE.md', bytes: 4210, truncated: false }, { name: 'AGENTS.md', bytes: 36000, truncated: true }],
  'bennu.assistant_project_context_get': 'Legacy Struts 1 su Java 8.\nNon toccare le classi in it.banca.pos.legacy.\nI messaggi utente sono in italiano.',
  'bennu.assistant_agents': [DEMO_AGENT],
  'bennu.assistant_agent_transcript': {
    agent: DEMO_AGENT,
    message: {
      id: 'am1', role: 'assistant', at: NOW_MS - 480000, model: 'claude-haiku-4-5',
      blocks: [
        { type: 'tool', call: { id: 'at1', kind: 'search', label: 'Find callers of totale()', detail: '7 references in 4 files', ok: true } },
        { type: 'text', text: 'Ho controllato i 7 siti di chiamata: nessuno usa l\'ordine di iterazione.' },
      ],
    },
  },
  'bennu.assistant_skills_list': {
    max: 24,
    skills: [
      { id: 'u:java-review', name: 'java-review', description: 'Review Java changes for null-safety and resource leaks.', scope: 'user', source: '~/.claude/skills/java-review', project: null, path: '~/.claude/skills/java-review/SKILL.md', body: '# Java review\n\nCheck every changed method for null-safety, resource leaks and unchecked exceptions.', truncated: false, bytes: 1820, shadowed: false },
      { id: 'u:sql-style', name: 'sql-style', description: 'House style for Oracle and PostgreSQL scripts.', scope: 'user', source: '~/.claude/skills/sql-style', project: null, path: '~/.claude/skills/sql-style/SKILL.md', body: '# SQL style\n\nUppercase keywords, one column per line.', truncated: false, bytes: 940, shadowed: true },
      { id: 'p:sql-style', name: 'sql-style', description: 'POS project SQL conventions (overrides the user one).', scope: 'project', source: '.claude/skills/sql-style', project: 'pos-gateway', path: '.claude/skills/sql-style/SKILL.md', body: '# SQL style (POS)\n\nSchema prefix POS_APP.', truncated: false, bytes: 700, shadowed: false },
      { id: 'u:big-notes', name: 'release-notes', description: 'Writes release notes from the git log.', scope: 'user', source: '~/.claude/skills/release-notes', project: null, path: '~/.claude/skills/release-notes/SKILL.md', body: '# Release notes …', truncated: true, bytes: 48000, shadowed: false },
    ],
    invalid: [{ path: '~/.claude/skills/broken/SKILL.md', reason: 'missing "description" in the front matter' }],
  },
  'bennu.assistant_worktree_options': { git: { available: true, reason: '' }, copy: { available: true, reason: '', via: 'git' } },
  'bennu.assistant_worktree_info': {
    conversationId: 'c-demo', mode: 'git', realRoot: '/Users/dev/work/pos-gateway', copyRoot: '/Users/dev/.arbor/worktrees/c-demo',
    viaGit: true, head: 'a3f9c21', branch: 'imber/c-demo', createdMs: NOW_MS - 900000,
  },
  'bennu.assistant_worktree_diff': {
    canApply: true,
    clashes: [{ rel: 'src/main/resources/application.properties', reason: 'changed in the project since the copy was made' }],
    files: [
      { path: 'src/main/java/it/banca/pos/OrdineService.java', kind: 'modified', added: 3, removed: 3, hunks: DEMO_HUNKS },
      { path: 'src/test/java/it/banca/pos/OrdineServiceTest.java', kind: 'added', added: 12, removed: 0, hunks: DEMO_HUNKS },
      { path: 'src/main/resources/application.properties', kind: 'modified', added: 1, removed: 1, hunks: DEMO_HUNKS },
      { path: 'docs/legacy-notes.txt', kind: 'deleted', added: 0, removed: 28, hunks: DEMO_HUNKS },
      { path: 'src/main/webapp/img/logo.png', kind: 'added', added: 0, removed: 0, hunks: null, binary: true },
    ],
  },
  'bennu.assistant_cancel': true,
};

// ── Bennu tools: new project, templates, DTO lab, profiler ──────────────────
const GENERATORS = [
  { id: 'maven-archetype', category: 'java', label: 'Maven project', description: 'A multi-module Maven layout from an archetype.', icon: 'java', name_rule: 'artifact', requires: ['mvn', 'java'], remote: false, available: true },
  { id: 'spring-initializr', category: 'java', label: 'Spring Boot', description: 'Dependencies picked on start.spring.io.', icon: 'spring', name_rule: 'artifact', requires: ['java'], remote: true, available: true },
  { id: 'gradle-java', category: 'java', label: 'Gradle (Java)', description: 'A Gradle build with the Java plugin and JUnit 5.', icon: 'gradle', name_rule: 'artifact', requires: ['gradle'], remote: false, available: false, reason: 'gradle was not found on PATH' },
  { id: 'cargo-bin', category: 'rust', label: 'Cargo binary', description: 'A binary crate with a workspace-friendly layout.', icon: 'rust', name_rule: 'crate', requires: ['cargo'], remote: false, available: true },
  { id: 'cargo-lib', category: 'rust', label: 'Cargo library', description: 'A library crate with a prelude module.', icon: 'rust', name_rule: 'crate', requires: ['cargo'], remote: false, available: true },
  { id: 'vite-svelte', category: 'web', label: 'Vite + Svelte', description: 'Svelte 5 with TypeScript.', icon: 'svelte', name_rule: 'npm', requires: ['node', 'npm'], remote: false, available: true },
];

const NP_FIELDS = [
  { key: 'groupId', label: 'Group', kind: 'text', default: 'it.banca.pos', options: [], placeholder: 'com.example', advanced: false },
  { key: 'java', label: 'Java version', kind: 'select', default: '17', options: [{ value: '8', label: 'Java 8' }, { value: '11', label: 'Java 11' }, { value: '17', label: 'Java 17 (LTS)' }, { value: '21', label: 'Java 21 (LTS)' }], advanced: false },
  { key: 'packaging', label: 'Packaging', kind: 'select', default: 'jar', options: [{ value: 'jar', label: 'jar' }, { value: 'war', label: 'war' }], advanced: false },
  { key: 'junit', label: 'JUnit 5', kind: 'toggle', default: true, options: [], hint: 'Adds the Jupiter engine and a sample test.', advanced: false },
  { key: 'modules', label: 'Modules', kind: 'multi', default: ['core', 'web'], options: [{ value: 'core', label: 'core' }, { value: 'web', label: 'web' }, { value: 'batch', label: 'batch' }], advanced: true },
];

const tpl = (name: string, extension: string, origin: 'builtin' | 'global', extra: Record<string, unknown> = {}) => ({
  name, extension, origin, starter: false, description: null, path: origin === 'global' ? `/Users/dev/.config/arbor/templates/${name}.${extension}.j2` : null,
  requires: [], abbrev: null, applies: [], unmet: null, ...extra,
});
const KIND_LISTS = [
  { kind: 'new-file', title: 'New file', description: 'What a new file starts with.', extension: 'java', directory: '~/.config/arbor/templates/new-file', java_only: false, picks_language: true, project: null,
    templates: [tpl('Java class', 'java', 'builtin', { starter: true, description: 'A public class named after the file.' }), tpl('Java interface', 'java', 'builtin'), tpl('Spring service', 'java', 'global', { description: 'A @Service with constructor injection.' }), tpl('Plain text', 'txt', 'builtin')] },
  { kind: 'class', title: 'Class members', description: 'Members written into an existing class.', extension: 'java', directory: '~/.config/arbor/templates/class', java_only: true, picks_language: false, project: null,
    templates: [tpl('Getters and setters', 'java', 'builtin', { starter: true }), tpl('equals() and hashCode()', 'java', 'builtin'), tpl('Builder', 'java', 'global', { description: 'A fluent builder, house style.' }), tpl('Lombok data class', 'java', 'builtin', { unmet: 'Lombok is not on the project classpath' })] },
  { kind: 'config-class', title: 'Configuration class', description: '@ConfigurationProperties from application.yml.', extension: 'java', directory: '~/.config/arbor/templates/config-class', java_only: false, picks_language: false, project: null,
    templates: [tpl('Spring properties class', 'java', 'builtin', { starter: true }), tpl('Spring properties record', 'java', 'builtin')] },
  { kind: 'validation-tests', title: 'Validation tests', description: 'JUnit tests for a DTO\'s constraints.', extension: 'java', directory: '~/.config/arbor/templates/validation-tests', java_only: true, picks_language: false, project: null,
    templates: [tpl('JUnit 5 + Hibernate Validator', 'java', 'builtin', { starter: true }), tpl('JUnit 4 + AssertJ', 'java', 'global')] },
];

const RENDERED_CLASS = `package it.banca.pos.config;

import java.time.Duration;
import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "pos.gateway")
public class GatewayProperties {

    private String host = "0.0.0.0";
    private int port = 8443;
    private Duration timeout = Duration.ofSeconds(30);
    private final Tls tls = new Tls();

    public String getHost() { return host; }
    public void setHost(String host) { this.host = host; }
    public int getPort() { return port; }
    public void setPort(int port) { this.port = port; }
    public Duration getTimeout() { return timeout; }
    public void setTimeout(Duration timeout) { this.timeout = timeout; }
    public Tls getTls() { return tls; }

    public static class Tls {
        private boolean enabled = true;
        private String keystore;
        public boolean isEnabled() { return enabled; }
        public void setEnabled(boolean enabled) { this.enabled = enabled; }
        public String getKeystore() { return keystore; }
        public void setKeystore(String keystore) { this.keystore = keystore; }
    }
}
`;

const DTO_JAVA = `package it.banca.pos.dto;

import jakarta.validation.constraints.*;

public class PagamentoDto {
    @NotBlank
    @Size(max = 34)
    private String iban;

    @NotNull
    @DecimalMin("0.01")
    private java.math.BigDecimal importo;

    @Email
    private String emailRicevuta;

    @Pattern(regexp = "[A-Z]{3}")
    private String divisa;
}
`;

const DTO_TEST = `package it.banca.pos.dto;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import jakarta.validation.Validation;
import jakarta.validation.Validator;
import org.junit.jupiter.api.Test;

class PagamentoDtoValidationTest {

    private final Validator validator = Validation.buildDefaultValidatorFactory().getValidator();

    @Test
    void iban_blank_is_rejected() {
        PagamentoDto dto = valid();
        dto.setIban(" ");
        assertEquals(1, validator.validateProperty(dto, "iban").size());
    }

    @Test
    void importo_below_minimum_is_rejected() {
        PagamentoDto dto = valid();
        dto.setImporto(new java.math.BigDecimal("0.00"));
        assertTrue(validator.validateProperty(dto, "importo").size() > 0);
    }

    @Test
    void divisa_lowercase_is_rejected() {
        PagamentoDto dto = valid();
        dto.setDivisa("eur");
        assertEquals(1, validator.validateProperty(dto, "divisa").size());
    }

    private PagamentoDto valid() {
        PagamentoDto dto = new PagamentoDto();
        dto.setIban("IT60X0542811101000000123456");
        dto.setImporto(new java.math.BigDecimal("125.50"));
        dto.setDivisa("EUR");
        return dto;
    }
}
`;

const dtoField = (name: string, type_name: string, constraints: [string, Record<string, string>, string | null][]) => ({
  name, json_name: name, type_name, ignored: false, setter: `set${name[0].toUpperCase()}${name.slice(1)}`, getter: `get${name[0].toUpperCase()}${name.slice(1)}`,
  constraints: constraints.map(([n, attributes, message]) => ({ name: n, fqn: `jakarta.validation.constraints.${n}`, attributes, message })),
});

const flame = (name: string, value: number, self: number, cls: string, children: unknown[] = []) => ({
  name, value, self, kind: 'method', class: cls, children,
});
const FLAME = {
  name: 'all', value: 1000, self: 0, kind: 'root',
  children: [{
    name: 'http-nio-8443-exec-3', value: 720, self: 0, kind: 'thread',
    children: [flame('OrdineController.crea', 720, 4, 'it.banca.pos.web.OrdineController', [
      flame('OrdineService.totale', 410, 30, 'it.banca.pos.service.OrdineService', [
        flame('BigDecimal.add', 210, 210, 'java.math.BigDecimal'),
        flame('RigaOrdineDao.findByOrdine', 170, 20, 'it.banca.pos.dao.RigaOrdineDao', [flame('Connection.prepareStatement', 150, 150, 'oracle.jdbc.driver.PhysicalConnection')]),
      ]),
      flame('FatturaMapper.toDto', 190, 60, 'it.banca.pos.mapper.FatturaMapper', [flame('ObjectMapper.writeValueAsString', 130, 130, 'com.fasterxml.jackson.databind.ObjectMapper')]),
      flame('AuditLogger.log', 116, 116, 'it.banca.pos.audit.AuditLogger'),
    ])],
  }, {
    name: 'scheduling-1', value: 280, self: 0, kind: 'thread',
    children: [flame('RiepilogoJob.run', 280, 40, 'it.banca.pos.batch.RiepilogoJob', [flame('String.format', 240, 240, 'java.lang.String')])],
  }],
};

const meth = (cls: string, method: string, self_w: number, total_w: number, file: string | null = null, line: number | null = null) => ({
  class: cls, method, arity: 1, kind: 'method', file, decl_line: line, self_w, total_w, hot_lines: line ? [[line + 2, self_w]] : [],
});

const PROFILE_META = {
  id: 'rec-1', lang: 'java', label: 'OrdineController load test', created_ms: NOW_MS - 3600000, duration_ms: 45200, samples: 4520,
  ms_per_unit: 10, top_method: 'BigDecimal.add',
};

const profilerFixtures: Record<string, FixtureHandler> = {
  'bennu.bennu_np_catalog': {
    categories: [{ id: 'java', label: 'Java' }, { id: 'rust', label: 'Rust' }, { id: 'web', label: 'Web' }],
    generators: GENERATORS,
    tools: [
      { name: 'java', label: 'JDK', found: true, path: '/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home/bin/java' },
      { name: 'mvn', label: 'Maven', found: true, path: '/opt/homebrew/bin/mvn' },
      { name: 'gradle', label: 'Gradle', found: false, path: null },
      { name: 'cargo', label: 'Cargo', found: true, path: '/Users/dev/.cargo/bin/cargo' },
      { name: 'node', label: 'Node.js', found: true, path: '/opt/homebrew/bin/node' },
    ],
    home: '/Users/dev',
  },
  'bennu.get_bennu_new_project': { last_location: '/Users/dev/work', last_generator: 'maven-archetype', git_init: true, package_manager: 'npm', last_group: 'it.banca.pos' },
  'bennu.bennu_np_fields': { fields: NP_FIELDS, stale: false },
  'bennu.bennu_np_preview': (p) => {
    const r = (p.args ?? p) as { name?: string; location?: string };
    const name = r.name || 'pos-gateway';
    const loc = r.location || '/Users/dev/work';
    return {
      problems: [], ok: true, target: { path: `${loc}/${name}`, state: 'free', message: null },
      files: ['pom.xml', 'core/pom.xml', 'core/src/main/java/it/banca/pos/App.java', 'web/pom.xml', 'web/src/main/webapp/WEB-INF/web.xml', '.gitignore'],
      summary: ['Maven multi-module', 'Java 17', 'JUnit 5', 'git init'], command: `mvn archetype:generate -DgroupId=it.banca.pos -DartifactId=${name}`,
    };
  },
  'bennu.bennu_list_templates': (p) => {
    const kind = (p.args as { kind?: string | null })?.kind;
    return kind ? KIND_LISTS.filter((k) => k.kind === kind) : KIND_LISTS;
  },
  'bennu.bennu_render_template': { template: 'Spring properties class', text: RENDERED_CLASS, output: 'file', file: 'src/main/java/it/banca/pos/config/GatewayProperties.java', exists: false, insertion: null, insertion_stops: [], import_edits: [], notes: ['Durations are bound as java.time.Duration.'], source_lines: null },
  'bennu.bennu_template_schema': { context: { class: { name: 'string', fields: 'list' } }, filters: [{ name: 'camel', doc: 'userName → userName' }, { name: 'pascal', doc: 'user_name → UserName' }] },
  'bennu.bennu_template_text': '{% for f in class.fields %}\n    private {{ f.type }} {{ f.name }};\n{% endfor %}\n',
  'bennu.bennu_config_class_origin': { key: 'host', prefixes: ['pos', 'pos.gateway'], prefix: 'pos.gateway', selected: null, class_name: 'GatewayProperties', directory: '/Users/dev/work/pos-gateway/src/main/java/it/banca/pos/config', package: 'it.banca.pos.config', profiles: ['application-prod.yml', 'application-test.yml'] },
  'bennu.bennu_config_class_keys': [
    { key: 'host', name: 'host', depth: 0, group: false, map: false, sample: '0.0.0.0' },
    { key: 'port', name: 'port', depth: 0, group: false, map: false, sample: '8443' },
    { key: 'timeout', name: 'timeout', depth: 0, group: false, map: false, sample: '30s' },
    { key: 'tls', name: 'tls', depth: 0, group: true, map: false, sample: '' },
    { key: 'tls.enabled', name: 'enabled', depth: 1, group: false, map: false, sample: 'true' },
    { key: 'tls.keystore', name: 'keystore', depth: 1, group: false, map: false, sample: 'certs/gateway.p12' },
    { key: 'routes', name: 'routes', depth: 0, group: false, map: false, sample: '[3]' },
  ],
  'bennu.bennu_dtolab_class': {
    class: {
      name: 'PagamentoDto', package: 'it.banca.pos.dto', fqn: 'it.banca.pos.dto.PagamentoDto', binary: 'it.banca.pos.dto.PagamentoDto', record: false,
      fields: [
        dtoField('iban', 'String', [['NotBlank', {}, null], ['Size', { max: '34' }, null]]),
        dtoField('importo', 'BigDecimal', [['NotNull', {}, null], ['DecimalMin', { value: '0.01' }, null]]),
        dtoField('emailRicevuta', 'String', [['Email', {}, null]]),
        dtoField('divisa', 'String', [['Pattern', { regexp: '[A-Z]{3}' }, 'must be a 3-letter code']]),
      ],
    },
    skeleton: { iban: 'IT60X0542811101000000123456', importo: 125.5, emailRicevuta: 'cliente@example.com', divisa: 'EUR' },
    validation: 'jakarta', test_file: 'src/test/java/it/banca/pos/dto/PagamentoDtoValidationTest.java', template: 'JUnit 5 + Hibernate Validator',
  },
  'bennu.bennu_dtolab_generate': {
    file: '/Users/dev/work/pos-gateway/src/test/java/it/banca/pos/dto/PagamentoDtoValidationTest.java', exists: false, text: DTO_TEST, inserted: DTO_TEST, base: null, offset: null,
    template: 'JUnit 5 + Hibernate Validator', cases: 3, verified: true, warnings: ['Field "emailRicevuta": no invalid sample for @Email in the value rules, a default was used.'],
    classes: [{ name: 'PagamentoDtoValidationTest', path: 'PagamentoDtoValidationTest', depth: 0 }], import_edits: [],
  },
  'bennu.bennu_dtolab_value_rules': {
    rules: [{ name: 'IBAN', fields: ['iban', '*_iban'], constraints: ['Size', 'NotBlank'], value: 'IT60X0542811101000000123456', java: null, invalid: '', invalid_java: null }],
    disabled: [],
    builtins: [
      { name: 'E-mail', fields: ['email*', '*mail'], constraints: ['Email'], value: 'mario.rossi@example.com', java: null, invalid: 'not-an-email', invalid_java: null },
      { name: 'Currency code', fields: ['divisa', 'currency'], constraints: ['Pattern'], value: 'EUR', java: null, invalid: 'eur', invalid_java: null },
    ],
    path: '/Users/dev/.config/arbor/bennu-value-rules.toml',
  },
  'bennu.bennu_dtolab_default_json': { ok: true, json: '{\n  "iban": "IT60X0542811101000000123456"\n}' },
  'bennu.bennu_profile_detect': {
    java: { available: true, how_to_parse: { kind: 'jfr_cli', jfr: '/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home/bin/jfr' }, reason: 'JDK 17 can record with Flight Recorder', remedy: null, jdk: 'Temurin 17.0.9' },
    jdk_home: '/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home', can_attach: true, node: '/opt/homebrew/bin/node', samply: null,
  },
  'bennu.bennu_profile_list_jvms': [{ pid: 48211, name: 'pos-gateway.jar' }, { pid: 51034, name: 'org.apache.catalina.startup.Bootstrap' }, { pid: 9921, name: 'gradle-daemon' }],
  'bennu.bennu_profile_list': [
    PROFILE_META,
    { id: 'rec-2', lang: 'java', label: 'After stream refactor', created_ms: NOW_MS - 1800000, duration_ms: 44800, samples: 4480, ms_per_unit: 10, top_method: 'AuditLogger.log' },
    { id: 'rec-3', lang: 'rust', label: 'pos-engine bench', created_ms: NOW_MS - 86400000, duration_ms: 12000, samples: 1200, ms_per_unit: 10, top_method: 'serde_json::from_slice' },
  ],
  'bennu.bennu_profile_get': {
    meta: PROFILE_META,
    methods: [
      meth('java.math.BigDecimal', 'add', 210, 210), meth('java.lang.String', 'format', 240, 240),
      meth('it.banca.pos.audit.AuditLogger', 'log', 116, 116, 'src/main/java/it/banca/pos/audit/AuditLogger.java', 28),
      meth('oracle.jdbc.driver.PhysicalConnection', 'prepareStatement', 150, 150),
      meth('com.fasterxml.jackson.databind.ObjectMapper', 'writeValueAsString', 130, 130),
      meth('it.banca.pos.mapper.FatturaMapper', 'toDto', 60, 190, 'src/main/java/it/banca/pos/mapper/FatturaMapper.java', 41),
      meth('it.banca.pos.service.OrdineService', 'totale', 30, 410, 'src/main/java/it/banca/pos/service/OrdineService.java', 41),
      meth('it.banca.pos.batch.RiepilogoJob', 'run', 40, 280, 'src/main/java/it/banca/pos/batch/RiepilogoJob.java', 22),
    ],
    alloc_sites: [{ name: 'OrdineService.totale', count: 48210, bytes: 9_420_000 }, { name: 'FatturaMapper.toDto', count: 12040, bytes: 5_110_000 }],
    alloc_types: [{ name: 'java.math.BigDecimal', count: 60100, bytes: 11_200_000 }, { name: 'java.lang.String', count: 33000, bytes: 3_900_000 }],
    locks: [{ monitor: 'it.banca.pos.audit.AuditLogger', site: 'AuditLogger.log', count: 812, total_ms: 3410, max_ms: 140 }],
    gc: [{ collector: 'G1 Young Generation', count: 38, total_pause_ms: 612, max_pause_ms: 41, top_cause: 'G1 Evacuation Pause' }],
    threads: [{ name: 'http-nio-8443-exec-3', weight: 720 }, { name: 'scheduling-1', weight: 280 }],
    waits: [{ kind: 'socket', target: 'db-coll.banca.local:1521', site: 'PhysicalConnection.prepareStatement', count: 920, total_ms: 6210, max_ms: 380 }],
    has_alloc_stacks: true, has_waits: true, notes: [],
  },
  'bennu.bennu_profile_flame': FLAME,
  'bennu.bennu_profile_alloc_by_code': { sites: [] },
  'bennu.bennu_profile_prefs_get': {
    filter: { project_only: false, include: ['it.banca.pos'], exclude: ['java.', 'jdk.'] },
    thresholds: {
      cpu_self_pct: { yellow: 5, orange: 15, red: 30 }, gc_pause_pct: { yellow: 2, orange: 5, red: 10 },
      alloc_share_pct: { yellow: 10, orange: 25, red: 50 }, lock_total_ms: { yellow: 100, orange: 500, red: 2000 },
    },
  },
  'bennu.bennu_samply_status': { supported: true, user_path: '', resolved: null, problem: null, managed_version: null, installing: false },
  'bennu.bennu_samply_latest': {
    version: 'v0.13.1', asset_name: 'samply-aarch64-apple-darwin.tar.xz', size: 5_812_340, url: 'https://github.com/mstange/samply/releases/download/samply-v0.13.1/samply-aarch64-apple-darwin.tar.xz',
    checksum_source: 'api', sha256: '9f2b5c1e7a04d3c8b6e1f0a2d4c7e8b9a1c3d5e7f9a0b2c4d6e8f0a1b3c5d7e9', installed_version: null, update_available: false, blocker: null,
  },
  'bennu.get_bennu_profiler_tools': { samply_path: '' },
};

// ── Sitta: a small virtual filesystem for the explorer / file picker ────────
const H = '/Users/dev';
const DAY = 86_400_000;
const fe = (dir: string, name: string, isDir: boolean, size: number | null, ageDays: number) => ({
  name, path: `${dir}/${name}`, is_dir: isDir, size: isDir ? null : size,
  modified: NOW_MS - ageDays * DAY, created: NOW_MS - (ageDays + 30) * DAY,
});
const VFS: Record<string, ReturnType<typeof fe>[]> = {
  [H]: [
    fe(H, 'Desktop', true, null, 3), fe(H, 'Documents', true, null, 9), fe(H, 'Downloads', true, null, 0.2),
    fe(H, 'work', true, null, 1), fe(H, 'Pictures', true, null, 40), fe(H, 'notes.md', false, 3120, 6),
  ],
  [`${H}/work`]: [
    fe(`${H}/work`, 'pos-gateway', true, null, 0.5), fe(`${H}/work`, 'pos-db', true, null, 2), fe(`${H}/work`, 'game', true, null, 14),
    fe(`${H}/work`, 'pos-engine', true, null, 21), fe(`${H}/work`, 'release-2.7.0.zip', false, 48_211_340, 4), fe(`${H}/work`, 'TODO.txt', false, 880, 8),
  ],
  [`${H}/work/pos-db`]: [
    fe(`${H}/work/pos-db`, 'ORACLE', true, null, 2), fe(`${H}/work/pos-db`, 'POS', true, null, 2), fe(`${H}/work/pos-db`, 'Migrazione', true, null, 120),
    fe(`${H}/work/pos-db`, '001_create_tables.sql', false, 48_211, 12), fe(`${H}/work/pos-db`, '2.6.0_to_2.7.0.sql', false, 15_390, 2),
    fe(`${H}/work/pos-db`, '2.5.0_to_2.6.0.sql', false, 8_120, 30), fe(`${H}/work/pos-db`, 'README.md', false, 1_940, 60), fe(`${H}/work/pos-db`, 'schema.png', false, 214_880, 90),
  ],
  [`${H}/work/pos-gateway`]: [
    fe(`${H}/work/pos-gateway`, 'src', true, null, 0.5), fe(`${H}/work/pos-gateway`, 'target', true, null, 0.5), fe(`${H}/work/pos-gateway`, 'pom.xml', false, 6_412, 5),
    fe(`${H}/work/pos-gateway`, 'README.md', false, 2_210, 40), fe(`${H}/work/pos-gateway`, 'application.yml', false, 640, 7), fe(`${H}/work/pos-gateway`, '.gitignore', false, 120, 200),
  ],
};
const fsFixtures: Record<string, FixtureHandler> = {
  'platform.fs_read_dir': (p) => VFS[String(p.path).replace(/\/$/, '')] ?? VFS[`${H}/work/pos-db`].map((e) => ({ ...e, path: `${String(p.path)}/${e.name}` })),
  'platform.list_fs_roots': [
    { name: 'Home', path: H, kind: 'home' }, { name: 'Desktop', path: `${H}/Desktop`, kind: 'desktop' },
    { name: 'Documents', path: `${H}/Documents`, kind: 'documents' }, { name: 'Downloads', path: `${H}/Downloads`, kind: 'downloads' },
    { name: 'Macintosh HD', path: '/', kind: 'drive' },
  ],
  'platform.list_wsl_distros': [],
  'platform.fs_overview_stats': {
    drives: [{ name: 'Macintosh HD', path: '/', total: 994_662_584_320, free: 412_880_101_376 }],
    total_capacity: 994_662_584_320, total_free: 412_880_101_376,
  },
  'platform.fs_expand_path': (p) => String(p.path),
  'platform.fs_paths_size': { bytes: 7_240_011, files: 12, dirs: 2 },
  'sitta.fs_git_status': (p) => {
    const dir = String(p.dir);
    const inRepo = dir.startsWith(`${H}/work/pos-gateway`);
    return {
      in_repo: inRepo, repo_root: inRepo ? `${H}/work/pos-gateway` : null, branch: inRepo ? 'feature/stream-totale' : null, detached: false, ahead: 2, behind: 0,
      badges: inRepo ? { [`${H}/work/pos-gateway/pom.xml`]: 'modified', [`${H}/work/pos-gateway/application.yml`]: 'untracked' } : {},
      repos: dir === `${H}/work` ? { [`${H}/work/pos-gateway`]: { branch: 'feature/stream-totale', dirty: true } } : {},
    };
  },
  'explorer_clip_get': null,
  'get_ide_config': { ides: [], default_ide: null },
  'corvus.get_ide_config': { ides: [], default_ide: null },
  'corvus.start_ide_detection': null,
  fs_watch_stop: null,
  fs_watch_file_stop: null,
};

// ── Tyto: screen recorder library ───────────────────────────────────────────
/** A flat "screenshot" the poster/preview can show — a data URI, so it needs no file on disk. */
const poster = (hue: number, label: string) =>
  'data:image/svg+xml,' + encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="200"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">` +
    `<stop offset="0" stop-color="hsl(${hue},45%,28%)"/><stop offset="1" stop-color="hsl(${(hue + 40) % 360},50%,14%)"/></linearGradient></defs>` +
    `<rect width="320" height="200" fill="url(#g)"/><rect x="24" y="24" width="272" height="22" rx="4" fill="rgba(255,255,255,.12)"/>` +
    `<rect x="24" y="60" width="120" height="116" rx="4" fill="rgba(255,255,255,.08)"/><rect x="156" y="60" width="140" height="52" rx="4" fill="rgba(255,255,255,.1)"/>` +
    `<text x="160" y="150" fill="rgba(255,255,255,.7)" font-family="sans-serif" font-size="14" text-anchor="middle">${label}</text></svg>`,
  );
const TYTO_DIR = `${H}/Movies/Arbor`;
const cap = (id: string, name: string, kind: string, target: string, dur: number | null, size: number, ageMin: number, hue: number) => ({
  id, name, kind, target, duration_ms: dur, size_bytes: size, created_at: NOW_MS - ageMin * 60000,
  path: `${TYTO_DIR}/${name}.${kind === 'screenshot' ? 'png' : kind === 'video' ? 'mp4' : ''}`.replace(/\.$/, ''), poster: poster(hue, name),
});

const tytoFixtures: Record<string, FixtureHandler> = {
  'tyto.get_tyto_config': {
    default_mode: 'record', default_target: 'monitor', record_output: 'video',
    capture: { fps: 30, system_audio: true, mic_id: '', countdown_secs: 3 },
    encoding: { quality: 'high', bitrate_kbps: 8000 },
    output: { dir: '', filename_template: 'Arbor {date} at {time}', screenshot_format: 'png', copy_screenshot_to_clipboard: true },
    frames: { format: 'png', sample_fps: 4, max_width: 1280 },
  },
  'tyto.output_dir': TYTO_DIR,
  'tyto.list_capture_sources': {
    monitors: [
      { id: 'mon-1', name: 'Built-in Retina Display', x: 0, y: 0, width: 3024, height: 1964, scale: 2, primary: true },
      { id: 'mon-2', name: 'Dell U2723QE', x: 3024, y: 0, width: 3840, height: 2160, scale: 1, primary: false },
    ],
    windows: [
      { id: 'win-1', title: 'Arbor — pos-gateway', app: 'Arbor' }, { id: 'win-2', title: 'Pull request #482 · banca/pos-gateway', app: 'Safari' },
      { id: 'win-3', title: 'Terminal — mvn verify', app: 'Terminal' }, { id: 'win-4', title: 'Slack — #pos-release', app: 'Slack' },
    ],
    unavailable: null,
  },
  'tyto.list_audio_inputs': [
    { id: 'mic-1', name: 'MacBook Pro Microphone', is_default: true }, { id: 'mic-2', name: 'Shure MV7', is_default: false },
  ],
  'tyto.list_captures': [
    cap('cap-1', 'Arbor 2026-11-01 at 13.12.04', 'video', 'Built-in Retina Display', 84_000, 61_240_000, 12, 210),
    cap('cap-2', 'Checkout bug repro', 'video', 'Safari — Pull request #482', 23_500, 18_900_000, 95, 12),
    cap('cap-3', 'Rename dialog', 'screenshot', 'Region 1200×800', null, 412_880, 190, 150),
    cap('cap-4', 'Sprite idle walk', 'frames', 'Region 256×256', 3_000, 2_140_000, 400, 280),
    cap('cap-5', 'Terminal build output', 'screenshot', 'Terminal', null, 188_020, 1500, 40),
  ],
  'tyto.session_state': { session_id: null, recording: false, paused: false, elapsed_ms: 0, output: 'video' },
  'tyto.screen_recording_status': { granted: true, hint: '' },
  'tyto.read_frame_sequence': {
    dir: `${TYTO_DIR}/Sprite idle walk`, width: 256, height: 256, sample_fps: 4, duration_ms: 3000, target: 'Region 256×256', size_bytes: 2_140_000,
    frames: [poster(200, 'frame 1'), poster(220, 'frame 2'), poster(240, 'frame 3'), poster(260, 'frame 4')], times: [0, 250, 500, 750],
  },
};

export const picusFixtures: Record<string, FixtureHandler> = {
  ...tytoFixtures,
  ...fsFixtures,
  ...studioFixtures,
  ...assistantFixtures,
  ...profilerFixtures,
  // The Java file the DTO lab and the template modals read.
  'bennu.bennu_read_file': (p) => {
    const file = String((p.args as { file?: string })?.file ?? '');
    if (file.endsWith('PagamentoDto.java')) return { text: DTO_JAVA, encoding: 'UTF-8' };
    if (file.endsWith('application.yml')) {
      return { text: 'pos:\n  gateway:\n    host: 0.0.0.0\n    port: 8443\n    timeout: 30s\n    tls:\n      enabled: true\n      keystore: certs/gateway.p12\n    routes:\n      - authorize\n      - settle\n      - refund\n', encoding: 'UTF-8' };
    }
    return { text: '', encoding: 'UTF-8' };
  },
  'picus.picus_providers': PROVIDERS,
  'picus.picus_list_auth_methods': [
    {
      id: 'kerberos', label: 'Kerberos (SSO)', help: 'Uses the ticket of the signed-in Windows user.',
      engines: ['oracle', 'postgres'], requiresTls: false, fields: [field('principal', 'Principal', 'text', { placeholder: 'user@BANCA.LOCAL' })],
    },
    {
      id: 'iam', label: 'Cloud IAM token', help: 'Short-lived token from the cloud CLI.',
      engines: ['postgres'], requiresTls: true, fields: [field('region', 'Region', 'text', { placeholder: 'eu-south-1' })],
    },
  ],
  'picus.picus_list_connections': CONNECTIONS,
  'picus.picus_read_lob': {
    bytes: 218304, base64: SAMPLE_PDF_B64, truncated: false,
  },
  'picus.picus_open_scripts': {
    project: { name: 'pos-db', root: '/Users/dev/work/pos-db', tree: PROJECT_TREE },
    notes: [], isNew: false, problems: [],
    aliases: [{ name: 'POS', engine: 'postgres', role: null, scope: 'folders' }],
  },
  'picus.picus_refresh_scripts': {
    project: { name: 'pos-db', root: '/Users/dev/work/pos-db', tree: PROJECT_TREE },
    notes: [], isNew: false, problems: [], aliases: [],
  },
  'picus.picus_analyze_scripts': { inventory: [], findings: [], skipped: [], rejectedSuppressions: [], orphans: [] },
  'picus.picus_project_settings': { versionTable: { table: '', versionColumn: '', filter: '' }, products: [] },
  fs_watch_start: null,
  take_explorer_reveal: null,
  ensure_drag_overlay: null,
  'picus.picus_destination_sets': [],
  'picus.picus_folders_named': ['POS', 'ORACLE/POS', 'Utility/POS'],
  'picus.picus_files_named': ['POS/Init/001_schema.sql', 'ORACLE/Update/POS_2.5.0.sql'],
};
