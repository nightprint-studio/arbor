/**
 * What the request fields offer while you type — pure functions of the text and the caret, so the same lists
 * serve the URL bar (CodeMirror), the header table (`SuggestInput`) and the "undefined variable" checks.
 *
 *   - `{{` → the variables that exist, each with where it comes from and its value;
 *   - a header name → the standard and the most used ones, with a line on what they do;
 *   - a header value → the usual values of that header.
 */
import type { HttpRequest, KeyValue } from '$lib/types/hirundo';
import { SECRET_MARK } from '$lib/types/hirundo';
import type { SuggestResult } from '$lib/components/shared/ui/SuggestInput.svelte';

export type VarScope = 'environment' | 'type' | 'local' | 'folder' | 'collection' | 'dynamic';

export interface VarInfo {
  name: string;
  /** The value, or the empty string for a secret (never shown). */
  value: string;
  scope: VarScope;
  secret: boolean;
  doc?: string;
}

export const SCOPE_LABEL: Record<VarScope, string> = {
  environment: 'environment',
  type: 'environment type',
  local: 'this machine',
  folder: 'folder',
  collection: 'collection',
  dynamic: 'dynamic',
};

/** The variables Hirundo makes up at send time. */
export const DYNAMIC_VARS: VarInfo[] = [
  { name: '$uuid', value: '', scope: 'dynamic', secret: false, doc: 'A new random UUID (v4) on every send.' },
  { name: '$timestamp', value: '', scope: 'dynamic', secret: false, doc: 'The current time in seconds since 1970.' },
  { name: '$isoTimestamp', value: '', scope: 'dynamic', secret: false, doc: 'The current time as an ISO-8601 string.' },
  { name: '$randomInt', value: '', scope: 'dynamic', secret: false, doc: 'A random whole number from 0 to 999.' },
];

/** Layers from the one that wins to the one that loses: the first to define a name is the one that counts. */
export function collectVariables(layers: { scope: VarScope; vars: readonly KeyValue[] }[]): VarInfo[] {
  const seen = new Set<string>();
  const out: VarInfo[] = [];
  for (const { scope, vars } of layers) {
    for (const v of vars) {
      if (!v.enabled || !v.key || seen.has(v.key)) continue;
      seen.add(v.key);
      const secret = v.value === SECRET_MARK;
      out.push({ name: v.key, value: secret ? '' : v.value, scope, secret, doc: v.description || undefined });
    }
  }
  for (const d of DYNAMIC_VARS) if (!seen.has(d.name)) out.push(d);
  return out;
}

/** What a variable is, in one line: where it comes from and what it holds. */
export function describeVariable(v: VarInfo): string {
  if (v.scope === 'dynamic') return v.doc ?? 'Made up at send time.';
  const where = `From the ${SCOPE_LABEL[v.scope]}`;
  return v.secret ? `${where} · secret (kept in the keyring)` : v.value ? `${where} · ${v.value.length > 60 ? `${v.value.slice(0, 57)}…` : v.value}` : `${where} · empty`;
}

/** The `{{partial` the caret is inside, if any. */
function openBrace(text: string, caret: number): { from: number; typed: string } | null {
  const before = text.slice(0, caret);
  const at = before.lastIndexOf('{{');
  if (at === -1 || before.indexOf('}}', at) !== -1) return null;
  const typed = before.slice(at + 2).trimStart();
  return /^[\w.$-]*$/.test(typed) ? { from: caret - typed.length, typed } : null;
}

/** `{{` → variables. The pick inserts the name and closes the braces. */
export function variableSuggest(text: string, caret: number, vars: readonly VarInfo[]): SuggestResult | null {
  const open = openBrace(text, caret);
  if (!open) return null;
  const q = open.typed.toLowerCase();
  const items = vars
    .filter((v) => v.name.toLowerCase().includes(q))
    .sort((a, b) => Number(b.name.toLowerCase().startsWith(q)) - Number(a.name.toLowerCase().startsWith(q)))
    .map((v) => ({ id: `${v.name}}}`, label: v.name, meta: SCOPE_LABEL[v.scope], description: describeVariable(v) }));
  const after = text.slice(caret).startsWith('}}') ? 2 : 0;
  return { from: open.from, to: caret + after, items };
}

