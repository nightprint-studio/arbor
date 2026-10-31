<script lang="ts">
  /**
   * An editable list of short texts — notes, rules, phrases — where each item is its own field.
   *
   * The keyboard is the interface: Enter adds the next item below, Backspace on an empty item removes it and lands
   * on the one above, Alt+↑ / Alt+↓ move an item, Shift+Enter is a newline inside one, Ctrl/Cmd+Enter submits.
   * `suggestions` are one-click starting points, offered until they are in the list.
   *
   * The component owns the order and the focus; `items` is bound out for the caller. To load a new list, remount it
   * (`{#key}`): ids are handed out per row so reordering keeps each field's caret and scroll with its text.
   */
  import { ChevronUp, ChevronDown, Trash2, Plus } from 'lucide-svelte';
  import Textarea from './Textarea.svelte';
  import IconButton from './IconButton.svelte';
  import Button from './Button.svelte';

  interface Row { id: number; text: string; el?: HTMLTextAreaElement }

  let {
    items = $bindable<string[]>([]),
    placeholder = 'Write one…',
    addLabel = 'Add',
    suggestions = [],
    maxItems = 50,
    ariaLabel,
    onsubmit,
  }: {
    items?: string[];
    /** Shown in an item that is still empty. */
    placeholder?: string;
    addLabel?: string;
    /** Ready-made items, shown as buttons while they are not in the list. */
    suggestions?: string[];
    maxItems?: number;
    ariaLabel: string;
    onsubmit?: () => void;
  } = $props();

  let nextId = 0;
  // Read once on purpose: the list is the component's own after mounting.
  // svelte-ignore state_referenced_locally
  let rows = $state<Row[]>(items.map((text) => ({ id: nextId++, text })));

  $effect(() => {
    items = rows.map((r) => r.text);
  });

  const unused = $derived(suggestions.filter((s) => !rows.some((r) => r.text.trim() === s)));
  const full = $derived(rows.length >= maxItems);

  function focusRow(id: number, atEnd = true) {
    queueMicrotask(() => {
      const el = rows.find((r) => r.id === id)?.el;
      if (!el) return;
      el.focus();
      if (atEnd) el.setSelectionRange(el.value.length, el.value.length);
    });
  }

  function add(text = '', after = rows.length - 1) {
    if (full) return;
    const row: Row = { id: nextId++, text };
    rows.splice(after + 1, 0, row);
    focusRow(row.id);
  }

  function remove(index: number, refocus = true) {
    rows.splice(index, 1);
    const near = rows[Math.max(0, index - 1)];
    if (refocus && near) focusRow(near.id);
  }

  function move(index: number, by: -1 | 1) {
    const to = index + by;
    if (to < 0 || to >= rows.length) return;
    const [row] = rows.splice(index, 1);
    rows.splice(to, 0, row);
    focusRow(row.id, false);
  }

  function keys(e: KeyboardEvent, index: number) {
    if (e.key === 'Enter' && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      // An empty last item already is the "next" one.
      if (!(rows[index].text === '' && index === rows.length - 1)) add('', index);
    } else if (e.key === 'Backspace' && rows[index].text === '') {
      e.preventDefault();
      remove(index);
    } else if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      e.preventDefault();
      move(index, e.key === 'ArrowUp' ? -1 : 1);
    }
  }
</script>

<div class="tle" role="group" aria-label={ariaLabel}>
  {#if rows.length === 0}
    <p class="empty">Nothing yet.</p>
  {/if}

  <ol class="rows">
    {#each rows as row, i (row.id)}
      <li class="row">
        <span class="mark" aria-hidden="true">{i + 1}</span>
        <div class="field">
          <Textarea
            framed
            bind:value={row.text}
            bind:element={row.el}
            minRows={1}
            maxRows={6}
            {placeholder}
            ariaLabel={`${ariaLabel}, item ${i + 1}`}
            onkeydown={(e) => keys(e, i)}
            {onsubmit}
          />
        </div>
        <div class="tools">
          <IconButton tooltip="Move up" shortcut="Alt+↑" size={22} disabled={i === 0} onclick={() => move(i, -1)}><ChevronUp size={13} /></IconButton>
          <IconButton tooltip="Move down" shortcut="Alt+↓" size={22} disabled={i === rows.length - 1} onclick={() => move(i, 1)}><ChevronDown size={13} /></IconButton>
          <IconButton tooltip="Remove" variant="danger" size={22} onclick={() => remove(i, false)}><Trash2 size={13} /></IconButton>
        </div>
      </li>
    {/each}
  </ol>

  <div class="foot">
    <Button size="xs" variant="outline" disabled={full} onclick={() => add()}>
      {#snippet iconStart()}<Plus size={12} />{/snippet}
      {addLabel}
    </Button>
    {#each unused as s (s)}
      <Button size="xs" variant="ghost" disabled={full} onclick={() => add(s)}>{s}</Button>
    {/each}
  </div>
</div>

<style>
  .tle { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .empty { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .row { display: flex; align-items: flex-start; gap: 8px; min-width: 0; }
  .mark {
    flex: none;
    width: 18px;
    padding-top: 7px;
    text-align: right;
    font-size: var(--font-size-2xs);
    font-variant-numeric: tabular-nums;
    color: var(--text-disabled);
  }
  .field { flex: 1; min-width: 0; }
  /* Visible on hover and while the row is being edited, so a list of ten is not ten toolbars. */
  .tools { flex: none; display: flex; gap: 1px; padding-top: 4px; opacity: 0; transition: opacity var(--anim-dur-base); }
  .row:hover .tools, .row:focus-within .tools { opacity: 1; }
  .foot { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 6px; }
</style>
