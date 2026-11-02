<script lang="ts">
  /**
   * One box for everything the page can start: a project, a product, an action. Type to narrow, ↑ ↓ to choose, Enter to
   * go, Esc to leave. `/` on the page focuses it (the page owns that key and calls `focus()`).
   */
  import { Search } from 'lucide-svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import { searchEntries, type SearchEntry } from './welcome-logic';

  let { entries, productOf, onpick }: {
    entries: SearchEntry[];
    /** The product whose icon an entry wears: a project's product, or the product itself; none for an action. */
    productOf: (e: SearchEntry) => string | null;
    onpick: (e: SearchEntry) => void;
  } = $props();

  let query = $state('');
  let open = $state(false);
  let sel = $state(0);
  let input = $state<HTMLInputElement>();

  const groups = $derived(searchEntries(query, entries));
  const flat = $derived(groups.flatMap((g) => g.items));
  $effect(() => { void query; sel = 0; });

  export function focus() { input?.focus(); }

  function choose(e: SearchEntry | undefined) {
    if (!e) return;
    query = ''; open = false; input?.blur();
    onpick(e);
  }

  function onKey(ev: KeyboardEvent) {
    if (ev.key === 'ArrowDown') { ev.preventDefault(); sel = (sel + 1) % Math.max(1, flat.length); }
    else if (ev.key === 'ArrowUp') { ev.preventDefault(); sel = (sel - 1 + flat.length) % Math.max(1, flat.length); }
    else if (ev.key === 'Enter') { ev.preventDefault(); choose(flat[sel]); }
    else if (ev.key === 'Escape') { query = ''; open = false; input?.blur(); }
  }

  $effect(() => { if (open) document.getElementById(`ws-opt-${sel}`)?.scrollIntoView({ block: 'nearest' }); });
</script>

<div class="omni">
  <label class="box">
    <Search size={18} />
    <input
      bind:this={input}
      bind:value={query}
      onfocus={() => (open = true)}
      onblur={() => setTimeout(() => (open = false), 120)}
      onkeydown={onKey}
      autocomplete="off"
      spellcheck="false"
      role="combobox"
      aria-expanded={open}
      aria-controls="ws-results"
      aria-label="Search projects, products and actions"
      placeholder="Open a project, start a product, run an action…"
    />
    <kbd>/</kbd>
  </label>

  {#if open}
    <div class="results" id="ws-results" role="listbox">
      {#if flat.length === 0}
        <div class="none">No match. Try a project, a product or “clone”.</div>
      {:else}
        {#each groups as g (g.label)}
          <div class="group">{g.label}</div>
          {#each g.items as e (e.id)}
            {@const i = flat.indexOf(e)}
            {@const product = productOf(e)}
            <button
              type="button" class="res" id={`ws-opt-${i}`} role="option" aria-selected={i === sel}
              onmousedown={(ev) => { ev.preventDefault(); choose(e); }}
              onmousemove={() => (sel = i)}
            >
              {#if product}<ProductIcon id={product} size={22} />{:else}<span class="glyph"><Search size={12} /></span>{/if}
              <span class="t">{e.title}</span>
              <small>{e.detail}</small>
            </button>
          {/each}
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .omni { position: relative; }
  .box {
    display: flex; align-items: center; gap: 12px; height: 52px; padding: 0 16px;
    background: var(--bg-elevated); border: 1px solid var(--border); border-radius: 14px;
    box-shadow: 0 8px 28px rgba(0, 0, 0, 0.35);
    color: var(--text-secondary);
    transition: border-color var(--anim-dur-base), box-shadow var(--anim-dur-base);
  }
  .box:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent), 0 8px 28px rgba(0, 0, 0, 0.35); }
  input { flex: 1; min-width: 0; background: none; border: 0; outline: 0; color: var(--text-primary); font: inherit; font-size: 16px; }
  input::placeholder { color: var(--text-muted); }
  kbd {
    font: 11px var(--font-code); padding: 1px 6px; border: 1px solid var(--border); border-bottom-width: 2px;
    border-radius: 6px; background: var(--bg-base); color: var(--text-secondary);
  }
  .results {
    position: absolute; z-index: 20; left: 0; right: 0; top: 58px; max-height: 360px; overflow: auto; padding: 6px;
    background: var(--bg-overlay); border: 1px solid var(--border); border-radius: 12px; box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
  }
  .group { padding: 8px 10px 4px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-muted); }
  .res { display: flex; align-items: center; gap: 10px; width: 100%; padding: 8px 10px; border: 0; background: none; border-radius: 8px; text-align: left; cursor: pointer; color: var(--text-primary); font: inherit; }
  .res[aria-selected='true'] { background: color-mix(in srgb, var(--accent) 16%, transparent); }
  .res small { margin-left: auto; color: var(--text-muted); font: 11px var(--font-code); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 45%; direction: rtl; }
  .glyph { display: grid; place-items: center; width: 22px; height: 22px; border-radius: 7px; background: var(--bg-hover); color: var(--text-secondary); }
  .none { padding: 14px; color: var(--text-secondary); }
</style>
