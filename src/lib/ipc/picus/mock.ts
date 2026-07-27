/**
 * Picus fixtures — TEMPORARY stand-in for `picus-be`.
 *
 * Picus's backend doesn't exist yet: this module serves the shapes the real RPC
 * surface will serve (schema, connections, script tree, inventory, findings) so
 * the whole UI can be built and reviewed first. Same staging as Tyto's mocked
 * control panel before `tyto-be` landed.
 *
 * DELETE THIS FILE when `picus-be` serves the corresponding handlers — every
 * store reads it through a single `mock*` accessor precisely so the swap is one
 * import per store, not a rewrite.
 */

import type {
  Connection,
  Branch,
  CellValue,
  Finding,
  InventoryObject,
  Project,
  SequenceInfo,
  TableInfo,
  Target,
  TriggerInfo,
} from '$lib/types/picus';

// ── Connections ──────────────────────────────────────────────────────────────

export const MOCK_CONNECTIONS: Connection[] = [
  {
    id: 'dev',
    name: 'ORCL-DEV',
    alias: 'development',
    dialect: 'oracle',
    schema: 'APPPROD',
    host: 'ora19-dev:1521/DEVPDB',
    state: 'connected',
    dbVersion: '4.12',
    colorIdx: 1,
    readOnly: false,
  },
  {
    id: 'staging',
    name: 'ORCL-STAGING',
    alias: 'staging',
    dialect: 'oracle',
    schema: 'APPPROD',
    host: 'ora19-stg:1521/STGPDB',
    state: 'connected',
    dbVersion: '4.11',
    colorIdx: 6,
    readOnly: false,
  },
  {
    id: 'pg',
    name: 'PG-DEV',
    alias: 'pg development',
    dialect: 'postgres',
    schema: 'public',
    host: 'pg-dev:5432/appprod',
    state: 'connected',
    dbVersion: '4.12',
    colorIdx: 0,
    readOnly: false,
  },
  {
    id: 'prod',
    name: 'ORCL-PROD',
    alias: 'production',
    dialect: 'oracle',
    schema: 'APPPROD',
    host: 'ora19-prod:1521/PRDPDB',
    state: 'read-only',
    dbVersion: '4.10',
    colorIdx: 4,
    readOnly: true,
  },
];

// ── Schema ───────────────────────────────────────────────────────────────────

