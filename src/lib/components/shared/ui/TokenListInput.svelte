<script module lang="ts">
  /** One row's verdict, drawn beside it — what this entry turned out to mean. */
  export interface TokenStatus {
    /** Short text, right-aligned: `3 artifacts`, `matches nothing`. */
    label: string;
    tone: 'success' | 'warning' | 'muted';
    tooltip?: string;
  }

  /** One offer in the add field's list. */
  export interface TokenSuggestion {
    value: string;
    /** A second line's worth of context — where it was seen, what it is. */
    detail?: string;
  }
</script>

<script lang="ts">
  /**
   * TokenListInput — a **list** of short string entries, added one at a time and removed one at a
   * time, each able to say what it turned out to mean.
   *
   * ## Why this is not a text field with commas in it
   *
   * A comma-separated field is a list pretending to be a string, and every property of a list is
   * lost in the pretence: you cannot remove one entry without re-reading the whole line, you cannot
   * be told that the third one matches nothing, and you cannot be offered the values that would
   * work. It also commits on blur, so nothing at all appears to happen while you type — which is
   * the question this widget exists to stop people having to ask.
   *
   * ## The three parts, and each is load-bearing
   *
   * * **the rows** — one entry each, removable, so a list is edited as a list;
   * * **the status** — what the host found out about that entry, right-aligned. An entry that
   *   matches nothing is the commonest mistake in any allowlist (a typo, a coordinate that moved),
   *   and it is invisible in a text field until somebody wonders why a panel is empty;
   * * **the suggestions** — the add field offers the values that actually exist, so the usual case
   *   is picking rather than typing, and a typo is something you have to go out of your way to make.
   *
   * Generic on purpose: it knows nothing about what the strings are. The host supplies the
   * suggestions, decides each entry's status, and may normalise what was typed.
   *
   * Keyboard-first: ↑/↓ walk the suggestions, Enter takes the highlighted one (or what was typed
   * when none is), Esc closes the list, Backspace on an empty field removes the last entry.
   */
  import { X, Plus } from 'lucide-svelte';
  import IconButton from './IconButton.svelte';

  interface Props {
    /** The entries, in order. */
    values: string[];
    /** Called with the whole new list — the host owns it. */
    onchange: (next: string[]) => void;
    /** What the add field says when empty. */
    placeholder?: string;
    /** Everything that could be added. Filtered against what is typed, minus what is already in. */
    suggestions?: TokenSuggestion[];
    /** What the host found out about one entry, or `null` to say nothing about it. */
    status?: (value: string) => TokenStatus | null;
    /** Clean up (or refuse, with `null`) what was typed before it becomes an entry. */
    normalise?: (raw: string) => string | null;
    /** What to say when the list is empty — this is the state that needs explaining. */
    emptyMessage?: string;
    ariaLabel: string;
  }

  let {
    values,
    onchange,
    placeholder = 'Add…',
    suggestions = [],
    status,
    normalise,
    emptyMessage = 'Nothing yet.',
    ariaLabel,
  }: Props = $props();

  let draft = $state('');
  let open = $state(false);
  let active = $state(0);
  let fieldEl = $state<HTMLInputElement | undefined>();

  /** What the add field offers: everything not already in the list, matching what is typed. */
  const offered = $derived.by(() => {
    const typed = draft.trim().toLowerCase();
    const already = new Set(values);
    return suggestions
      .filter((s) => !already.has(s.value))
      .filter((s) => !typed || s.value.toLowerCase().includes(typed))
      .slice(0, 12);
  });

  /** Whether the list is worth drawing at all: it opens on focus, and an empty one is a box with
   *  nothing in it rather than a hint. */
  const showList = $derived(open && offered.length > 0);

  function add(raw: string) {
    const cleaned = normalise ? normalise(raw) : raw.trim();
    if (!cleaned || values.includes(cleaned)) {
      draft = '';
      return;
    }
    onchange([...values, cleaned]);
    draft = '';
    active = 0;
  }

  function remove(value: string) {
    onchange(values.filter((v) => v !== value));
    // Focus goes back to the field: removing three entries in a row should not cost three clicks
    // into the same place.
    queueMicrotask(() => fieldEl?.focus());
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && showList) {
      e.preventDefault();
      active = (active + 1) % offered.length;
    } else if (e.key === 'ArrowUp' && showList) {
      e.preventDefault();
      active = (active - 1 + offered.length) % offered.length;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      // The highlighted offer when the list is open and something is highlighted; otherwise what
      // was typed — an allowlist entry is often a prefix that matches nothing yet, and refusing to
      // accept it would make the field usable only for what already exists.
      add(showList && offered[active] ? offered[active].value : draft);
    } else if (e.key === 'Escape' && showList) {
      e.preventDefault();
      e.stopPropagation();
      open = false;
    } else if (e.key === 'Backspace' && draft === '' && values.length > 0) {
      onchange(values.slice(0, -1));
    } else {
      active = 0;
    }
  }
