<!--
  FormNodeReorderList — a list whose ORDER is the answer.

  The node a plugin emits when the question is "in what sequence", not "which one": chunks to
  concatenate, steps to run, files to merge. Its value in the form is the array of ids in the
  order shown, so a submit carries the sequence without the plugin tracking moves itself.

  ## Keyboard first, and not as a fallback

  A reordering control that can only be dragged is unusable for the people Arbor is for, and
  drag is also the half that WebView2 gets wrong (native HTML5 drag-and-drop does not work
  there — see the Tabs strip, which drives its drag from mouse events instead). So the arrows
  are the control: ↑/↓ move the selection, Alt+↑/↓ move the ROW, Home/End jump. The buttons
  are the same three verbs for the mouse.

  ## What a plugin gets to say

  `items` is a list of `{ id, label, sublabel?, icon?, meta? }`. `id` is what comes back;
  everything else is what the person reads. A row without an id falls back to its index, so a
  plugin that emitted plain labels still gets a usable answer rather than an empty one.
-->
<script lang="ts">
  import { ArrowDown, ArrowUp } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import PluginIcon from '../PluginIcon.svelte';
  import type { FormNode } from '$lib/types/plugin';
  import type { FormNodeCtx } from './ctx';
  import { toArr } from './helpers';

  interface Props {
    node: FormNode;
    ctx:  FormNodeCtx;
  }
  let { node, ctx }: Props = $props();

  interface RawRow {
    id?:       string;
    label?:    string;
    sublabel?: string;
    icon?:     string;
    meta?:     string;
  }
  interface Row {
    id:        string;
    label:     string;
    sublabel?: string;
    icon?:     string;
    meta?:     string;
  }

  const rl       = $derived(node as any);
  const name     = $derived<string>(rl.name ?? rl.id ?? 'order');
  const readonly = $derived<boolean>(!!rl.readonly || ctx.disabled);

  // `toArr`, never `?? []`: an empty Lua table crosses as `{}`, and the object form would
  // otherwise sail past the guard as a truthy non-array (CLAUDE.md · form nodes).
  const rows = $derived<Row[]>(
    toArr<RawRow>(rl.items).map((raw, i) => ({
      id:       typeof raw?.id === 'string' && raw.id ? raw.id : String(i),
      label:    typeof raw?.label === 'string' ? raw.label : String(raw?.id ?? i),
      sublabel: typeof raw?.sublabel === 'string' ? raw.sublabel : undefined,
      icon:     typeof raw?.icon === 'string' ? raw.icon : undefined,
      meta:     typeof raw?.meta === 'string' ? raw.meta : undefined,
    })),
  );

  // ── The order, live ───────────────────────────────────────────────────────
  //
  // Held here rather than read back out of `ctx.values` on every render: the value IS this
  // list, and a node that re-derived it from the form would snap back to the plugin's order
  // the moment any other node patched. Re-seeded when the plugin genuinely replaces `items`
  // — which is what re-running a listing does, and what should reset the sequence.
  let order = $state<string[]>([]);
  let selected = $state<string | null>(null);

  $effect(() => {
    const ids = rows.map((r) => r.id);
    // Reading `rows` subscribes this to the plugin's own updates and to nothing else.
    order = ids;
    selected = ids[0] ?? null;
    ctx.values[name] = ids;
  });

  const ordered = $derived<Row[]>(
    order.map((id) => rows.find((r) => r.id === id)).filter((r): r is Row => !!r),
  );
  const index = $derived<number>(selected ? order.indexOf(selected) : -1);

  function commit(next: string[]) {
    order = next;
    ctx.values[name] = next;
    // The plugin hears about a move only if it asked to: a reorder list is usually read at
    // submit, and a chatty one would fire a round-trip per keystroke of Alt+Down.
    ctx.notifyChange(name, next);
  }

  /** Move the selected row by `delta`, clamped. Returns whether anything moved. */
  function move(delta: number): boolean {
    if (readonly || index < 0) return false;
    const to = index + delta;
    if (to < 0 || to >= order.length) return false;
    const next = [...order];
    const [row] = next.splice(index, 1);
    next.splice(to, 0, row);
    commit(next);
    return true;
  }

  function select(delta: number) {
    if (order.length === 0) return;
    const from = index < 0 ? 0 : index;
    const to = Math.min(order.length - 1, Math.max(0, from + delta));
    selected = order[to];
  }

  function onKeyDown(e: KeyboardEvent) {
    // Alt is the modifier that means "take the row with you" in every list that does this —
    // IntelliJ, VS Code, the pipeline editor next door.
    const step = e.key === 'ArrowUp' ? -1 : e.key === 'ArrowDown' ? 1 : 0;
    if (step !== 0) {
      e.preventDefault();
      if (e.altKey) move(step);
      else select(step);
      return;
    }
    if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      const to = e.key === 'Home' ? 0 : order.length - 1;
      if (e.altKey && index >= 0) move(to - index);
      else selected = order[to] ?? null;
    }
  }