/** The names `{{x}}` in a text. */
export function variableNames(text: string): string[] {
  return [...new Set([...text.matchAll(/\{\{\s*([\w.$-]+)\s*\}\}/g)].map((m) => m[1]))];
}

/** Every text of a request a variable can appear in. */
export function requestTexts(r: HttpRequest): string[] {
  return [
    r.url,
    ...r.params.flatMap((p) => (p.enabled ? [p.key, p.value] : [])),
    ...r.headers.flatMap((h) => (h.enabled ? [h.key, h.value] : [])),
    r.body.text,
    ...r.body.form.flatMap((f) => [f.key, f.value]),
  ];
}

/** The names the request's path rows (`path:id`) define: the URL's `{{id}}` has its value there. */
export function pathParamNames(r: HttpRequest): string[] {
  return r.params.filter((p) => p.enabled && p.key.startsWith('path:')).map((p) => p.key.slice(5));
}

/** What a request still needs, by what its own rows say: enabled rows marked "required" with nothing in them. */
export function missingRequired(r: HttpRequest): string[] {
  const rows = [...r.params, ...r.headers, ...(r.body.kind === 'form' || r.body.kind === 'multipart' ? r.body.form : [])];
  return rows
    .filter((row) => row.enabled && row.key && row.value.trim() === '' && /\brequired\b/.test(row.description) && !/\boptional\b/.test(row.description))
    .map((row) => row.key.replace(/^path:/, ''));
}

/** The variables a request uses, in order of first use, `$dynamic` ones included. */
export function usedVariables(r: HttpRequest): string[] {
  return [...new Set(requestTexts(r).flatMap(variableNames))];
}

/** The variables a set of texts use that nothing defines. */
export function undefinedVariables(texts: readonly string[], known: ReadonlySet<string>): string[] {
  return [...new Set(texts.flatMap(variableNames))].filter((n) => !known.has(n) && !n.startsWith('$'));
}

// ── Headers ─────────────────────────────────────────────────────────────────────────────────────────

export const HEADER_NAMES: { name: string; doc: string }[] = [
  { name: 'Accept', doc: 'The media types the client can handle.' },
  { name: 'Accept-Charset', doc: 'The character sets the client accepts.' },
  { name: 'Accept-Encoding', doc: 'The compressions the client accepts (gzip, br…).' },
  { name: 'Accept-Language', doc: 'The languages the client prefers.' },
  { name: 'Authorization', doc: 'Credentials: Bearer token, Basic user:password…' },
  { name: 'Cache-Control', doc: 'How caches may keep the answer (no-cache, max-age…).' },
  { name: 'Connection', doc: 'Whether the connection stays open (keep-alive, close).' },
  { name: 'Content-Disposition', doc: 'How a body is presented: inline or as an attachment.' },
  { name: 'Content-Encoding', doc: 'The compression applied to the body.' },
  { name: 'Content-Language', doc: 'The language of the body.' },
  { name: 'Content-Length', doc: 'The size of the body in bytes (set for you).' },
  { name: 'Content-Type', doc: 'The media type of the body you send.' },
  { name: 'Cookie', doc: 'Cookies sent back to the server (the jar adds them for you).' },
  { name: 'DNT', doc: 'Do Not Track preference.' },
  { name: 'ETag', doc: 'A version tag of a resource.' },
  { name: 'Forwarded', doc: 'Information about the proxies the request went through.' },
  { name: 'Host', doc: 'The host and port requested (set for you).' },
  { name: 'If-Match', doc: 'Only act if the resource still has this ETag.' },
  { name: 'If-Modified-Since', doc: 'Only send the body if it changed since this date.' },
  { name: 'If-None-Match', doc: 'Only send the body if its ETag differs.' },
  { name: 'Last-Event-ID', doc: 'Where a Server-Sent Events stream should resume from.' },
  { name: 'Origin', doc: 'Where a cross-origin request comes from.' },
  { name: 'Pragma', doc: 'Legacy cache directive (no-cache).' },
  { name: 'Range', doc: 'Ask for part of a resource (bytes=0-1023).' },
  { name: 'Referer', doc: 'The page that linked to this request.' },
  { name: 'SOAPAction', doc: 'The operation a SOAP 1.1 call performs.' },
  { name: 'TE', doc: 'Transfer encodings the client accepts.' },
  { name: 'User-Agent', doc: 'Names the client making the request.' },
  { name: 'X-API-Key', doc: 'An API key, by a common convention.' },
  { name: 'X-Correlation-Id', doc: 'An id that follows a request across services.' },
  { name: 'X-Forwarded-For', doc: 'The original client address behind proxies.' },
  { name: 'X-Request-Id', doc: 'An id for this request, for logs.' },
  { name: 'X-Requested-With', doc: 'XMLHttpRequest marker some servers check.' },
];

