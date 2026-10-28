/**
 * Bennu ⇄ Picus — the database a project's persistence layer is checked against.
 *
 * Bennu knows what the Java side *declares* (entities, native queries); Picus knows what the
 * database *is*. This store is the join: it borrows `picus-be` (the connections Picus defines, its
 * schema reader, its statement validator) and compares. Nothing here duplicates a Picus feature —
 * if the Picus window is open on the same connection, the session is the same session.
 *
 * Everything is gated on `available`: the tool window and its palette entries exist only when the
 * Picus backend ships with the install.
 *
 * The chosen connection is session-only for now. A per-project setting belongs in the project's
 * `.arbor/config.toml` and needs the bennu config surface extended — not a `localStorage` key.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { ensureBackend, isProductInstalled } from '$lib/ipc/rpc';
// The Picus stores the abbreviation expander reads its schema and connection from. They are fed
// from here, so the Database window stays the one place a connection is chosen and opened.
import { connectionsStore } from '$lib/stores/picus/connections.svelte';
import { schemaStore } from '$lib/stores/picus/schema.svelte';
import {
  connect as connectSession,
  listConnections,
  readSchema,
  validateSql,
  type ConnectionRow,
} from '$lib/ipc/picus/db';
import {
  mybatisFileStatements,
  mybatisStatements,
  nativeSqlSites,
  persistenceModel,
  type MybatisStatement,
  type NativeQuerySite,
  type PersistenceModel,
} from '$lib/ipc/bennu/persistence';
import { checkEntities, type EntityCheck } from '$lib/components/bennu/picus/entity-check';
import { prepareNative } from '$lib/components/bennu/picus/native-sql';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
import type { TableInfo } from '$lib/types/picus';

/** The identity of a MyBatis statement within a project. */
export function mybatisKey(s: Pick<MybatisStatement, 'file' | 'id' | 'database_id'>): string {
  return `${s.file}|${s.id}|${s.database_id}`;
}

/** The identity of a native query site within a project. */
export function nativeKey(q: Pick<NativeQuerySite, 'file' | 'repository' | 'method'>): string {
  return `${q.file}|${q.repository}.${q.method}`;
}

export type NativeOutcome =
  | { state: 'checking' }
  | { state: 'ok'; params: number }
  | { state: 'rejected'; messages: string[] }
  /** The connection failed mid-check — distinct from a clean result, which is what silence would read as. */
  | { state: 'unchecked'; reason: string };

/** A finding of one SQL string, relative to the start of its contents (UTF-8 bytes). */
interface SiteFinding { from: number; to: number; message: string }

const utf8 = new TextEncoder();

/** UTF-16 index in `s` of the UTF-8 byte offset `byte`. */
function charOfByte(s: string, byte: number): number {
  let bytes = 0;
  for (let i = 0; i < s.length; i++) {
    if (bytes >= byte) return i;
    const cp = s.codePointAt(i)!;
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
    if (cp > 0xffff) i++;
  }
  return s.length;
}