</script>

<div class="reorder">
  {#if rl.label}<div class="rl-label">{rl.label}</div>{/if}

  <ul
    class="rl-list"
    role="listbox"
    tabindex="0"
    aria-label={rl.label ?? 'Order'}
    aria-activedescendant={selected ? `${name}-${selected}` : undefined}
    onkeydown={onKeyDown}
  >
    {#each ordered as row, i (row.id)}
      <li
        id="{name}-{row.id}"
        class="rl-row"
        class:selected={row.id === selected}
        role="option"
        aria-selected={row.id === selected}
      >
        <span class="rl-pos">{i + 1}</span>
        {#if row.icon}<PluginIcon name={row.icon} size={14} />{/if}
        <button type="button" class="rl-pick" onclick={() => (selected = row.id)}>
          <span class="rl-name">{row.label}</span>
          {#if row.sublabel}<span class="rl-sub">{row.sublabel}</span>{/if}
        </button>
        {#if row.meta}<span class="rl-meta">{row.meta}</span>{/if}
      </li>
    {/each}
    {#if ordered.length === 0}
      <li class="rl-empty">Nothing to order.</li>
    {/if}
  </ul>

  <div class="rl-actions">
    <Button
      variant="ghost"
      size="sm"
      icon={ArrowUp}
      disabled={readonly || index <= 0}
      onclick={() => move(-1)}
    >Up</Button>
    <Button
      variant="ghost"
      size="sm"
      icon={ArrowDown}
      disabled={readonly || index < 0 || index >= order.length - 1}
      onclick={() => move(1)}
    >Down</Button>
    <span class="rl-hint">Alt+↑ / Alt+↓ to move the row</span>
  </div>
</div>

<style>
  .reorder { display: flex; flex-direction: column; gap: 6px; }

  .rl-label { font-size: 12px; color: var(--text-secondary); }

  .rl-list {
    display: flex; flex-direction: column;
    margin: 0; padding: 4px;
    list-style: none;
    max-height: 320px; overflow-y: auto;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
  }
  .rl-list:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }

  .rl-row {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
    color: var(--text-primary);
  }
  .rl-row.selected { background: var(--bg-selected); }

  .rl-pos {
    min-width: 18px;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--text-tertiary);
  }

  .rl-pick {
    flex: 1; min-width: 0;
    display: flex; align-items: baseline; gap: 8px;
    background: none; border: none; padding: 0;
    color: inherit; font: inherit; text-align: left; cursor: pointer;
  }
  .rl-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rl-sub, .rl-meta { font-size: 11px; color: var(--text-tertiary); }
  .rl-meta { font-variant-numeric: tabular-nums; }

  .rl-empty { padding: 8px; font-size: 12px; color: var(--text-tertiary); }

  .rl-actions { display: flex; align-items: center; gap: 6px; }
  .rl-hint { font-size: 11px; color: var(--text-tertiary); }
</style>
