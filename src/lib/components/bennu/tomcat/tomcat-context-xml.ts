/**
 * Reading a Tomcat `context.xml`.
 *
 * **Reading** is what makes "paste whatever the other server had" work: a `context.xml`, a
 * `<Resource>` copied out of `server.xml`, a `GlobalNamingResources` block. Anything that names a
 * data source, an environment entry or a parameter becomes a resource of the form; everything else
 * in the text (Valves, Managers, `<Loader>`) is ignored on purpose — the form models what a
 * development launch needs, and a "raw" context exists for the rest.
 *
 * The same reading carries the raw text back to the guided form: {@link mergeImport} keeps what
 * the XML cannot say (the linked Picus connection, a stored password), and
 * {@link unmodeledTags} names what the form would drop.
 */

import { newResource, type TomcatResource } from '$lib/stores/bennu/tomcat-config';

export interface ContextImport {
  resources: TomcatResource[];
  /** Passwords found in the text, by the id of the resource they belong to. They go to the secret
   *  store, not into the configuration. */
  passwords: Map<string, string>;
  /** Why nothing was read, when that is the case. */
  error: string | null;
}

/** The reading's `error` when the text is valid but holds nothing the form models. */
export const NOTHING_FOUND = 'No data source, environment entry or parameter found in that text.';

/** A value that is Tomcat's own `${property}` substitution, not a password. */
const isPlaceholder = (v: string) => v.includes('${');

/** The text as a document under one wrapper element (a paste may hold several roots), or why not. */
function parseDocument(text: string): { doc: Document } | { error: string } {
  const body = text.replace(/<\?xml[\s\S]*?\?>/g, '');
  const doc = new DOMParser().parseFromString(`<root>${body}</root>`, 'application/xml');
  const broken = doc.querySelector('parsererror');
  if (broken) return { error: `Not valid XML — ${(broken.textContent ?? '').split('\n')[0].slice(0, 120)}` };
  return { doc };
}

/** A pool size or a wait read from an attribute: any whole number, `-1` (no limit) and `0` included, and the default for what is absent or not a number. */
const whole = (text: string, fallback: number) => (text.trim() !== '' && Number.isInteger(Number(text)) ? Number(text) : fallback);

const isDataSource = (el: Element) => /DataSource/i.test(el.getAttribute('type') ?? '') || !!el.getAttribute('url');

export function parseContextXml(text: string): ContextImport {
  const empty = (error: string): ContextImport => ({ resources: [], passwords: new Map(), error });
  if (!text.trim()) return empty('Paste some XML first.');

  const parsed = parseDocument(text);
  if ('error' in parsed) return empty(parsed.error);
  const { doc } = parsed;

  const resources: TomcatResource[] = [];
  const passwords = new Map<string, string>();
  const attr = (el: Element, name: string) => el.getAttribute(name) ?? '';

  doc.querySelectorAll('Resource, Environment, Parameter').forEach((el) => {
    if (el.tagName === 'Resource') {
      // Only a data source: a mail session or a queue is something this form cannot fulfil.
      if (!isDataSource(el)) return;
      const password = attr(el, 'password');
      const r = newResource('datasource', {
        name: attr(el, 'name'),
        driver: attr(el, 'driverClassName'),
        url: attr(el, 'url'),
        user: attr(el, 'username'),
        // Imported passwords go to the stored-secret side: pasting a context that carried one
        // should not put it into the file the repository shares.
        passwordMode: 'secret',
        maxTotal: whole(attr(el, 'maxTotal') || attr(el, 'maxActive'), 20),
        maxIdle: whole(attr(el, 'maxIdle'), 5),
        maxWaitMs: whole(attr(el, 'maxWaitMillis') || attr(el, 'maxWait'), 10000),
        validationQuery: attr(el, 'validationQuery'),
        testOnBorrow: attr(el, 'testOnBorrow') !== 'false',
      });
      if (password && !isPlaceholder(password)) passwords.set(r.id, password);
      resources.push(r);
    } else if (el.tagName === 'Environment') {
      resources.push(
        newResource('env', {
          name: attr(el, 'name'),
          value: attr(el, 'value'),
          valueType: attr(el, 'type') || 'java.lang.String',
          allowOverride: attr(el, 'override') === 'true',
        }),
      );
    } else {
      resources.push(newResource('param', { name: attr(el, 'name'), value: attr(el, 'value') }));
    }
  });

  if (!resources.length) return empty(NOTHING_FOUND);
  return { resources, passwords, error: null };
}

/** The elements of `text` the guided form cannot show: anything but the context itself, its
 *  environment entries, parameters and data sources. Empty when the text is not valid XML — the
 *  reading says so on its own. */
export function unmodeledTags(text: string): string[] {
  const parsed = parseDocument(text);
  if ('error' in parsed) return [];
  const out = new Set<string>();
  parsed.doc.querySelectorAll('root *').forEach((el) => {
    const tag = el.tagName;
    if (tag === 'Context' || tag === 'Environment' || tag === 'Parameter') return;
    if (tag === 'Resource' && isDataSource(el)) return;
    out.add(tag);
  });
  return [...out];
}

/**
 * The resources of `read` (the raw text, read back) as the form should hold them.
 *
 * A resource is the same one when its kind and name match, and then it keeps what the XML cannot
 * carry — its id (a stored password is filed under it), the linked Picus
 * connection and how its password is kept. A literal password found in the text goes to the secret
 * store under the merged id, never into the configuration.
 */
export function mergeImport(
  current: TomcatResource[],
  read: ContextImport,
): { resources: TomcatResource[]; passwords: Map<string, string> } {
  const key = (r: TomcatResource) => `${r.kind}\u0000${r.name}`;
  const byKey = new Map(current.map((r) => [key(r), r]));
  const passwords = new Map<string, string>();
  const resources = read.resources.map((r) => {
    const old = byKey.get(key(r));
    const merged: TomcatResource = old
      ? {
          ...r,
          id: old.id,
          picusConnection: old.picusConnection,
          passwordMode: old.passwordMode,
          password: old.password,
        }
      : r;
    const typed = read.passwords.get(r.id);
    if (typed !== undefined) passwords.set(merged.id, typed);
    return merged;
  });
  return { resources, passwords };
}
