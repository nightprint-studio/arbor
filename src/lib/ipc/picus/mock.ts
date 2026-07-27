/**
 * Picus fixtures — TEMPORARY stand-in for the parts of `picus-be` that don't
 * exist yet.
 *
 * The **database** half is real: connections, schema, table rows and query
 * execution all come from `picus-be` now, and their fixtures have been deleted
 * from this file rather than left to rot beside the working code.
 *
 * What remains is the **script** half — the repository tree, the inventory, the
 * consistency findings, a file's text, the generator's sample inputs — which the
 * UI still needs shapes for until `picus-parse` / `picus-inventory` /
 * `picus-analyze` land. Same staging as Tyto's mocked control panel before
 * `tyto-be` shipped.
 *
 * DELETE THIS FILE when those handlers exist. Every store reads it through a
 * single `mock*` accessor precisely so the swap is one import per store, not a
 * rewrite.
 */

import type {
  Branch,
  Finding,
  InventoryObject,
  Project,
  Target,
} from '$lib/types/picus';

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
