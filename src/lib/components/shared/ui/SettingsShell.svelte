<script module lang="ts">
  import type { IconComponent } from '$lib/types/icon';

  /** One entry in the settings sidebar nav. */
  export interface SettingsNavItem {
    id: string;
    label: string;
    /** Optional lucide (or any) icon component. */
    icon?: IconComponent;
    /**
     * A colour for the icon (any CSS colour or `var(--token)`) — the same option `Dropdown` and
     * `ContextMenu` carry.
     *
     * For identity, never decoration: a row that stands for a language or a framework is easier to
     * find by its colour than by reading four labels, and a page that stands for a setting is not.
     */
    iconColor?: string;
    /**
     * Pages that belong *under* this one — the tree an IDE's settings have.
     *
     * A parent is a page in its own right, not a folder: it is selected like any other entry, and
     * its children are what it splits into. Nesting stops here on purpose, because a third level is
     * where a settings tree stops being navigable and starts being a filesystem.
     */
    children?: SettingsNavItem[];
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
  import { ChevronDown, ChevronRight } from 'lucide-svelte';
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
  /** An entry survives a search when it matches, or when one of its children does — a parent whose
   *  child is the answer has to stay on screen, or the answer has nowhere to hang. */
  function visibleItem(item: SettingsNavItem): SettingsNavItem | null {
    if (hit(item.label)) return item;
    const kept = (item.children ?? []).filter((c) => hit(c.label));
    return kept.length ? { ...item, children: kept } : null;
  }
  function visibleItems(g: SettingsNavGroup): SettingsNavItem[] {
    if (!filtering || hit(g.label)) return g.items;
    return g.items.flatMap((i) => {
      const kept = visibleItem(i);
      return kept ? [kept] : [];
    });
  }
  const visibleGroups = $derived(
    groups.map((g) => ({ group: g, items: visibleItems(g) })).filter((x) => x.items.length > 0),
  );
  const noMatches = $derived(filtering && visibleGroups.length === 0);

  function idsOf(items: SettingsNavItem[]): string[] {
    return items.flatMap((i) => [i.id, ...idsOf(i.children ?? [])]);
  }

  // Keep `active` pointing at something visible when a query hides it.
  $effect(() => {
    if (!filtering) return;
    const ids = visibleGroups.flatMap((x) => idsOf(x.items));
    if (ids.length && !ids.includes(active)) active = ids[0];
  });

  // ── Expanded branches ───────────────────────────────────────────────────
  //
  // Collapsed by default would hide half the settings behind a disclosure nobody opens, so a branch
  // starts open and stays where the user puts it. A search opens everything it kept: a child that
  // matched is the whole reason its parent is still on screen.
  let collapsed = $state<Set<string>>(new Set());
  const isOpen = (item: SettingsNavItem) => filtering || !collapsed.has(item.id);

  function toggleBranch(item: SettingsNavItem) {
    const next = new Set(collapsed);
    if (next.has(item.id)) next.delete(item.id);
    else next.add(item.id);
    collapsed = next;
  }

  /** Selecting a parent opens it: its children are what the page splits into, and a page that
   *  answers "see the four pages under this" while hiding them is answering nothing. */
  function select(item: SettingsNavItem) {
    active = item.id;
    if (item.children?.length && collapsed.has(item.id)) toggleBranch(item);
  }

  /** The tree's own keys, on top of Tab: → opens a branch, ← closes it. */
  function onBranchKeydown(e: KeyboardEvent, item: SettingsNavItem) {
    if (!item.children?.length) return;
    const open = isOpen(item);
    if ((e.key === 'ArrowRight' && !open) || (e.key === 'ArrowLeft' && open)) {
      e.preventDefault();
      toggleBranch(item);
    }
  }

  /** A branch holding the active page is drawn as such while collapsed — otherwise selecting a
   *  child and collapsing its parent leaves nothing on screen saying where you are. */
  const activeTrail = $derived.by(() => {
    for (const { items } of visibleGroups) {
      for (const item of items) {
        if ((item.children ?? []).some((c) => c.id === active)) return item.id;
      }
    }
    return null;
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
      <!-- A group with nothing to expand gets no twisty column: the column exists to line labels up
           with the rows that have one, and a group where no row has one would be indenting every
           label away from the edge for nothing. -->
      {@const anyBranch = items.some((i) => (i.children ?? []).length > 0)}
      <div class="nav-group-label">{group.label}</div>
      {#each items as item (item.id)}
        {@const Icon = item.icon}
        {@const branch = (item.children ?? []).length > 0}
        {@const open = isOpen(item)}
        <div class="nav-branch">
          <!-- The row is a strip, not one control: the twisty is its own button, because a branch you
               can only open is not a branch. Selecting the row still opens it — that is what picking
               a page with children means — and the arrow is the way back. A button inside a button is
               invalid HTML, which is why the row's label is a sibling of the arrow rather than its
               parent, and the selected tint lives on the strip so it covers both. -->
          <div class="nav-row" class:active={active === item.id} class:trail={activeTrail === item.id && !open}>
            {#if branch}
              <button
                class="nav-twisty"
                class:open
                type="button"
                tabindex="-1"
                aria-label={`${open ? 'Collapse' : 'Expand'} ${item.label}`}
                aria-expanded={open}
                onclick={() => toggleBranch(item)}
              >
                {#if open}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
              </button>
            {:else if anyBranch}
              <span class="nav-twisty-gap" aria-hidden="true"></span>
            {/if}
            <button
              class="nav-item"
              onclick={() => select(item)}
              onkeydown={(e) => onBranchKeydown(e, item)}
              aria-expanded={branch ? open : undefined}
            >
              {#if Icon}<Icon size={13} />{/if}
              <span class="nav-label">{item.label}</span>
              {#if active === item.id}<ChevronRight size={11} class="nav-arrow" />{/if}
            </button>
          </div>
          {#if branch && open}
            {#each item.children ?? [] as child (child.id)}
              {@const ChildIcon = child.icon}
              <div class="nav-row nav-child" class:active={active === child.id}>
                <button class="nav-item" onclick={() => (active = child.id)}>
                  {#if ChildIcon}<ChildIcon size={12} />{/if}
                  <span class="nav-label">{child.label}</span>
                  {#if active === child.id}<ChevronRight size={11} class="nav-arrow" />{/if}
                </button>
              </div>
            {/each}
          {/if}
        </div>
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
  /* The strip: what is tinted, hovered and indented. Inside it the twisty and the label are two
     controls, so the tint has to live on their parent or it would stop at the arrow. */
  .nav-row {
    display: flex; align-items: center;
    padding-left: 8px;
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .nav-row:hover:not(.active) { background: var(--bg-hover); color: var(--text-primary); }
  .nav-row.active { background: var(--accent-subtle); color: var(--accent); font-weight: 500; }
  /* The branch you are inside, while it is closed. */
  .nav-row.trail { color: var(--text-primary); }
  .nav-row.trail .nav-twisty { color: var(--accent); }
  /* A child sits under its parent's label: the twisty column plus the row's own padding and gap. */
  .nav-row.nav-child { padding-left: 25px; }

  .nav-item {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 6px;
    padding: 5px 10px 5px 0;
    background: transparent; border: none; cursor: pointer;
    color: inherit; font-family: inherit; font-weight: inherit;
    font-size: var(--font-size-sm); text-align: left; position: relative;
  }
  .nav-branch { display: contents; }
  .nav-twisty, .nav-twisty-gap {
    display: inline-flex; align-items: center; justify-content: center;
    /* Fixed, never flexible: it is an 11px glyph and a column to line the labels up in. */
    flex: 0 0 11px; color: var(--text-disabled);
  }
  .nav-twisty {
    padding: 4px 0; margin-right: 6px;
    background: transparent; border: none; cursor: pointer;
    border-radius: var(--radius-sm);
    transition: color var(--transition-fast);
  }
  .nav-twisty:hover { color: var(--text-primary); }
  .nav-row:hover .nav-twisty { color: var(--text-secondary); }
  .nav-twisty-gap { margin-right: 6px; }
  .nav-icon { display: inline-flex; flex: none; }
  /* The selected row speaks with one voice: its icon takes the accent like everything else in it,
     whatever colour it wears at rest. */
  .nav-row.active .nav-icon { color: var(--accent) !important; }
  /* The LABEL takes the free space — named, not "any span in the row". The tree put two more spans
     in here (the twisty, and the gap that stands in for it on a leaf), and a bare `span` selector
     handed each of them `flex: 1` too: they ate the width, pushed icon and label into the middle of
     the nav and made every second label wrap. */
  .nav-item .nav-label { flex: 1; min-width: 0; }
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
  /* The icon carries the colour a page of grey rows otherwise has none of. Structural, not
     decorative: it marks where a card begins, which is the only thing the eye needs to find while
     scrolling a long page. State keeps its own colours — a warning is not an accent. */
  .content :global(.card-section-title svg) { color: var(--accent); opacity: 0.9; }
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

  /* ── The vocabulary a settings page writes with ──────────────────────
     One definition per concept, here, because every page of every product's settings needs the
     same handful: a hint under a card title, an empty line, a key/value fact, a list of paths you
     can remove from, chips, a warning, a read-only preview. Twelve copies of these drifted apart
     the moment one of them was improved. */
  .content :global(.set-hint) {
    font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; padding: 4px 2px 8px;
  }
  .content :global(.set-empty) {
    font-size: var(--font-size-sm); color: var(--text-muted); font-style: italic; padding: 4px 2px;
  }
  .content :global(.set-mono) { font-family: var(--font-code); }
  .content :global(.set-muted) { color: var(--text-muted); }
  .content :global(.set-invalid) {
    font-size: var(--font-size-xs); color: var(--error); padding: 2px 2px 4px;
  }

  .content :global(.set-kv) {
    display: flex; align-items: center; gap: 10px; padding: 6px 2px; font-size: var(--font-size-sm);
  }
  .content :global(.set-k) { width: 110px; flex-shrink: 0; color: var(--text-muted); }
  .content :global(.set-v) { color: var(--text-primary); }

  .content :global(.set-chips) { display: flex; flex-wrap: wrap; gap: 6px; padding: 10px 14px; }

  .content :global(.set-warn) {
    display: flex; align-items: center; gap: 7px; margin: 8px 2px 2px;
    padding: 7px 10px; font-size: var(--font-size-sm); line-height: 1.4;
    color: var(--warning); background: color-mix(in srgb, var(--warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--warning) 30%, transparent); border-radius: var(--radius-md);
  }
  .content :global(.set-warn svg) { flex-shrink: 0; }
  .content :global(.set-warn-error) {
    color: var(--error); background: color-mix(in srgb, var(--error) 12%, transparent);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
  }

  /* A list of things you add to and remove from — paths, patterns. */
  .content :global(.set-list) { display: flex; flex-direction: column; gap: 4px; padding: 4px 2px; }
  .content :global(.set-list-row) {
    display: flex; align-items: center; gap: 8px;
    padding: 5px 8px; background: var(--bg-base);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .content :global(.set-list-text) {
    flex: 1; min-width: 0; font-family: var(--font-code); font-size: var(--font-size-xs);
    color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left;
  }
  .content :global(.set-list-del) {
    display: flex; flex-shrink: 0; padding: 3px; background: transparent; border: none;
    color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .content :global(.set-list-del:hover) { color: var(--error); background: var(--bg-hover); }
  .content :global(.set-list-add) { display: flex; align-items: center; gap: 6px; padding: 6px 2px 2px; }
  .content :global(.set-list-add .input-wrap) { flex: 1; min-width: 0; }

  /* A read-only preview of what a setting does. */
  .content :global(.set-snippet) {
    margin: 10px 14px 12px; padding: 10px 12px;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    font-family: var(--font-code); font-size: var(--font-size-sm); line-height: 1.55;
    color: var(--text-primary); white-space: pre; overflow-x: auto; user-select: text;
  }
  .content :global(.set-snippet-wrap) { position: relative; }
  .content :global(.set-snippet-ruler) {
    position: absolute; top: 10px; bottom: 12px; width: 1px;
    background: var(--border-focus); opacity: 0.4; pointer-events: none;
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
