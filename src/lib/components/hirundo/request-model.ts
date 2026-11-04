/** Small pure helpers over the Hirundo wire types: blank values, tree walks, display words. */

import type { Auth, CollectionNode, EnvType, HttpRequest, KeyValue, Method, Node } from '$lib/types/hirundo';

export function blankRequest(name = 'New request'): HttpRequest {
  return {
    name, order: 0, method: 'GET', url: '', params: [], headers: [], auth: { type: 'inherit' },
    body: { kind: 'none', text: '', form: [] },
    soap: { version: '1.1', action: '' },
    settings: { timeout_ms: 30000, follow_redirects: true, verify_tls: true, cookie_jar: true, stream: false },
    docs: '', tests: '',
  };
}

/** A SOAP call to start from: a POST whose body is an (empty) envelope. */
export function blankSoapRequest(name = 'New SOAP request'): HttpRequest {
  const request = blankRequest(name);
  request.method = 'POST';
  request.body = { kind: 'soap', text: SAMPLE_ENVELOPE, form: [] };
  return request;
}

export const SAMPLE_ENVELOPE = `<soapenv:Envelope xmlns:soapenv="http://schemas.xmlsoap.org/soap/envelope/" xmlns:tns="http://example.com/service">
   <soapenv:Header/>
   <soapenv:Body>
      <tns:Operation>
      </tns:Operation>
   </soapenv:Body>
</soapenv:Envelope>`;

export function blankRow(): KeyValue {
  return { key: '', value: '', description: '', enabled: true };
}

export function cloneRequest(r: HttpRequest): HttpRequest {
  return JSON.parse(JSON.stringify(r)) as HttpRequest;
}

export function sameRequest(a: HttpRequest, b: HttpRequest): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** CSS colour of a method, from the theme's status tokens. */
/** Whether Send should listen to the request as a stream: set explicitly, or asked for by its `Accept` header. */
export function isStreamRequest(r: HttpRequest): boolean {
  return r.settings.stream || r.headers.some((h) => h.enabled && h.key.toLowerCase() === 'accept' && h.value.toLowerCase().includes('text/event-stream') && !h.value.includes(','));
}

export function methodColor(m: Method): string {
  switch (m) {
    case 'GET': return 'var(--success)';
    case 'POST': return 'var(--warning)';
    case 'PUT': return 'var(--accent)';
    case 'PATCH': return 'var(--purple, var(--accent))';
    case 'DELETE': return 'var(--error)';
    default: return 'var(--text-secondary)';
  }
}

/** Three-letter label the tree and the tabs use (`DELETE` → `DEL`). */
export function methodShort(m: Method): string {
  return m === 'DELETE' ? 'DEL' : m === 'OPTIONS' ? 'OPT' : m === 'PATCH' ? 'PAT' : m;
}

export function authLabel(a: Auth): string {
  switch (a.type) {
    case 'inherit': return 'Inherit';
    case 'none': return 'No auth';
    case 'bearer': return 'Bearer';
    case 'basic': return 'Basic';
    case 'api_key': return 'API key';
    case 'ws_security': return 'WS-Security';
  }
}

/** Every request under a node, in tree order. */
export function requestsUnder(children: Node[]): Extract<Node, { kind: 'request' }>[] {
  return children.flatMap((n) => (n.kind === 'request' ? [n] : requestsUnder(n.children)));
}

/** The collection or folder node at `path`, or the collection itself. */
export function findContainer(tree: CollectionNode[], path: string): { name: string; children: Node[] } | null {
  for (const c of tree) {
    if (c.path === path) return c;
    const hit = findIn(c.children, path);
    if (hit) return hit;
  }
  return null;
}

function findIn(nodes: Node[], path: string): { name: string; children: Node[] } | null {
  for (const n of nodes) {
    if (n.kind !== 'folder') continue;
    if (n.path === path) return n;
    const hit = findIn(n.children, path);
    if (hit) return hit;
  }
  return null;
}

/** The collection a path belongs to (`bandi/auth/login.toml` → `bandi`). */
export function collectionOf(path: string): string {
  return path.split('/')[0] ?? path;
}

/** A tree row for the flat fuzzy search. */
export interface FlatRequest {
  path: string;
  name: string;
  method: Method;
  /** `Bandi / Auth`: where it lives, for the search's second column. */
  where: string;
}

export function flattenRequests(tree: CollectionNode[]): FlatRequest[] {
  const out: FlatRequest[] = [];
  const walk = (nodes: Node[], trail: string[]) => {
    for (const n of nodes) {
      if (n.kind === 'request') out.push({ path: n.path, name: n.name, method: n.method, where: trail.join(' / ') });
      else walk(n.children, [...trail, n.name]);
    }
  };
  for (const c of tree) walk(c.children, [c.name]);
  return out;
}

