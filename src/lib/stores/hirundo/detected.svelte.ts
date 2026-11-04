/**
 * Hirundo › Detected from code — the endpoints Bennu's index holds for the projects the user chose.
 *
 * Only the chosen projects are ever asked about (never "all ten"): the choice is kept in the profile
 * (`detected_roots`), and until one is made the list is the Bennu project that is this window's own project.
 * A project that Bennu has not indexed yet answers with nothing, which is told apart from "no endpoints" by
 * the panel (the project must be opened in Bennu once).
 */
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { endpointsDetected } from '$lib/ipc/hirundo';
import { requiredPaths, skeletonText } from '$lib/components/hirundo/editor/schema';
import type { EndpointSchema } from '$lib/types/hirundo';
import { getBennuWorkspaces } from '$lib/ipc/bennu/config';
import { ensureBackend } from '$lib/ipc/rpc';
import type { DetectedEndpoint, HttpRequest } from '$lib/types/hirundo';
import { blankRequest, sampleFor } from '$lib/components/hirundo/request-model';
import { hirundoStore } from './workspace.svelte';

export interface BennuProject {
  root: string;
  name: string;
  workspace: string;
}

export interface ProjectEndpoints {
  root: string;
  name: string;
  endpoints: DetectedEndpoint[];
}

const base = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;
const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();

/** `{id}` in a Spring path is `{{id}}` in a request: a variable the user fills in. */
export function pathToTemplate(path: string): string {
  return path.replace(/\{([^}/]+)\}/g, '{{$1}}');
}

/** The media types of a `produces` / `consumes` value, one by one. */
function mediaTypes(text: string | undefined): string[] {
  return (text ?? '').split(',').map((m) => m.trim()).filter(Boolean);
}

/**
 * A request for a detected endpoint: method, URL on `{{baseUrl}}` (path variables as `{{id}}`, which the
 * request then asks you to fill in), its query parameters with their Java type and whether they are required,
 * headers and a JSON body when it takes one, `Accept` from what it produces and `Content-Type` from what it
 * consumes. A stream (`text/event-stream`) is said in the notes, since that is listened to, not awaited.
 */
export function endpointToRequest(e: DetectedEndpoint, schema: EndpointSchema | null = null): HttpRequest {
  const request = blankRequest(`${e.controller}.${e.handler}`);
  request.method = (e.methods[0]?.toUpperCase() ?? 'GET') as HttpRequest['method'];
  request.url = `{{baseUrl}}${pathToTemplate(e.path.startsWith('/') ? e.path : `/${e.path}`)}`;
  const notes: string[] = [];
  for (const p of e.params) {
    const binding = p.binding.toLowerCase();
    const label = `${p.type_text} · ${p.required ? 'required' : 'optional'}`;
    if (binding.includes('body')) {
      request.body = { kind: 'json', text: schema?.request ? skeletonText(schema.request) : '{\n  \n}', form: [] };
      notes.push(`Body: ${p.type_text}`);
      const required = requiredPaths(schema?.request);
      if (required.length) notes.push(`Required in the body: ${required.join(', ')}`);
    } else if (binding.includes('part')) {
      request.body = { kind: 'multipart', text: '', form: [{ key: p.name, value: '', description: label, enabled: p.required }] };
    } else if (binding.includes('param') || binding.includes('query')) {
      request.params.push({ key: p.name, value: sampleFor(p.type_text), description: label, enabled: p.required });
    } else if (binding.includes('header')) {
      request.headers.push({ key: p.name, value: sampleFor(p.type_text), description: label, enabled: p.required });
    } else if (binding.includes('cookie')) {
      request.headers.push({ key: 'Cookie', value: `${p.name}=${sampleFor(p.type_text)}`, description: label, enabled: p.required });
    } else if (binding.includes('path')) {
      // A path variable is a row of its own in Params (marked path, required, with its type and a sample), and the
      // URL names it as a variable that the row's value fills in.
      request.params.push({ key: `path:${p.name}`, value: sampleFor(p.type_text), description: `path · ${p.type_text} · required`, enabled: true });
      notes.push(`Path variable {{${p.name}}}: ${p.type_text}`);
    }
  }
  const accept = mediaTypes(e.produces);
  if (accept.length) request.headers.push({ key: 'Accept', value: accept.join(', '), description: 'what the handler produces', enabled: true });
  const consumes = mediaTypes(e.consumes);
  if (consumes.length && request.body.kind !== 'none') request.headers.push({ key: 'Content-Type', value: consumes[0], description: 'what the handler consumes', enabled: true });
  if (accept.length === 1 && accept[0] === 'text/event-stream') request.settings.stream = true;
  if (accept.includes('text/event-stream')) notes.push('This endpoint streams Server-Sent Events: listen to it with Ctrl+Shift+L instead of waiting for an answer.');
  request.docs = [
    `${e.methods[0]?.toUpperCase() ?? 'GET'} ${e.path}`,
    `${e.controller}#${e.handler}(${e.params.map((p) => `${p.type_text} ${p.name}`).join(', ')}) → ${e.return_type}`,
    ...(notes.length ? ['', ...notes.map((n) => `- ${n}`)] : []),
    '',
    e.file,
  ].join('\n');
  return request;
}

function createHirundoDetected() {
  let candidates = $state<BennuProject[]>([]);
  let groups = $state<ProjectEndpoints[]>([]);
  let loading = $state(false);
  let unavailable = $state('');

  /** The roots to list: the choice, else the Bennu project this window is on. */
  const roots = $derived.by(() => {
    const chosen = hirundoStore.config.detected_roots.filter((r) => candidates.some((c) => norm(c.root) === norm(r)));
    if (chosen.length) return chosen;
    const own = hirundoStore.project;
    const match = own ? candidates.find((c) => norm(c.root) === norm(own)) : null;
    return match ? [match.root] : [];
  });

  async function loadCandidates() {
    unavailable = '';
    try {
      await ensureBackend('bennu');
      const ws = await getBennuWorkspaces();
      candidates = ws.workspaces.flatMap((w) => w.projects.map((p) => ({ root: p.root, name: base(p.root), workspace: w.name || 'Workspace' })));
    } catch (e) {
      candidates = [];
      unavailable = `Bennu is not available: ${e}`;
    }
  }

  async function refresh() {
    loading = true;
    try {
      if (!candidates.length) await loadCandidates();
      const out: ProjectEndpoints[] = [];
      for (const root of roots) {
        out.push({ root, name: candidates.find((c) => norm(c.root) === norm(root))?.name ?? base(root), endpoints: await endpointsDetected(root).catch(() => []) });
      }
      groups = out;
    } catch (e) {
      toastStore.show(String(e), 'error');
    } finally {
      loading = false;
    }
  }

  async function choose(next: string[]) {
    await hirundoStore.setDetectedRoots(next);
    await refresh();
  }

  return {
    get candidates() { return candidates; },
    get roots() { return roots; },
    get groups() { return groups; },
    get loading() { return loading; },
    get unavailable() { return unavailable; },
    /** The Bennu project an endpoint was found in. */
    rootOf: (e: DetectedEndpoint) => groups.find((g) => g.endpoints.some((x) => x.id === e.id && x.file === e.file))?.root ?? null,
    refresh, choose, loadCandidates,
  };
}

export const hirundoDetected = createHirundoDetected();