</script>

<div class="tl">
  {#if values.length === 0}
    <p class="tl-empty">{emptyMessage}</p>
  {:else}
    <ul class="tl-rows">
      {#each values as value (value)}
        {@const st = status?.(value) ?? null}
        <li class="tl-row">
          <code class="tl-value">{value}</code>
          {#if st}
            <span class="tl-status tl-{st.tone}" title={st.tooltip ?? ''}>{st.label}</span>
          {/if}
          <IconButton tooltip="Remove" size={20} onclick={() => remove(value)}>
            <X size={11} />
          </IconButton>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="tl-add">
    <Plus size={12} class="tl-plus" />
    <input
      class="tl-field"
      bind:this={fieldEl}
      bind:value={draft}
      {placeholder}
      aria-label={ariaLabel}
      role="combobox"
      aria-expanded={showList}
      aria-controls="tl-list-{ariaLabel.replace(/\W+/g, '-')}"
      aria-autocomplete="list"
      autocomplete="off"
      spellcheck="false"
      onfocus={() => (open = true)}
      onblur={() => setTimeout(() => (open = false), 120)}
      onkeydown={onKey}
    />
    {#if draft.trim()}
      <IconButton tooltip="Add" size={20} onclick={() => add(draft)}>
        <Plus size={11} />
      </IconButton>
    {/if}
  </div>

  {#if showList}
    <ul class="tl-list" id="tl-list-{ariaLabel.replace(/\W+/g, '-')}" role="listbox">
      {#each offered as s, i (s.value)}
        <li role="option" aria-selected={i === active}>
          <button
            type="button"
            class="tl-offer"
            class:active={i === active}
            onmousedown={(e) => { e.preventDefault(); add(s.value); }}
            onmouseenter={() => (active = i)}
          >
            <code>{s.value}</code>
            {#if s.detail}<span class="tl-detail">{s.detail}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .tl { display: flex; flex-direction: column; gap: 4px; position: relative; }
  .tl-empty { margin: 0; font-size: 11px; color: var(--text-disabled); font-style: italic; }
  .tl-rows { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
  .tl-row {
    display: flex; align-items: center; gap: 8px;
    padding: 2px 2px 2px 8px;
    background: var(--bg-subtle);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .tl-value { flex: 1; font-family: var(--font-code); font-size: 11px; color: var(--text-primary); }
  .tl-status { font-size: 10px; white-space: nowrap; }
  .tl-success { color: var(--success); }
  .tl-warning { color: var(--warning); }
  .tl-muted { color: var(--text-disabled); }

  .tl-add {
    display: flex; align-items: center; gap: 6px;
    padding: 2px 2px 2px 8px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .tl-add :global(.tl-plus) { color: var(--text-disabled); flex: none; }
  .tl-field {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    padding: 4px 0;
    color: var(--text-primary);
    font-family: var(--font-code); font-size: 11px;
  }
  .tl-field::placeholder { color: var(--text-disabled); }
  .tl-add:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }

  .tl-list {
    list-style: none; margin: 0; padding: 3px;
    position: absolute; left: 0; right: 0; top: 100%; z-index: 20;
    max-height: 220px; overflow-y: auto;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
  }
  .tl-offer {
    display: flex; align-items: baseline; gap: 8px; width: 100%;
    padding: 3px 6px; border: none; background: none; text-align: left; cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .tl-offer code { font-family: var(--font-code); font-size: 11px; color: var(--text-primary); }
  .tl-offer.active { background: var(--accent-subtle); }
  .tl-detail { font-size: 10px; color: var(--text-muted); margin-left: auto; }
</style>
