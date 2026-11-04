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
import { getBennuWorkspaces } from '$lib/ipc/bennu/config';
import { ensureBackend } from '$lib/ipc/rpc';
import type { DetectedEndpoint, HttpRequest } from '$lib/types/hirundo';
import { blankRequest } from '$lib/components/hirundo/request-model';
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

/** A request for a detected endpoint: method, URL on `{{baseUrl}}`, its query parameters, a JSON body if it takes one. */
export function endpointToRequest(e: DetectedEndpoint): HttpRequest {
  const request = blankRequest(`${e.controller}.${e.handler}`);
  request.method = (e.methods[0]?.toUpperCase() ?? 'GET') as HttpRequest['method'];
  request.url = `{{baseUrl}}${pathToTemplate(e.path.startsWith('/') ? e.path : `/${e.path}`)}`;
  for (const p of e.params) {
    const binding = p.binding.toLowerCase();
    if (binding.includes('body')) {
      request.body = { kind: 'json', text: '{\n  \n}', form: [] };
    } else if (binding.includes('param') || binding.includes('query')) {
      request.params.push({ key: p.name, value: '', description: `${p.type_text}${p.required ? '' : ' (optional)'}`, enabled: p.required });
    } else if (binding.includes('header')) {
      request.headers.push({ key: p.name, value: '', description: p.type_text, enabled: p.required });
    }
  }
  if (e.produces) request.headers.push({ key: 'Accept', value: e.produces, description: '', enabled: true });
  request.docs = `${e.controller}#${e.handler} → ${e.return_type}\n\n${e.file}`;
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
    refresh, choose, loadCandidates,
  };
}

export const hirundoDetected = createHirundoDetected();
