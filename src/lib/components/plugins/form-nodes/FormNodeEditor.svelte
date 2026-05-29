<!--
  FormNodeEditor — the `editor` host widget (CodeMirror 6).

  A value-bearing field: the document is `ctx.values[name]` (picked up by the
  whole-form submit like any other field, and pushable by the host via the
  `set_value` op — StudioTextPane is controlled, so an external write
  reconciles the buffer without an echo loop).

  On top of the whole-form model it is the first live consumer of the scoped
  per-node channel (§3.1/§3.4 of plugin-ui-dispatch-and-patch):
    · `on_edit`   — debounced, slot `edit`,   value = full document text
    · `on_select` — slot `select`, value = `{ from, to, text }`

  Both slots route through `ctx.handleScopedDispatch`, so they ship only
  `{ node_id, slot, value, state? }` (never the whole form) and can target a
  command. `scope_state` declares the liveState slice that rides along.
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import StudioTextPane from '$lib/components/shared/studio/StudioTextPane.svelte';
  import type { StudioLanguage } from '$lib/utils/studio-codemirror';
  import TypePill from '$lib/components/shared/internal/TypePill.svelte';
  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();

  const n = $derived(node as any);

  // Plugin language ids → the studio CM6 language set. Unknown → plain.
  const STUDIO_LANGS = new Set<StudioLanguage>([
    'ron', 'json', 'toml', 'yaml', 'properties', 'plain',
  ]);
  function toStudioLang(raw: unknown): StudioLanguage {
    const k = String(raw ?? '').toLowerCase();
    if (k === 'yml') return 'yaml';
    if (k === 'jsonc') return 'json';
    return STUDIO_LANGS.has(k as StudioLanguage) ? (k as StudioLanguage) : 'plain';
  }

  const language    = $derived(toStudioLang(n.language));
  const isReadOnly  = $derived(!!n.readonly || ctx.resolvedDisabled(n));
  const heightStyle = $derived(
    n.height == null ? '240px'
    : typeof n.height === 'number' ? `${n.height}px`
    : String(n.height),
  );

  // ── Edit slot — debounced scoped dispatch ───────────────────────────────
  let editTimer: ReturnType<typeof setTimeout> | undefined;

  function onInput(text: string) {
    ctx.values[n.name] = text;
    ctx.notifyChange(n.name, text);
    if (!n.on_edit) return;
    if (editTimer) clearTimeout(editTimer);
    const delay = typeof n.debounce_ms === 'number' ? n.debounce_ms : 300;
    editTimer = setTimeout(() => {
      ctx.handleScopedDispatch(n.id, 'edit', n.on_edit, text, { stateKeys: n.scope_state });
    }, delay);
  }

  function onSelect(sel: { from: number; to: number; text: string }) {
    if (!n.on_select) return;
    ctx.handleScopedDispatch(n.id, 'select', n.on_select, sel, { stateKeys: n.scope_state });
  }

  onDestroy(() => { if (editTimer) clearTimeout(editTimer); });
</script>

<div
  class="pf-field {n.class ?? ''}"
  class:pf-field-highlight={n.highlight}
  style={n.style}
>
  {#if n.label}
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="pf-label">
      {n.label}
      {#if n.required}<span class="pf-required" aria-hidden="true">*</span>{/if}
    </label>
  {/if}

  <div
    class="pf-editor"
    class:pf-editor-error={!!ctx.validationErrors[n.name]}
    style="height:{heightStyle}"
  >
    <StudioTextPane
      value={String(ctx.values[n.name] ?? '')}
      {language}
      readOnly={isReadOnly}
      showLineNumbers={n.line_numbers ?? true}
      showActiveLine={n.active_line ?? true}
      oninput={onInput}
      onselect={onSelect}
    />
  </div>

  {#if ctx.validationErrors[n.name]}
    <span class="pf-validation-error">{ctx.validationErrors[n.name]}</span>
  {/if}
  {#if n.hint}
    <span class="pf-hint">{n.hint}</span>
  {/if}
  {#if n.pill}
    <TypePill label={n.pill} kind={n.pill_kind ?? n.pill} tooltip={n.pill_tooltip} />
  {/if}
</div>

<style>
  /* CodeMirror host: a framed box that lets StudioTextPane (flex:1) fill it.
     Matches the rounded/bordered look of the other `.pf-*` controls. */
  .pf-editor {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md, 6px);
    overflow: hidden;
    background: var(--bg-base);
  }
  .pf-editor:focus-within {
    border-color: var(--accent);
  }
  .pf-editor-error {
    border-color: var(--danger, #e5534b);
  }
</style>
