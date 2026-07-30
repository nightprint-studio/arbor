<script module lang="ts">
  import type { IconComponent } from '$lib/types/icon';

  /** One entry in the settings sidebar nav. */
  export interface SettingsNavItem {
    id: string;
    label: string;
    /** Optional lucide (or any) icon component. */
    icon?: IconComponent;
  }
  /** A labelled group of nav entries (the uppercase category headers). */
  export interface SettingsNavGroup {
    label: string;
    items: SettingsNavItem[];
  }
</script>

<script lang="ts">
  /**
   * SettingsShell — the shared two-pane settings layout (the look Arbor's
   * SettingsPanel established): a `bg-elevated` frame revealing two floating
   * `bg-base` cards — a grouped, searchable nav on the left and a scrollable
   * content pane on the right. App-agnostic: the host passes the nav groups and
   * a `content` snippet that switches on the bound `active` id.
   *
   * The content helper classes (`.section-header`, `.card`, `.card-section-title`,
   * `.card-row-note`, `.info-box`) are styled here (as `:global` within the
   * content pane) so any consumer's sections read identically — pair them with
   * the shared `FormRow` for each setting.
   *
   *   <SettingsShell {groups} bind:active>
   *     {#snippet content()}
   *       {#if active === 'general'}
   *         <div class="section-header"><h2>General</h2><p>…</p></div>
   *         <div class="card"> <FormRow …/> </div>
   *       {/if}
   *     {/snippet}
   *   </SettingsShell>
   */
  import type { Snippet } from 'svelte';
  import { ChevronRight } from 'lucide-svelte';
  import { fade } from 'svelte/transition';
  import SearchBar from './SearchBar.svelte';
  import { animStore } from '$lib/stores/animations.svelte';

  let {
    groups,
    active = $bindable(),
    content,
    searchable = true,
    searchPlaceholder = 'Search settings…',
  }: {
    groups: SettingsNavGroup[];
    active: string;
    content: Snippet;
    searchable?: boolean;
    searchPlaceholder?: string;
  } = $props();

  let query = $state('');
  let regex = $state(false);

  // null → no filter; 'invalid' → bad regex (match nothing); else a {test}.
  const matcher = $derived.by<RegExp | { test: (s: string) => boolean } | 'invalid' | null>(() => {
    const t = query.trim();
    if (!t) return null;
    if (regex) { try { return new RegExp(t, 'i'); } catch { return 'invalid'; } }
    const lower = t.toLowerCase();
    return { test: (s: string) => s.toLowerCase().includes(lower) };
  });
  const regexInvalid = $derived(matcher === 'invalid');
  const filtering = $derived(matcher !== null);

  function hit(s: string): boolean {
    return matcher != null && matcher !== 'invalid' ? matcher.test(s) : false;
  }
  function visibleItems(g: SettingsNavGroup): SettingsNavItem[] {
    if (!filtering || hit(g.label)) return g.items;
    return g.items.filter((i) => hit(i.label));
  }
  const visibleGroups = $derived(
    groups.map((g) => ({ group: g, items: visibleItems(g) })).filter((x) => x.items.length > 0),
  );
  const noMatches = $derived(filtering && visibleGroups.length === 0);

  // Keep `active` pointing at something visible when a query hides it.
  $effect(() => {
    if (!filtering) return;
    const ids = visibleGroups.flatMap((x) => x.items.map((i) => i.id));
    if (ids.length && !ids.includes(active)) active = ids[0];
  });
</script>