/** The colour of an environment's type, or a neutral one when it has none. */
export function typeColor(types: EnvType[], typeId: string): string {
  return types.find((t) => t.id === typeId)?.color ?? 'var(--text-muted)';
}

/** A file-name-safe id from a free name (`Staging EU` → `staging-eu`). */
export function idFromName(name: string): string {
  return name.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 48);
}

/** The first `{{name}}` placeholders in a text, for the variable highlight. */
export function splitVariables(text: string): { text: string; variable: boolean }[] {
  const parts: { text: string; variable: boolean }[] = [];
  const re = /\{\{\s*[\w.\-]+\s*\}\}/g;
  let last = 0;
  for (const m of text.matchAll(re)) {
    if (m.index > last) parts.push({ text: text.slice(last, m.index), variable: false });
    parts.push({ text: m[0], variable: true });
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push({ text: text.slice(last), variable: false });
  return parts;
}

/** The URL as the bar shows it: the base plus the enabled params as a query. */
export function urlWithQuery(r: HttpRequest): string {
  const q = r.params.filter((p) => p.enabled && p.key && !p.key.startsWith('path:')).map((p) => (p.value ? `${p.key}=${p.value}` : p.key)).join('&');
  return q ? `${r.url}?${q}` : r.url;
}

/**
 * The inverse, for typing in the bar: what follows `?` becomes the enabled params (keeping a description a
 * key already had), and params switched off are kept, since the bar has no way to show them.
 */
export function applyUrl(r: HttpRequest, text: string): void {
  const at = text.indexOf('?');
  r.url = at === -1 ? text : text.slice(0, at);
  // Path variables (`path:id`) are not part of the query: they stay as they are.
  const pathRows = r.params.filter((p) => p.key.startsWith('path:'));
  const disabled = r.params.filter((p) => !p.enabled && !p.key.startsWith('path:'));
  const described = new Map(r.params.map((p) => [p.key, p.description]));
  const typed: KeyValue[] = at === -1 ? [] : text.slice(at + 1).split('&').filter(Boolean).map((pair) => {
    const eq = pair.indexOf('=');
    const key = eq === -1 ? pair : pair.slice(0, eq);
    return { key, value: eq === -1 ? '' : pair.slice(eq + 1), description: described.get(key) ?? '', enabled: true };
  });
  r.params = [...pathRows, ...typed, ...disabled];
}

/** One row of the collections tree, uniform across collections, folders and requests (what the shared Tree walks). */
export interface TreeRow {
  id: string;
  kind: 'collection' | 'folder' | 'request';
  name: string;
  method?: Method;
  /** Label of the auth set here ("Bearer"), or "Inherit". */
  auth: string;
  error: string | null;
  children: TreeRow[];
}

export function toTreeRows(tree: CollectionNode[]): TreeRow[] {
  const nodes = (list: Node[]): TreeRow[] =>
    list.map((n) =>
      n.kind === 'folder'
        ? { id: n.path, kind: 'folder' as const, name: n.name, auth: n.auth, error: null, children: nodes(n.children) }
        : { id: n.path, kind: 'request' as const, name: n.name, method: n.method, auth: '', error: n.error, children: [] },
    );
  return tree.map((c) => ({ id: c.path, kind: 'collection' as const, name: c.name, auth: c.auth, error: null, children: nodes(c.children) }));
}

/** CSS colour of a workspace, from the theme's `--ws-color-N` tokens (the same ones Bennu and Corvus use). */
export function wsColor(idx: number): string {
  return `var(--ws-color-${Math.max(0, Math.min(7, Math.floor(idx || 0)))})`;
}

/** A plausible value for a parameter of this Java type, so the field is not empty and shows what is expected. */
export function sampleFor(type: string): string {
  const t = type.replace(/\s+/g, '').replace(/^java\.(lang|util)\./, '');
  if (/^(Long|long|Integer|int|Short|short|BigInteger)$/.test(t)) return '1';
  if (/^(Double|double|Float|float|BigDecimal)$/.test(t)) return '1.0';
  if (/^(Boolean|boolean)$/.test(t)) return 'true';
  if (/^UUID$/.test(t)) return '{{$uuid}}';
  if (/^(LocalDate)$/.test(t)) return '2025-01-31';
  if (/^(LocalDateTime|Instant|OffsetDateTime|ZonedDateTime|Date)$/.test(t)) return '{{$isoTimestamp}}';
  return '';
}

export const WS_COLOR_COUNT = 8;

/**
 * The current value of a theme colour token as the `#rrggbb` the env-type file stores (a file cannot hold a CSS
 * variable), so a new type takes its colour from the theme instead of a literal in the code.
 */
export function themeHex(token: string, fallback = '#808080'): string {
  if (typeof document === 'undefined') return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  return /^#[0-9a-f]{6}$/i.test(v) ? v : fallback;
}