const MEDIA = ['application/json', 'application/xml', 'application/x-www-form-urlencoded', 'multipart/form-data', 'text/plain', 'text/html', 'text/event-stream', 'application/octet-stream', 'application/pdf', '*/*'];

export const HEADER_VALUES: Record<string, { value: string; doc: string }[]> = {
  accept: MEDIA.map((value) => ({ value, doc: 'A media type' })),
  'content-type': MEDIA.filter((m) => m !== '*/*').map((value) => ({ value: value === 'application/json' || value.startsWith('text/') || value.endsWith('xml') ? `${value}; charset=utf-8` : value, doc: 'A media type' })).concat([{ value: 'application/json', doc: 'A media type' }]),
  authorization: [
    { value: 'Bearer {{token}}', doc: 'A bearer token (OAuth 2, JWT).' },
    { value: 'Basic ', doc: 'Base64 of user:password.' },
    { value: 'Digest ', doc: 'Digest authentication.' },
  ],
  'cache-control': ['no-cache', 'no-store', 'max-age=0', 'max-age=3600', 'public', 'private', 'must-revalidate'].map((value) => ({ value, doc: 'A cache directive' })),
  'accept-encoding': ['gzip, deflate, br', 'gzip', 'identity'].map((value) => ({ value, doc: 'Accepted compressions' })),
  'accept-language': ['en-US,en;q=0.9', 'it-IT,it;q=0.9', '*'].map((value) => ({ value, doc: 'Preferred languages' })),
  connection: ['keep-alive', 'close'].map((value) => ({ value, doc: '' })),
  'x-requested-with': [{ value: 'XMLHttpRequest', doc: '' }],
  'user-agent': [{ value: 'Hirundo', doc: '' }],
};

function narrow<T>(rows: readonly T[], typed: string, text: (r: T) => string): T[] {
  const q = typed.trim().toLowerCase();
  const hits = rows.filter((r) => text(r).toLowerCase().includes(q));
  return hits.sort((a, b) => Number(text(b).toLowerCase().startsWith(q)) - Number(text(a).toLowerCase().startsWith(q)));
}

/** The header-name field: names that contain what is typed; a `{{` inside offers variables instead. */
export function headerNameSuggest(text: string, caret: number, vars: readonly VarInfo[]): SuggestResult | null {
  const v = variableSuggest(text, caret, vars);
  if (v) return v;
  if (!text.trim() && caret === 0) return { from: 0, to: 0, items: HEADER_NAMES.slice(0, 12).map((h) => ({ id: h.name, label: h.name, description: h.doc })) };
  const items = narrow(HEADER_NAMES, text, (h) => h.name).filter((h) => h.name.toLowerCase() !== text.trim().toLowerCase());
  return { from: 0, to: text.length, items: items.map((h) => ({ id: h.name, label: h.name, description: h.doc })) };
}

/** The header-value field: that header's usual values, or variables after `{{`. */
export function headerValueSuggest(header: string, text: string, caret: number, vars: readonly VarInfo[]): SuggestResult | null {
  const v = variableSuggest(text, caret, vars);
  if (v) return v;
  const rows = HEADER_VALUES[header.trim().toLowerCase()];
  if (!rows) return null;
  const items = narrow(rows, text, (r) => r.value).filter((r) => r.value !== text);
  return { from: 0, to: text.length, items: items.map((r) => ({ id: r.value, label: r.value, description: r.doc || undefined })) };
}