export const MOCK_TABLES: TableInfo[] = [
  {
    name: 'PARAMETRI',
    kind: 'table',
    estimatedRows: 4,
    columns: [
      { name: 'COD_PARAMETRO', type: 'VARCHAR2(30)', primaryKey: true, notNull: true },
      { name: 'VALORE', type: 'VARCHAR2(200)' },
      { name: 'DESCRIZIONE', type: 'VARCHAR2(400)' },
      { name: 'DATA_MOD', type: 'DATE', defaultValue: 'SYSDATE' },
    ],
    primaryKeyName: 'PK_PARAMETRI',
    foreignKeys: [],
    indexes: [
      { name: 'PK_PARAMETRI', columns: ['COD_PARAMETRO'], unique: true, kind: 'BTREE', primaryKey: true },
    ],
  },
  {
    name: 'CLIENTI',
    kind: 'table',
    estimatedRows: 128,
    columns: [
      { name: 'ID_CLIENTE', type: 'NUMBER', primaryKey: true, notNull: true },
      { name: 'RAG_SOCIALE', type: 'VARCHAR2(200)', notNull: true },
      { name: 'PIVA', type: 'VARCHAR2(16)' },
      { name: 'COD_LISTINO', type: 'VARCHAR2(20)' },
      { name: 'ATTIVO', type: 'CHAR(1)', defaultValue: "'S'" },
      { name: 'DATA_INS', type: 'DATE', defaultValue: 'SYSDATE' },
    ],
    primaryKeyName: 'PK_CLIENTI',
    foreignKeys: [
      {
        name: 'FK_CLIENTI_LISTINO',
        columns: ['COD_LISTINO'],
        referencedTable: 'LISTINI',
        referencedColumns: ['COD_LISTINO'],
        onDelete: 'NO ACTION',
      },
    ],
    indexes: [
      { name: 'PK_CLIENTI', columns: ['ID_CLIENTE'], unique: true, kind: 'BTREE', primaryKey: true },
      { name: 'UQ_CLIENTI_PIVA', columns: ['PIVA'], unique: true, kind: 'BTREE' },
      { name: 'IX_CLIENTI_LISTINO', columns: ['COD_LISTINO'], unique: false, kind: 'BTREE' },
      { name: 'IX_CLIENTI_RAGSOC_UP', columns: ['UPPER(RAG_SOCIALE)'], unique: false, kind: 'FUNCTION-BASED' },
    ],
  },
  {
    name: 'LISTINI',
    kind: 'table',
    estimatedRows: 2,
    columns: [
      { name: 'COD_LISTINO', type: 'VARCHAR2(20)', primaryKey: true, notNull: true },
      { name: 'DESCRIZIONE', type: 'VARCHAR2(200)' },
      { name: 'SCONTO_MAX', type: 'NUMBER(5,2)' },
      { name: 'VALIDO_DA', type: 'DATE' },
    ],
    primaryKeyName: 'PK_LISTINI',
    foreignKeys: [],
    indexes: [
      { name: 'PK_LISTINI', columns: ['COD_LISTINO'], unique: true, kind: 'BTREE', primaryKey: true },
    ],
  },
  {
    name: 'ORDINI',
    kind: 'table',
    estimatedRows: 4210,
    columns: [
      { name: 'ID_ORDINE', type: 'NUMBER', primaryKey: true, notNull: true },
      { name: 'ID_CLIENTE', type: 'NUMBER', notNull: true },
      { name: 'TOTALE', type: 'NUMBER(12,2)' },
      { name: 'STATO', type: 'VARCHAR2(12)', defaultValue: "'APERTO'" },
      { name: 'DATA_ORD', type: 'DATE' },
    ],
    primaryKeyName: 'PK_ORDINI',
    foreignKeys: [
      {
        name: 'FK_ORDINI_CLIENTE',
        columns: ['ID_CLIENTE'],
        referencedTable: 'CLIENTI',
        referencedColumns: ['ID_CLIENTE'],
        onDelete: 'CASCADE',
      },
    ],
    indexes: [
      { name: 'PK_ORDINI', columns: ['ID_ORDINE'], unique: true, kind: 'BTREE', primaryKey: true },
      { name: 'IX_ORDINI_CLIENTE', columns: ['ID_CLIENTE'], unique: false, kind: 'BTREE' },
      { name: 'IX_ORDINI_DATA_STATO', columns: ['DATA_ORD', 'STATO'], unique: false, kind: 'BTREE' },
    ],
  },
  {
    name: 'VERSIONE_DB',
    kind: 'table',
    estimatedRows: 1,
    columns: [
      { name: 'VERSIONE', type: 'VARCHAR2(10)', primaryKey: true, notNull: true },
      { name: 'DATA_AGG', type: 'DATE' },
    ],
    primaryKeyName: 'PK_VERSIONE_DB',
    foreignKeys: [],
    indexes: [
      { name: 'PK_VERSIONE_DB', columns: ['VERSIONE'], unique: true, kind: 'BTREE', primaryKey: true },
    ],
  },
];

export const MOCK_VIEWS: TableInfo[] = [
  {
    name: 'V_CLIENTI_ATTIVI',
    kind: 'view',
    columns: [
      { name: 'ID_CLIENTE', type: 'NUMBER' },
      { name: 'RAG_SOCIALE', type: 'VARCHAR2(200)' },
      { name: 'COD_LISTINO', type: 'VARCHAR2(20)' },
      { name: 'SCONTO_MAX', type: 'NUMBER(5,2)' },
    ],
    definition:
      "SELECT c.ID_CLIENTE,\n       c.RAG_SOCIALE,\n       c.COD_LISTINO,\n       l.SCONTO_MAX\n  FROM CLIENTI c\n  JOIN LISTINI l ON l.COD_LISTINO = c.COD_LISTINO\n WHERE c.ATTIVO = 'S';",
  },
  {
    name: 'V_ORDINI_APERTI',
    kind: 'view',
    columns: [
      { name: 'ID_ORDINE', type: 'NUMBER' },
      { name: 'RAG_SOCIALE', type: 'VARCHAR2(200)' },
      { name: 'TOTALE', type: 'NUMBER(12,2)' },
      { name: 'DATA_ORD', type: 'DATE' },
    ],
    definition:
      "SELECT o.ID_ORDINE,\n       c.RAG_SOCIALE,\n       o.TOTALE,\n       o.DATA_ORD\n  FROM ORDINI o\n  JOIN CLIENTI c ON c.ID_CLIENTE = o.ID_CLIENTE\n WHERE o.STATO = 'APERTO';",
  },
];

