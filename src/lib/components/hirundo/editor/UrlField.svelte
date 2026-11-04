<script lang="ts">
  /**
   * UrlField — the URL bar: one line of the shared code editor, so it has real editing (selection, undo,
   * clipboard, Home/End) and the same highlighting machinery as code. Scheme, host, path, query keys and
   * values are coloured apart; a `{{variable}}` is coloured as a constant when the active environment, the
   * type, this machine or the collection above gives it a value, and underlined when nothing does.
   * Enter sends; the field never holds a line break.
   */
  import type { KeyBinding } from '@codemirror/view';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { languageForPath } from '$lib/components/bennu/languages';
  import { metaGet } from '$lib/ipc/hirundo';
  import { hirundoStore } from '$lib/stores/hirundo/workspace.svelte';
  import { urlMarks } from './url-marks';

  interface Props {
    value: string;
    oninput: (text: string) => void;
    onsend: () => void;
    /** Where the request lives, so the variables of its collection and folders count as known. */
    path: string | null;
  }

  let { value, oninput, onsend, path }: Props = $props();

  const plain = languageForPath('url.txt');

  // Variables the layers above the request define (collection, folders): read once per request.
  let inherited = $state<string[]>([]);
  $effect(() => {
    const project = hirundoStore.project;
    const p = path;
    inherited = [];
    if (!project || !p) return;
    const dirs = p.split('/').slice(0, -1);
    let cancelled = false;
    void Promise.all(dirs.map((_, i) => metaGet(project, dirs.slice(0, i + 1).join('/')).catch(() => null))).then((metas) => {
      if (!cancelled) inherited = metas.flatMap((m) => m?.vars.filter((v) => v.enabled && v.key).map((v) => v.key) ?? []);
    });
    return () => { cancelled = true; };
  });

  const known = $derived.by(() => {
    const names = new Set<string>(inherited);
    const envs = hirundoStore.envs;
    for (const v of [...(hirundoStore.activeEnvType?.vars ?? []), ...(hirundoStore.activeEnv?.vars ?? []), ...envs.local.vars]) if (v.enabled && v.key) names.add(v.key);
    return names;
  });

  const marks = $derived(urlMarks(value, known));
  const keys: KeyBinding[] = [{ key: 'Enter', run: () => { onsend(); return true; } }];
</script>

<div class="uf" role="group" aria-label="URL">
  <CodeEditor {value} language={plain} singleLine lineNumbers={false} folding={false} highlightActiveLine={false} problemMarkers={false} {marks} keyBindings={keys} placeholder={'{{baseUrl}}/path?query=1'} {oninput} />
</div>

<style>
  .uf {
    height: 30px; display: flex; align-items: center; min-width: 0;
    padding: 0 8px; border: 1px solid var(--border); border-radius: var(--radius-md);
    background: var(--bg-input, var(--bg-base)); font-family: var(--font-mono); font-size: var(--font-size-sm);
  }
  .uf:focus-within { border-color: var(--accent); }
  .uf > :global(*) { flex: 1; min-width: 0; }
  .uf :global(.cm-editor) { background: transparent; outline: none; }
  .uf :global(.cm-scroller) { overflow: hidden; }
</style>
