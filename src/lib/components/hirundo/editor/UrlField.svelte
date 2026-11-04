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
  import type { CompletionContext, CompletionResult } from '@codemirror/autocomplete';
  import { hirundoDetected } from '$lib/stores/hirundo/detected.svelte';
  import { pathToTemplate } from '$lib/stores/hirundo/detected.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { hirundoUiStore } from '$lib/stores/hirundo/ui.svelte';
  import { undefinedVariables, variableSuggest } from './completions';
  import { createVariableContext } from './variable-context.svelte';
  import { urlMarks } from './url-marks';

  interface Props {
    value: string;
    oninput: (text: string) => void;
    onsend: () => void;
    /** Where the request lives, so the variables of its collection and folders count as known. */
    path: string | null;
    /** Names given a value elsewhere in the request (its path rows), so they are not shown as undefined. */
    extraKnown?: readonly string[];
  }

  let { value, oninput, onsend, path, extraKnown = [] }: Props = $props();

  const ctx = createVariableContext(() => path);

  /** `{{` → the variables that exist (with where they come from); a typed `/…` → the paths Bennu found in the code. */
  function complete(c: CompletionContext): CompletionResult | null {
    const text = c.state.doc.toString();
    const v = variableSuggest(text, c.pos, ctx.vars);
    if (v) {
      return {
        from: v.from, to: v.to, filter: false,
        options: v.items.map((i) => ({ label: i.label, detail: i.meta, info: i.description, apply: i.id, type: 'variable' })),
      };
    }
    const typed = /(?:^|\}\})(\/[\w\-/{}.]*)$/.exec(text.slice(0, c.pos));
    if (!typed) return null;
    const paths = [...new Set(hirundoDetected.groups.flatMap((g) => g.endpoints.map((e) => pathToTemplate(e.path.startsWith('/') ? e.path : `/${e.path}`))))];
    const options = paths.filter((p) => p.startsWith(typed[1]) && p !== typed[1]).map((p) => ({ label: p, detail: 'from the code', apply: p, type: 'text' }));
    return options.length ? { from: c.pos - typed[1].length, options, filter: false } : null;
  }

  const base = languageForPath('url.txt');
  const plain = { ...base, intel: { ...(base.intel ?? {}), completion: complete } };

  const known = $derived(new Set([...ctx.known, ...extraKnown]));
  const marks = $derived(urlMarks(value, known));
  /** Alt+Enter on an undefined `{{variable}}`: the quick action that opens "Create variable" for it. */
  const keys: KeyBinding[] = [
    { key: 'Enter', run: () => { onsend(); return true; } },
    {
      key: 'Alt-Enter',
      run: (view) => {
        const at = view.state.selection.main.head;
        for (const m of view.state.doc.toString().matchAll(/\{\{\s*([\w.-]+)\s*\}\}/g)) {
          const start = m.index ?? 0;
          if (at >= start && at <= start + m[0].length && !known.has(m[1])) {
            hirundoUiStore.open({ kind: 'create-variable', name: m[1], path });
            return true;
          }
        }
        return false;
      },
    },
  ];
  const undefinedHere = $derived(undefinedVariables([value], known));
  const tip = $derived(undefinedHere.length ? `Undefined: ${undefinedHere.join(', ')} — Alt+Enter on it, or the bar below, to create it` : '');
</script>

<div class="uf" role="group" aria-label="URL" use:tooltip={tip}>
  <CodeEditor {value} language={plain} singleLine lineNumbers={false} folding={false} highlightActiveLine={false} problemMarkers={false} {marks} keyBindings={keys} placeholder={'{{baseUrl}}/path?query=1'} {oninput} />
</div>

<style>
  /* A field like every other input: its own background, a full border, the same height as the method and Send. */
  .uf {
    box-sizing: border-box; height: 32px; display: flex; align-items: stretch; min-width: 0; overflow: hidden;
    padding: 0 10px; border: 1px solid var(--border); border-radius: var(--radius-md);
    background: var(--bg-input); font-family: var(--font-mono); font-size: var(--font-size-sm);
    transition: border-color var(--transition-fast);
  }
  .uf:hover { border-color: var(--border-strong, var(--border)); }
  .uf:focus-within { border-color: var(--border-focus); box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 25%, transparent); }
  .uf > :global(*) { flex: 1; min-width: 0; height: 100%; }
  .uf :global(.cm-editor) { background: transparent; outline: none; height: 100%; }
  .uf :global(.cm-scroller) { overflow: hidden; align-items: center; }
  .uf :global(.cm-content) { padding: 0; }
</style>