export const MOCK_SEQUENCES: SequenceInfo[] = [
  { name: 'SEQ_CLIENTI', lastValue: 1045, incrementBy: 1, minValue: 1, cycle: false, cacheSize: 20 },
  { name: 'SEQ_ORDINI', lastValue: 90413, incrementBy: 1, minValue: 1, cycle: false, cacheSize: 50 },
  { name: 'SEQ_MOVIMENTI', lastValue: 771204, incrementBy: 1, minValue: 1, maxValue: 999999999, cycle: false, cacheSize: 100 },
];

export const MOCK_TRIGGERS: TriggerInfo[] = [
  { name: 'TRG_CLIENTI_BI', table: 'CLIENTI', timing: 'BEFORE', events: ['INSERT'], enabled: true, forEachRow: true },
  { name: 'TRG_CLIENTI_AUD', table: 'CLIENTI', timing: 'AFTER', events: ['UPDATE', 'DELETE'], enabled: true, forEachRow: true },
  { name: 'TRG_ORDINI_BI', table: 'ORDINI', timing: 'BEFORE', events: ['INSERT'], enabled: true, forEachRow: true },
  { name: 'TRG_PARAMETRI_AUD', table: 'PARAMETRI', timing: 'AFTER', events: ['INSERT', 'UPDATE'], enabled: false, forEachRow: true },
];

/**
 * A deliberately long ORDINI so paging and virtualisation are exercisable.
 * Deterministic (no randomness) so two runs show the same rows and a screenshot
 * stays comparable.
 */
function generateOrdini(count: number): CellValue[][] {
  const states = ['APERTO', 'CHIUSO', 'ANNULLATO', 'IN CORSO'];
  return Array.from({ length: count }, (_, i) => {
    const id = 90411 + i;
    const client = 1041 + (i % 5);
    const total = Number(((i * 137.45) % 18_000).toFixed(2));
    const day = (i % 28) + 1;
    const month = (i % 12) + 1;
    return [
      id,
      client,
      // Every 17th order has no total yet — a real NULL, so the grid's
      // NULL-vs-empty distinction is visible in the fixtures too.
      i % 17 === 0 ? null : total,
      states[i % states.length],
      `2026-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`,
    ];
  });
}

/** Sample contents, keyed by table. `null` is a real NULL (rendered italic). */
export const MOCK_TABLE_ROWS: Record<string, CellValue[][]> = {
  PARAMETRI: [
    ['VALUTA_BASE', 'EUR', 'Valuta di riferimento', '2026-01-12'],
    ['GIORNI_STORICO', '365', 'Giorni di storico conservati', '2026-01-12'],
    ['SCONTO_MAX', '30', 'Sconto massimo di listino', '2026-03-04'],
    ['SOGLIA_SCONTO', '15', 'Soglia sconto massimo applicabile', '2026-07-27'],
  ],
  CLIENTI: [
    [1041, 'Fornaci Adriatiche SpA', '01928374651', 'STD2026', 'S', '2026-02-03'],
    [1042, 'Vetrerie Murano Srl', '02938475612', 'PREM2026', 'S', '2026-02-11'],
    [1043, 'Cantieri Lido SNC', '03847561209', 'STD2026', 'N', '2026-03-28'],
    [1044, 'Officine Brenta Srl', null, 'STD2026', 'S', '2026-04-02'],
    [1045, 'Marmi Apuani SpA', '05948372615', 'PREM2026', 'S', '2026-05-19'],
  ],
  LISTINI: [
    ['STD2026', 'Listino standard 2026', 15, '2026-01-01'],
    ['PREM2026', 'Listino premium 2026', 25, '2026-01-01'],
  ],
  ORDINI: generateOrdini(4210),
  VERSIONE_DB: [['4.12', '2026-07-20']],
  V_CLIENTI_ATTIVI: [
    [1041, 'Fornaci Adriatiche SpA', 'STD2026', 15],
    [1042, 'Vetrerie Murano Srl', 'PREM2026', 25],
    [1044, 'Officine Brenta Srl', 'STD2026', 15],
    [1045, 'Marmi Apuani SpA', 'PREM2026', 25],
  ],
  V_ORDINI_APERTI: [
    [90412, 'Vetrerie Murano Srl', 3190.5, '2026-07-19'],
    [90413, 'Marmi Apuani SpA', 872.4, '2026-07-24'],
  ],
};

