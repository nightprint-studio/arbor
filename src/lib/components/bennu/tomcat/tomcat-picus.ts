/**
 * Picus as a source of data sources, and as the one that checks them.
 *
 * Picus already knows the databases — host, port, database, user, and a password it keeps safe —
 * so a data source for Tomcat is, mostly, a translation: the same connection written as a JDBC
 * URL. And because Picus can open a connection, "does this work?" is a question for it, not for a
 * second connection stack inside Bennu.
 *
 * ## What this does not claim
 *
 * **The password does not come across.** Picus never hands a secret out of its backend, which is the
 * right rule, so an imported data source arrives without one and the form asks for it once.
 *
 * **The test is of what the form says.** The host, port, database and user are read back out of the
 * JDBC URL typed there and the password is the one typed there (or, for a data source taken from a
 * Picus connection and not changed since, the one Picus holds). Picus opens that connection and
 * closes it again. It proves the database answers with those credentials; it does not exercise the
 * JDBC driver. The form says so next to the button.
 */

import {
  deleteSecret,
  storeSecret,
  testConnection,
  type ConnectionRow,
  type ConnectionSpec,
} from '$lib/ipc/picus/db';
import { newResource, type TomcatResource } from '$lib/stores/bennu/tomcat-config';

export interface JdbcTarget {
  driver: string;
  url: string;
  validationQuery: string;
}

/** The connection as a JDBC target, or `null` for an engine this has no driver for. */
export function jdbcFor(c: ConnectionSpec): JdbcTarget | null {
  const schema = (c.schema ?? '').trim();
  switch (c.engine) {
    case 'postgres': {
      const query = [c.tls ? 'sslmode=require' : '', schema ? `currentSchema=${schema}` : '']
        .filter(Boolean)
        .join('&');
      return {
        driver: 'org.postgresql.Driver',
        url: `jdbc:postgresql://${c.host}:${c.port || 5432}/${c.database}${query ? `?${query}` : ''}`,
        validationQuery: 'SELECT 1',
      };
    }
    case 'oracle':
      // `database` is the service name — what Picus connects with — so the `//host/service` form.
      return {
        driver: 'oracle.jdbc.OracleDriver',
        url: `jdbc:oracle:thin:@//${c.host}:${c.port || 1521}/${c.database}`,
        validationQuery: 'SELECT 1 FROM DUAL',
      };
    default:
      return null;
  }
}

/** A data source named `name`, taken from `c`; `null` when its engine has no JDBC driver here. */
export function resourceFromPicus(c: ConnectionSpec, name: string): TomcatResource | null {
  const target = jdbcFor(c);
  if (!target) return null;
  return newResource('datasource', {
    name,
    driver: target.driver,
    url: target.url,
    user: c.user,
    validationQuery: target.validationQuery,
    picusConnection: c.id,
  });
}

/** A JNDI-style name from a connection's display name: `Gestionale (locale)` → `jdbc/GestionaleLocale`. */
export function jndiNameFor(c: ConnectionSpec): string {
  const words = c.name.split(/[^A-Za-z0-9]+/).filter(Boolean);
  const camel = words.map((w) => w[0].toUpperCase() + w.slice(1)).join('');
  return `jdbc/${camel || 'Database'}`;
}

export interface PicusTest {
  ok: boolean;
  /** `Connected · PostgreSQL 16.2`, or Picus's own reason. */
  message: string;
}

const POSTGRES_URL = /^jdbc:postgresql:\/\/([^/:?]+)(?::(\d+))?\/([^?]+)(?:\?(.*))?$/i;
const ORACLE_URL = /^jdbc:oracle:thin:@\/\/([^/:]+)(?::(\d+))?\/(.+)$/i;

/** The connection a JDBC URL names, as Picus would open it; `null` for a URL or an engine it cannot read. */
export function specFromJdbc(url: string, user: string, id: string): ConnectionSpec | null {
  const base = { id, name: 'Tomcat data source', alias: '', schema: '', colorIdx: 0, readOnly: true, tls: false, params: {}, user };
  const pg = POSTGRES_URL.exec(url.trim());
  if (pg) {
    const query = new URLSearchParams(pg[4] ?? '');
    return {
      ...base,
      engine: 'postgres',
      host: pg[1],
      port: Number(pg[2]) || 5432,
      database: pg[3],
      schema: query.get('currentSchema') ?? '',
      tls: /^(require|verify-ca|verify-full)$/i.test(query.get('sslmode') ?? ''),
    };
  }
  const ora = ORACLE_URL.exec(url.trim());
  if (ora) return { ...base, engine: 'oracle', host: ora[1], port: Number(ora[2]) || 1521, database: ora[3] };
  return null;
}

const answer = (status: { serverVersion: string }): PicusTest => ({
  ok: true,
  message: status.serverVersion ? `Connected · ${status.serverVersion}` : 'Connected',
});
const failure = (e: unknown): PicusTest => ({ ok: false, message: e instanceof Error ? e.message : String(e) });

/** Ask Picus to open a saved connection, report, and close it again (it touches nothing it had open). */
export async function testWithPicus(row: ConnectionRow): Promise<PicusTest> {
  // The spec only: the live state and the secret flag are the sidebar's, not part of what is tested.
  const { state: _state, serverVersion: _version, hasSecret: _secret, ...spec } = row;
  try {
    return answer(await testConnection(spec));
  } catch (e) {
    return failure(e);
  }
}

/** The same for a connection that exists only in this form: its password is filed under the spec's
 *  own id for the length of the test and forgotten again whatever the outcome. */
export async function testTyped(spec: ConnectionSpec, password: string): Promise<PicusTest> {
  try {
    await storeSecret(spec.id, password);
    return answer(await testConnection(spec));
  } catch (e) {
    return failure(e);
  } finally {
    await deleteSecret(spec.id).catch(() => {});
  }
}
