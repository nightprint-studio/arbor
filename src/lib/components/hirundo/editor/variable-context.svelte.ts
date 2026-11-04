/**
 * The variables a request can see, with where each comes from. One reading of the cascade for every place that
 * needs it — the URL's `{{` completion, the header fields, the "undefined variable" banner — so they cannot
 * disagree. Order of precedence, strongest first: this machine → the environment → its type → the folders
 * (innermost first) → the collection.
 *
 * Call it from a component's init: it keeps its own `$effect` for the settings of the collections above the
 * request, which are read from the backend once per request path.
 */
import { metaGet } from '$lib/ipc/hirundo';
import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
import type { KeyValue } from '$lib/types/hirundo';
import { collectVariables, type VarInfo, type VarScope } from './completions';

export function createVariableContext(getPath: () => string | null) {
  let layers = $state<{ scope: VarScope; vars: KeyValue[] }[]>([]);

  $effect(() => {
    const project = hirundoStore.project;
    const path = getPath();
    layers = [];
    if (!project || !path) return;
    const dirs = path.split('/').slice(0, -1);
    let cancelled = false;
    void Promise.all(dirs.map((_, i) => metaGet(project, dirs.slice(0, i + 1).join('/')).catch(() => null))).then((metas) => {
      if (cancelled) return;
      // Innermost folder first, the collection (depth 1) last.
      layers = metas
        .map((m, i) => ({ scope: (i === 0 ? 'collection' : 'folder') as VarScope, vars: m?.vars ?? [] }))
        .reverse();
    });
    return () => { cancelled = true; };
  });

  const vars = $derived<VarInfo[]>(
    collectVariables([
      { scope: 'local', vars: hirundoStore.envs.local.vars },
      { scope: 'environment', vars: hirundoStore.activeEnv?.vars ?? [] },
      { scope: 'type', vars: hirundoStore.activeEnvType?.vars ?? [] },
      ...layers,
    ]),
  );
  const known = $derived(new Set(vars.map((v) => v.name)));

  return {
    get vars() { return vars; },
    get known() { return known; },
    find: (name: string) => vars.find((v) => v.name === name) ?? null,
  };
}