// ── Script repository ────────────────────────────────────────────────────────

export const MOCK_BRANCHES: Branch[] = [
  {
    id: 'ora',
    label: 'ORACLE',
    dialect: 'oracle',
    folders: [
      {
        id: 'ora-init',
        label: 'INIZIALIZZAZIONE',
        role: 'init',
        path: 'ORACLE/INIZIALIZZAZIONE',
        files: [
          { path: 'ORACLE/INIZIALIZZAZIONE/01_TABELLE.sql', name: '01_TABELLE.sql', size: 43_008, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'CRLF', expectedEncoding: 'windows-1252' },
          { path: 'ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql', name: '02_PARAMETRI.sql', size: 18_432, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'CRLF', expectedEncoding: 'windows-1252', status: 'modified' },
          { path: 'ORACLE/INIZIALIZZAZIONE/03_CLIENTI.sql', name: '03_CLIENTI.sql', size: 98_304, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'CRLF', expectedEncoding: 'windows-1252' },
          { path: 'ORACLE/INIZIALIZZAZIONE/04_PROCEDURE.sql', name: '04_PROCEDURE.sql', size: 72_704, encoding: 'windows-1252', encodingSource: 'inherited', eol: 'CRLF', expectedEncoding: 'windows-1252' },
        ],
      },
      {
        id: 'ora-upd',
        label: 'AGGIORNAMENTO',
        role: 'update',
        path: 'ORACLE/AGGIORNAMENTO',
        files: [
          { path: 'ORACLE/AGGIORNAMENTO/4_11__4_12.sql', name: '4_11__4_12.sql', size: 9_216, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'CRLF', expectedEncoding: 'windows-1252' },
          { path: 'ORACLE/AGGIORNAMENTO/4_12__4_13.sql', name: '4_12__4_13.sql', size: 1_024, encoding: 'windows-1252', encodingSource: 'inherited', eol: 'CRLF', expectedEncoding: 'windows-1252', status: 'new' },
        ],
      },
    ],
  },
  {
    id: 'pg',
    label: 'POSTGRES',
    dialect: 'postgres',
    folders: [
      {
        id: 'pg-init',
        label: 'INIZIALIZZAZIONE',
        role: 'init',
        path: 'POSTGRES/INIZIALIZZAZIONE',
        files: [
          { path: 'POSTGRES/INIZIALIZZAZIONE/01_tabelle.sql', name: '01_tabelle.sql', size: 38_912, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'LF', expectedEncoding: 'windows-1252' },
          { path: 'POSTGRES/INIZIALIZZAZIONE/02_parametri.sql', name: '02_parametri.sql', size: 16_384, encoding: 'UTF-8', encodingSource: 'utf8', eol: 'LF', expectedEncoding: 'windows-1252', status: 'error' },
          { path: 'POSTGRES/INIZIALIZZAZIONE/03_clienti.sql', name: '03_clienti.sql', size: 90_112, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'LF', expectedEncoding: 'windows-1252' },
        ],
      },
      {
        id: 'pg-upd',
        label: 'AGGIORNAMENTO',
        role: 'update',
        path: 'POSTGRES/AGGIORNAMENTO',
        files: [
          { path: 'POSTGRES/AGGIORNAMENTO/4_11__4_12.sql', name: '4_11__4_12.sql', size: 8_192, encoding: 'windows-1252', encodingSource: 'heuristic', eol: 'LF', expectedEncoding: 'windows-1252' },
          { path: 'POSTGRES/AGGIORNAMENTO/4_12__4_13.sql', name: '4_12__4_13.sql', size: 1_024, encoding: 'windows-1252', encodingSource: 'inherited', eol: 'LF', expectedEncoding: 'windows-1252', status: 'new' },
        ],
      },
    ],
  },
];

export const MOCK_PROJECT: Project = {
  name: 'PROD_CORE',
  root: 'C:\\progetti\\prod-core\\database',
  branches: MOCK_BRANCHES,
};

// ── Generator targets ────────────────────────────────────────────────────────

/**
 * The default target set for a two-branch project: the init script gets bare
 * statements, the update script gets a guarded procedural block. These are the
 * per-role presets of §4.4, materialised.
 */