/** Java escapes that would reach the database as backslashes, swapped for same-length blanks so offsets hold. */
function unescapeJava(text: string): string {
  return text.replace(/\\[ntr]/g, '  ').replace(/\\"/g, ' "');
}

function createBennuPicusStore() {
  let available = $state(false);
  let probed = $state(false);
  let connections = $state<ConnectionRow[]>([]);
  let selectedId = $state('');
  let tables = $state<TableInfo[]>([]);
  let model = $state<PersistenceModel | null>(null);
  let loading = $state(false);
  let error = $state('');
  let native = $state<Record<string, NativeOutcome>>({});
  let mybatis = $state<MybatisStatement[]>([]);
  let mybatisOutcome = $state<Record<string, NativeOutcome>>({});
  let generating = $state<{ table: string } | null>(null);
  /** The schema read in flight, so concurrent completions share it. Plain: nothing renders it. */
  let schemaLoad: Promise<void> | null = null;
  /** The project the selection belongs to, and the last `followProject` input (plain: nothing renders them). */
  let projectRoot = '';
  let followed = '';
  /** What the database said about one SQL string, by connection and text — see `nativeDiagnostics`. */
  const lintCache = new Map<string, SiteFinding[]>();
  /** What the database said about one rendered mapper statement, by connection and SQL — see `mapperDiagnostics`. */
  const mapperCache = new Map<string, string>();

  const selected = $derived(connections.find((c) => c.id === selectedId) ?? null);
  const connected = $derived(selected?.state === 'connected' || selected?.state === 'read-only');
  const checks = $derived<EntityCheck[]>(model ? checkEntities(model.entities, tables) : []);

  /** A statement for every database, or for this connection's engine (`databaseId="postgres"`). */
  function forThisEngine(s: Pick<MybatisStatement, 'database_id'>): boolean {
    if (!s.database_id) return true;
    const engine = selected?.engine ?? '';
    const id = s.database_id.toLowerCase();
    return engine === 'postgres' ? /postgres|pgsql|pg/.test(id) : id.includes(engine);
  }

  /** What was read from the previous connection's database (or project) — none of it describes the next. */
  function clearData() {
    tables = [];
    model = null;
    native = {};
    mybatis = [];
    mybatisOutcome = {};
    error = '';
    lintCache.clear();
    mapperCache.clear();
  }

  /** Read the project's entities and native queries again — without touching the verdicts already given. */
  async function reloadModel() {
    if (!projectRoot) return;
    try {
      model = await persistenceModel(projectRoot);
    } catch {
      /* keep the list we have: the check below then runs on what was last read */
    }
  }

  /** Render the project's MyBatis statements again, with the values given for `${…}`. */
  async function reloadStatements() {
    if (!projectRoot) return;
    try {
      mybatis = (await mybatisStatements(projectRoot, bennuSettingsStore.mybatisSplicesFor(projectRoot))).filter(forThisEngine);
    } catch {
      /* same: check what was last read */
    }
  }

  /** Pair the open project with the chosen connection, so the next visit starts from it. */
  function remember() {
    if (projectRoot && selectedId) bennuSettingsStore.bindPicusConnection(projectRoot, selectedId);
  }

  /** Run `work` with `loading` set, turning a failure into `error` instead of an unhandled rejection. */
  async function guarded(work: () => Promise<void>): Promise<void> {
    loading = true;
    error = '';
    try {
      await work();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  return {
    get available() { return available; },
    get probed() { return probed; },
    get connections() { return connections; },
    get selectedId() { return selectedId; },
    get selected() { return selected; },
    get connected() { return connected; },
    get tables() { return tables; },
    get model() { return model; },
    get checks() { return checks; },
    get loading() { return loading; },
    get error() { return error; },
    get native() { return native; },
    get mybatis() { return mybatis; },
    get mybatisOutcome() { return mybatisOutcome; },

    /** Ask the shell whether Picus ships with this install. Cheap, and idempotent after the first. */
    async probe(): Promise<void> {
      if (probed) return;
      try {
        available = await isProductInstalled('picus');
      } catch {
        available = false;
      }
      probed = true;
    },

    /** Read the connections Picus has defined. Brings the backend up on first use. */
    async loadConnections(): Promise<void> {
      if (!available) return;
      await guarded(async () => {
        await ensureBackend('picus');
        connections = await listConnections();
        // No fallback to "the first one": a connection is the user's choice for a project, and an
        // arbitrary default is how the wrong database ends up validating a project's queries.
        if (!connections.some((c) => c.id === selectedId)) selectedId = '';
        await connectionsStore.load();
      });
    },

    /** Whether the open project remembers a connection — the Database window says so, and offers to forget it. */
    get bound() { return !!projectRoot && bennuSettingsStore.picusConnectionFor(projectRoot) === selectedId && !!selectedId; },

    select(id: string) {
      if (id === selectedId) return;
      selectedId = id;
      clearData();
      // Choosing a connection that is already open is as much "this project uses it" as opening it.
      if (selected && (selected.state === 'connected' || selected.state === 'read-only')) remember();
    },

    /** Forget which connection this project uses: nothing is validated against a database until one is chosen again. */
    unbind() {
      if (projectRoot) bennuSettingsStore.bindPicusConnection(projectRoot, null);
      selectedId = '';
      clearData();
    },

    /**
     * Follow the open project. A connection is chosen by hand the first time; from then on the
     * project remembers it, and coming back to the project brings that connection (and its checks)
     * back. A project with no remembered connection has none selected — whatever the previous
     * project used must never be what this one's queries are validated against.
     */
    async followProject(root: string, isAvailable: boolean): Promise<void> {
      const wanted = root ? bennuSettingsStore.picusConnectionFor(root) : '';
      // The pairing is part of the key: the config may load after the project does.
      const key = `${root}|${isAvailable}|${wanted}`;
      if (key === followed) return;
      followed = key;
      // The pairing just written by `remember` on this very project: nothing to switch.
      if (root === projectRoot && wanted && wanted === selectedId) return;
      projectRoot = root;
      clearData();
      // Kept even before the connection list is read: `loadConnections` resolves it against the list.
      selectedId = wanted;
      if (!wanted || !isAvailable) return;
      await this.loadConnections();
      if (followed !== key) return; // another project came up while the list loaded
      if (selected && !connected) await this.connect();
    },

    /** Open a session on the chosen connection (the password comes from Arbor's keychain). */
    async connect(): Promise<void> {
      if (!selected) return;
      const id = selected.id;
      await guarded(async () => {
        const status = await connectSession(id);
        connections = connections.map((c) => (c.id === id ? { ...c, state: status.state } : c));
        await connectionsStore.load();
        if (selectedId === id && (status.state === 'connected' || status.state === 'read-only')) remember();
      });
    },

    /** Re-read the schema and the project's persistence model, and drop the stale native results. */
    async refresh(root: string): Promise<void> {
      if (!selected || !connected) return;
      const id = selected.id;
      await guarded(async () => {
        const [schema, m, statements] = await Promise.all([
          readSchema(id),
          persistenceModel(root),
          // Optional: a project without MyBatis, or a config graph still building, answers empty.
          mybatisStatements(root, bennuSettingsStore.mybatisSplicesFor(root)).catch(() => [] as MybatisStatement[]),
        ]);
        tables = [...schema.tables, ...schema.views];
        model = m;
        mybatis = statements.filter(forThisEngine);
        native = {};
        mybatisOutcome = {};
      });
    },

    /**
     * What the database says about the native SQL strings of a Java buffer, placed where they are
     * written — the editor's red squiggles. Each distinct string is asked once per connection; an
     * edit re-asks only about the string it touched. Nothing is reported when the database cannot be
     * asked (no connection, a failed prepare of the session): silence there is "unknown", and the
     * squiggles are for what the server actually rejected.
     */
    async nativeDiagnostics(file: string, source: string): Promise<{ from: number; to: number; message: string }[]> {
      if (!selected || !connected) return [];
      const id = selected.id;
      const engine = selected.engine;
      const sites = await nativeSqlSites(file, source).catch(() => []);
      const out: { from: number; to: number; message: string }[] = [];
      for (const site of sites) {
        const key = `${id}|${site.text}`;
        let found = lintCache.get(key);
        if (!found) {
          const text = unescapeJava(site.text);
          const prepared = prepareNative(text, engine);
          try {
            const findings = await validateSql(id, prepared.sql, true);
            found = findings.map((f) => {
              const from = prepared.toOriginal(charOfByte(prepared.sql, f.start));
              const to = Math.max(from + 1, prepared.toOriginal(charOfByte(prepared.sql, f.end)));
              return {
                from: utf8.encode(text.slice(0, from)).length,
                to: utf8.encode(text.slice(0, to)).length,
                message: f.message,
              };
            });
          } catch {
            continue; // could not ask — not cached, so the next edit tries again
          }
          if (lintCache.size > 500) lintCache.clear();
          lintCache.set(key, found);
        }
        for (const f of found) out.push({ from: site.start + f.from, to: site.start + f.to, message: f.message });
      }
      return out;
    },

    /** Read the entities and native queries again, keeping the verdicts. What a tab does when it is
     *  shown, so it never presents the project as it was when the window last refreshed. */
    refreshModel: reloadModel,

    /**
     * What the database says about the statements of a mapper in the editor, placed on each statement's
     * opening tag. A statement is rendered to plain SQL first (so a finding inside it has no place in the XML),
     * and one the renderer cannot make preparable — a `${…}` without a value, a fragment from another mapper —
     * is left alone rather than reported. Asked once per distinct SQL: editing one statement re-asks about that one.
     */
    async mapperDiagnostics(source: string): Promise<{ from: number; to: number; message: string }[]> {
      if (!selected || !connected) return [];
      const id = selected.id;
      const statements = await mybatisFileStatements(source, bennuSettingsStore.mybatisSplicesFor(projectRoot)).catch(() => []);
      const out: { from: number; to: number; message: string }[] = [];
      for (const s of statements) {
        if (s.unchecked || !forThisEngine(s)) continue;
        const key = `${id}|${s.sql}`;
        let message = mapperCache.get(key);
        if (message === undefined) {
          try {
            message = (await validateSql(id, s.sql, true)).map((f) => f.message).join('\n');
          } catch {
            continue; // could not ask — not cached, so the next edit tries again
          }
          if (mapperCache.size > 500) mapperCache.clear();
          mapperCache.set(key, message);
        }
        if (message) out.push({ from: s.start, to: s.end, message });
      }
      return out;
    },

    /** Give a MyBatis `${name}` a value and render the statements again with it. */
    async setSplice(root: string, name: string, value: string): Promise<void> {
      bennuSettingsStore.setMybatisSplice(root, name, value);
      try {
        mybatis = (await mybatisStatements(root, bennuSettingsStore.mybatisSplicesFor(root))).filter(forThisEngine);
        mybatisOutcome = {};
      } catch (e) {
        error = String(e);
      }
    },

    /** The project has JPA / Hibernate — the entity tabs and the generator are offered only then. */
    get jpa() { return model?.jpa ?? false; },

    /** The table the entity generator is open on (`''` for none picked yet), or `null` while closed. */
    get generating() { return generating; },
    openGenerate(table = '') { generating = { table }; },
    closeGenerate() { generating = null; },

    /**
     * Make sure the schema is in memory, reading it once for as long as the connection stays
     * chosen. What completion calls: the editor must be able to ask without the Database tool
     * window ever having been opened, and a burst of keystrokes must share one read.
     */
    ensureSchema(): Promise<void> {
      if (!selected || !connected) return Promise.resolve();
      if (tables.length) return Promise.resolve();
      const id = selected.id;
      void schemaStore.ensure(id);
      schemaLoad ??= readSchema(id)
        .then((schema) => {
          // A different connection may have been picked while this was in flight.
          if (selectedId === id) tables = [...schema.tables, ...schema.views];
        })
        .catch(() => {})
        .finally(() => { schemaLoad = null; });
      return schemaLoad;
    },

    /** Prepare one native query on the server; the outcome lands in {@link native}. */
    async checkNative(listed: NativeQuerySite, reload = true): Promise<void> {
      if (!selected || !connected) return;
      const key = nativeKey(listed);
      // The query is read again as it is NOW: the list was made when the window last refreshed, and a
      // query edited since would otherwise be checked as it used to be.
      if (reload) await reloadModel();
      const site = model?.native_queries.find((q) => nativeKey(q) === key);
      if (!site) {
        native = { ...native, [key]: { state: 'unchecked', reason: 'This query no longer exists in the project.' } };
        return;
      }
      const { sql, paramCount } = prepareNative(site.text, selected.engine);
      native = { ...native, [key]: { state: 'checking' } };
      try {
        const findings = await validateSql(selected.id, sql, true);
        native = {
          ...native,
          [key]: findings.length
            ? { state: 'rejected', messages: findings.map((f) => f.message) }
            : { state: 'ok', params: paramCount },
        };
      } catch (e) {
        native = { ...native, [key]: { state: 'unchecked', reason: String(e) } };
      }
    },

    /** Prepare one MyBatis statement. One that cannot be prepared says why instead of staying silent. */
    async checkMybatis(listed: MybatisStatement, reload = true): Promise<void> {
      if (!selected || !connected) return;
      const key = mybatisKey(listed);
      // Rendered again from the mapper as it is now, for the same reason as a native query.
      if (reload) await reloadStatements();
      const stmt = mybatis.find((s) => mybatisKey(s) === key);
      if (!stmt) {
        mybatisOutcome = { ...mybatisOutcome, [key]: { state: 'unchecked', reason: 'This statement no longer exists in the mapper.' } };
        return;
      }
      if (stmt.unchecked) {
        mybatisOutcome = { ...mybatisOutcome, [key]: { state: 'unchecked', reason: stmt.unchecked } };
        return;
      }
      mybatisOutcome = { ...mybatisOutcome, [key]: { state: 'checking' } };
      try {
        const findings = await validateSql(selected.id, stmt.sql, true);
        mybatisOutcome = {
          ...mybatisOutcome,
          [key]: findings.length
            ? { state: 'rejected', messages: findings.map((f) => f.message) }
            : { state: 'ok', params: 0 },
        };
      } catch (e) {
        mybatisOutcome = { ...mybatisOutcome, [key]: { state: 'unchecked', reason: String(e) } };
      }
    },

    async checkAllMybatis(): Promise<void> {
      await reloadStatements();
      for (const stmt of [...mybatis]) {
        // eslint-disable-next-line no-await-in-loop
        await this.checkMybatis(stmt, false);
      }
    },

    /** Check every native query, one at a time — they share a session, and a burst of prepares
     *  against it is how a validator becomes the thing that stalls the database. */
    async checkAllNative(): Promise<void> {
      await reloadModel();
      for (const site of [...(model?.native_queries ?? [])]) {
        // eslint-disable-next-line no-await-in-loop
        await this.checkNative(site, false);
      }
    },
  };
}

export const bennuPicusStore = createBennuPicusStore();
