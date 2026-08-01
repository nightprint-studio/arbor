<script lang="ts">
  /**
   * The query box: structured filters as chips, free text as text, in one field.
   *
   * This is the shape the design asks for (`docs/garrulus-design.md` §12.14,
   * §8.5 "Ricerca strutturata") and the mockup draws: `type:bug` is not a
   * checkbox in a panel beside the input, it is a token *inside* it, because the
   * query is one sentence and splitting it across widgets makes the user assemble
   * it twice. Typing a finished `key:value` turns it into a chip; Backspace on an
   * empty input takes the last chip back; Enter searches.
   *
   * The chips are a rendering of the string, never a parse of it — the whole box
   * is reassembled and sent verbatim, so the backend's grammar stays the only
   * grammar (see `query-tokens.ts`).
   */
  import { Search, X } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { garrulusSearchStore } from '$lib/stores/garrulus/search.svelte';
  import { tokenPrefix } from './query-tokens';

  interface Props {
    /** Focus the input on mount — the search view is opened to be typed into. */
    autofocus?: boolean;
    /** The Down arrow left the input: hand the keyboard to the result list. */
    onLeaveDown?: () => void;
  }

  let { autofocus = false, onLeaveDown }: Props = $props();

  let inputEl = $state<HTMLInputElement | null>(null);

  $effect(() => {
    if (autofocus) inputEl?.focus();
  });

  /** Put the caret back in the box — after a chip is removed there is nowhere
   *  else sensible for it to be. */
  export function focus(): void {
    inputEl?.focus();
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void garrulusSearchStore.run();
      return;
    }
    if (e.key === 'Backspace' && garrulusSearchStore.text.length === 0) {
      // Only when there is a chip to take back, so Backspace in an empty box is
      // still an ordinary no-op rather than a key that silently does something.
      if (garrulusSearchStore.tokens.length === 0) return;
      e.preventDefault();
      garrulusSearchStore.dropLastToken();
      return;
    }
    if (e.key === 'ArrowDown' && onLeaveDown) {
      e.preventDefault();
      onLeaveDown();
    }
  }

  function removeAt(index: number) {
    garrulusSearchStore.removeToken(index);
    inputEl?.focus();
  }
</script>

<div class="qbox">
  <span class="qbox-icon"><Search size={13} /></span>

  {#each garrulusSearchStore.tokens as token, i (`${token.kind}:${token.key}:${token.op}:${token.value}`)}
    <span class="qtok" data-kind={token.kind}>
      <span class="qtok-key">{tokenPrefix(token)}</span><span class="qtok-val">{token.value}</span>
      <button
        type="button"
        class="qtok-x"
        aria-label="Remove the {tokenPrefix(token)}{token.value} filter"
        use:tooltip={'Remove this filter'}
        onclick={() => removeAt(i)}
      >
        <X size={10} />
      </button>
    </span>
  {/each}

  <input
    bind:this={inputEl}
    class="qbox-input"
    type="text"
    spellcheck="false"
    autocomplete="off"
    aria-label="Search the vault"
    placeholder={garrulusSearchStore.tokens.length > 0
      ? 'and these words…'
      : 'words to find — or field: to filter, Enter to search'}
    value={garrulusSearchStore.text}
    oninput={(e) => garrulusSearchStore.setText(e.currentTarget.value)}
    onkeydown={onKeyDown}
  />

  {#if garrulusSearchStore.query}
    <button
      type="button"
      class="qbox-clear"
      aria-label="Clear the query"
      use:tooltip={'Clear the query and its results'}
      onclick={() => {
        garrulusSearchStore.clear();
        inputEl?.focus();
      }}
    >
      <X size={12} />
    </button>
  {/if}
</div>

<style>
  .qbox {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 5px;
    min-height: 32px;
    padding: 4px 6px 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-input);
  }
  .qbox:focus-within { border-color: var(--border-focus); }

  .qbox-icon {
    display: flex;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  /* A chip is monospace because it is syntax: `stato:in corso` has to be read
     character by character to see where the key ends. */
  .qtok {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    height: 20px;
    padding: 0 3px 0 6px;
    border-radius: var(--radius-sm);
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    max-width: 100%;
  }
  /* A `sort:` term is not a filter — it reorders, it excludes nothing — so it
     does not wear the colour that means "this is narrowing your results". */
  .qtok[data-kind='sort'] {
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border-color: var(--border-subtle);
  }
  .qtok-key { opacity: 0.72; }
  .qtok-val {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .qtok-x {
    display: flex;
    align-items: center;
    padding: 2px;
    border: none;
    background: none;
    color: inherit;
    opacity: 0.6;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }
  .qtok-x:hover { opacity: 1; background: color-mix(in srgb, var(--accent) 18%, transparent); }

  .qbox-input {
    flex: 1;
    min-width: 140px;
    height: 22px;
    border: none;
    outline: none;
    background: none;
    color: var(--text-primary);
    font-size: var(--font-size-sm);
    font-family: inherit;
  }
  .qbox-input::placeholder { color: var(--text-disabled); }

  .qbox-clear {
    display: flex;
    align-items: center;
    padding: 3px;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }
  .qbox-clear:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