export const MOCK_TARGETS: Target[] = [
  {
    id: 't-ora-init',
    file: 'ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql',
    dialect: 'oracle',
    role: 'init',
    branchId: 'ora',
    enabled: true,
    wrap: 'plain',
    guards: { version: null, skipIfPresent: false, requireObject: false, transactional: false },
  },
  {
    id: 't-ora-upd',
    file: 'ORACLE/AGGIORNAMENTO/4_12__4_13.sql',
    dialect: 'oracle',
    role: 'update',
    branchId: 'ora',
    enabled: true,
    wrap: 'block',
    guards: { version: { from: '4.12', to: '4.13' }, skipIfPresent: true, requireObject: false, transactional: true },
  },
  {
    id: 't-pg-init',
    file: 'POSTGRES/INIZIALIZZAZIONE/02_parametri.sql',
    dialect: 'postgres',
    role: 'init',
    branchId: 'pg',
    enabled: true,
    wrap: 'plain',
    guards: { version: null, skipIfPresent: false, requireObject: false, transactional: false },
  },
  {
    id: 't-pg-upd',
    file: 'POSTGRES/AGGIORNAMENTO/4_12__4_13.sql',
    dialect: 'postgres',
    role: 'update',
    branchId: 'pg',
    enabled: true,
    wrap: 'block',
    guards: { version: { from: '4.12', to: '4.13' }, skipIfPresent: true, requireObject: false, transactional: false },
  },
];

// ── Inventory ────────────────────────────────────────────────────────────────

export const MOCK_INVENTORY: InventoryObject[] = [
  { name: 'CLIENTI', kind: 'table', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 1, 'pg/pg-init': 1, 'pg/pg-upd': 1 } },
  { name: 'PARAMETRI', kind: 'table', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 1, 'pg/pg-init': 1, 'pg/pg-upd': 0 } },
  { name: 'LISTINI', kind: 'table', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 2, 'pg/pg-init': 1, 'pg/pg-upd': 1 } },
  { name: 'ORDINI', kind: 'table', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 1, 'pg/pg-init': 1, 'pg/pg-upd': 1 } },
  { name: 'VERSIONE_DB', kind: 'table', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 1, 'pg/pg-init': 1, 'pg/pg-upd': 1 } },
  { name: 'PKG_CLIENTI', kind: 'package', coverage: { 'ora/ora-init': 1, 'ora/ora-upd': 0, 'pg/pg-init': 0, 'pg/pg-upd': 0 } },
];

// ── Findings ─────────────────────────────────────────────────────────────────