<div class="settings-body">
  <nav class="nav" aria-label="Settings sections">
    {#if searchable}
      <div class="nav-search">
        <SearchBar bind:query bind:regex {regexInvalid} showCounter={false}
                   placeholder={searchPlaceholder} ariaLabel="Search settings"
                   onClear={() => (query = '')} />
      </div>
    {/if}

    {#if noMatches}
      <p class="search-empty">{regexInvalid ? 'Invalid regex pattern' : 'No matches'}</p>
    {/if}

    {#each visibleGroups as { group, items } (group.label)}
      <div class="nav-group-label">{group.label}</div>
      {#each items as item (item.id)}
        {@const Icon = item.icon}
        <button class="nav-item" class:active={active === item.id} onclick={() => (active = item.id)}>
          {#if Icon}<Icon size={13} />{/if}
          <span>{item.label}</span>
          {#if active === item.id}<ChevronRight size={11} class="nav-arrow" />{/if}
        </button>
      {/each}
    {/each}
  </nav>

  {#key active}
    <div class="content" in:fade={{ duration: animStore.dFast }}>
      {@render content()}
    </div>
  {/key}
</div>

<style>
  /* ── Shell ──────────────────────────────────────────────────────────
     bg-elevated reveals as a 4px gap around floating bg-base cards. */
  .settings-body {
    display: flex; height: 100%; min-height: 0; overflow: hidden;
    background: var(--bg-elevated); padding: 4px;
  }

  /* ── Nav ────────────────────────────────────────────────────────── */
  .nav {
    width: 230px; flex-shrink: 0;
    background: var(--bg-base); border-radius: 12px; margin-right: 4px;
    padding: 8px 0 16px;
    display: flex; flex-direction: column; overflow-y: auto;
  }
  .nav-search { margin: 0 8px 6px; }
  .search-empty {
    font-size: var(--font-size-xs); color: var(--text-muted); padding: 12px 14px; margin: 0;
    text-align: center; font-style: italic;
  }
  .nav-group-label {
    font-size: var(--font-size-2xs); font-weight: 600; color: var(--text-disabled);
    text-transform: uppercase; letter-spacing: 0.7px; padding: 10px 14px 4px;
  }
  .nav-item {
    display: flex; align-items: center; gap: 8px;
    padding: 6px 12px 6px 14px;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-secondary); font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm); text-align: left; position: relative;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .nav-item:hover:not(.active) { background: var(--bg-hover); color: var(--text-primary); }
  .nav-item.active { background: var(--accent-subtle); color: var(--accent); font-weight: 500; }
  .nav-item span { flex: 1; }
  :global(.nav-arrow) { opacity: 0.55; flex-shrink: 0; }

  /* ── Content area ───────────────────────────────────────────────── */
  .content {
    flex: 1; min-height: 0;
    background: var(--bg-base); border-radius: 12px;
    padding: 22px 24px 32px; overflow-y: auto;
    display: flex; flex-direction: column; gap: 16px;
  }
  .content > :global(*) { flex-shrink: 0; }

  /* Section header */
  .content :global(.section-header) { margin-bottom: 4px; }
  .content :global(.section-header h2) {
    font-size: var(--font-size-lg); font-weight: 600; color: var(--text-primary); margin: 0 0 4px;
  }
  .content :global(.section-header p) {
    font-size: var(--font-size-xs); color: var(--text-muted); margin: 0; line-height: 1.5;
  }

  /* Card */
  .content :global(.card) {
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); overflow: hidden;
  }
  .content :global(.card-section-title) {
    display: flex; align-items: center; gap: 6px;
    font-size: var(--font-size-xs); font-weight: 600; color: var(--text-muted);
    text-transform: uppercase; letter-spacing: 0.5px;
    padding: 10px 14px 8px; border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
  }
  .content :global(.card-row-note) {
    font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.55;
    padding: 8px 14px 10px; border-bottom: 1px solid var(--border-subtle);
  }

  /* Inline code */
  .content :global(code) {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-secondary);
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 0 4px;
  }

  /* Info box */
  .content :global(.info-box) {
    display: flex; align-items: flex-start; gap: 8px;
    background: var(--bg-overlay); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); padding: 10px 14px;
    color: var(--text-muted); font-size: var(--font-size-xs); line-height: 1.55;
  }
  .content :global(.info-box svg) { flex-shrink: 0; margin-top: 1px; opacity: 0.7; }
</style>