export const MOCK_FINDINGS: Finding[] = [
  {
    id: 'f1',
    rule: 'CONS001',
    severity: 'blocking',
    title: 'SOGLIA_SCONTO is missing from the PostgreSQL branch',
    consequence:
      'The row is inserted by ORACLE/AGGIORNAMENTO but the matching PostgreSQL script never propagates it: the two branches diverge from version 4.13 onwards, and a PostgreSQL install ends up without the parameter.',
    file: 'POSTGRES/AGGIORNAMENTO/4_12__4_13.sql',
    branchId: 'pg',
    fixLabel: 'Generate for PostgreSQL too',
  },
  {
    id: 'f2',
    rule: 'VER001',
    severity: 'blocking',
    title: 'Update block has no starting-version guard',
    consequence:
      'The block writes without checking which version it is starting from: running the script again on a database already at 4.13 re-applies the UPDATE and silently overwrites newer values.',
    file: 'ORACLE/AGGIORNAMENTO/4_11__4_12.sql',
    line: 24,
    branchId: 'ora',
    fixLabel: 'Add the version guard',
  },
  {
    id: 'f3',
    rule: 'VER002',
    severity: 'blocking',
    title: 'Final version is never carried forward',
    consequence:
      'The block applies its changes but never moves VERSIONE_DB to 4.13, so the next update refuses to start and the installation stalls one version behind.',
    file: 'POSTGRES/AGGIORNAMENTO/4_12__4_13.sql',
    line: 31,
    branchId: 'pg',
    fixLabel: 'Add the closing UPDATE',
  },
  {
    id: 'f4',
    rule: 'DUP001',
    severity: 'blocking',
    title: "Duplicate INSERT on LISTINI ('STD2026')",
    consequence:
      'The same key is inserted twice in one script: the second statement fails on the primary key and aborts the rest of the run.',
    file: 'ORACLE/AGGIORNAMENTO/4_11__4_12.sql',
    line: 12,
    alsoAt: 'ORACLE/AGGIORNAMENTO/4_11__4_12.sql:45',
    branchId: 'ora',
    fixLabel: 'Remove the duplicate',
  },
  {
    id: 'f5',
    rule: 'ENC001',
    severity: 'review',
    title: 'File was rewritten as UTF-8',
    consequence:
      'The script was saved by an external editor and lost its windows-1252 encoding: three accented characters are now mojibake, and the descriptions they belong to will install wrong.',
    file: 'POSTGRES/INIZIALIZZAZIONE/02_parametri.sql',
    branchId: 'pg',
    fixLabel: 'Convert back to windows-1252',
  },
  {
    id: 'f6',
    rule: 'DML001',
    severity: 'review',
    title: 'DELETE without a WHERE clause',
    consequence:
      'The statement empties the whole table rather than a subset. Intentional resets are fine, but this one is not declared as such.',
    file: 'ORACLE/INIZIALIZZAZIONE/02_PARAMETRI.sql',
    line: 8,
    branchId: 'ora',
    suppressedBecause: 'full reload of the parameter table on install',
  },
  {
    id: 'f7',
    rule: 'DML002',
    severity: 'review',
    title: 'INSERT without an explicit column list',
    consequence:
      'The statement relies on the physical column order: adding a column to the table silently shifts every value one position to the right.',
    file: 'POSTGRES/INIZIALIZZAZIONE/03_clienti.sql',
    line: 117,
    branchId: 'pg',
    fixLabel: 'Spell out the columns',
  },
  {
    id: 'f8',
    rule: 'DUP002',
    severity: 'review',
    title: 'PKG_CLIENTI is defined in two files',
    consequence:
      'Two definitions of the same package exist; whichever runs last wins, so the installed body depends on file ordering rather than on intent.',
    file: 'ORACLE/INIZIALIZZAZIONE/04_PROCEDURE.sql',
    line: 402,
    alsoAt: 'ORACLE/AGGIORNAMENTO/4_11__4_12.sql:88',
    branchId: 'ora',
  },
];

// ── Sample script text (file editor tab) ─────────────────────────────────────

export const MOCK_FILE_TEXT: Record<string, string> = {
  'ORACLE/AGGIORNAMENTO/4_12__4_13.sql': `-- Aggiornamento 4.12 -> 4.13
-- Parametri di sconto introdotti con la revisione listini.

DECLARE
  v_versione VARCHAR2(10);
  v_presenti NUMBER;
BEGIN
  SELECT VERSIONE INTO v_versione FROM VERSIONE_DB;
  IF v_versione <> '4.12' THEN
    RETURN;
  END IF;

  SELECT COUNT(*) INTO v_presenti FROM PARAMETRI
   WHERE COD_PARAMETRO = 'SOGLIA_SCONTO';
  IF v_presenti = 0 THEN
    INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE, DATA_MOD)
    VALUES ('SOGLIA_SCONTO', '15', 'Soglia sconto massimo applicabile', SYSDATE);
  END IF;

  UPDATE VERSIONE_DB SET VERSIONE = '4.13', DATA_AGG = SYSDATE;
  COMMIT;
END;
/
`,
};

export const DEFAULT_QUERY_TEXT = `-- parameters changed in the last 30 days
SELECT COD_PARAMETRO, VALORE, DESCRIZIONE, DATA_MOD
  FROM PARAMETRI
 WHERE DATA_MOD >= SYSDATE - 30
 ORDER BY DATA_MOD DESC;
`;

export const MOCK_PASTED_SQL = `INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
VALUES ('SOGLIA_SCONTO', '15', 'Soglia sconto massimo applicabile');

INSERT INTO PARAMETRI (COD_PARAMETRO, VALORE, DESCRIZIONE)
VALUES ('SOGLIA_RESO', '7', 'Giorni entro cui accettare un reso');`;

export const MOCK_CSV = `COD_PARAMETRO;VALORE;DESCRIZIONE
SOGLIA_SCONTO;15;Soglia sconto massimo applicabile
SOGLIA_RESO;7;Giorni entro cui accettare un reso
MAX_RATE;12;Numero massimo di rate
GIORNI_PREAVVISO;30;Giorni di preavviso per il recesso`;
