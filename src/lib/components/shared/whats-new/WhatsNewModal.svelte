<script lang="ts">
  /**
   * WhatsNewModal — release-notes dialog auto-opened after every upgrade,
   * also reachable from the Command Palette and the About modal.
   *
   *   - Content source is `CHANGELOG.md` (parsed via `utils/changelog.ts`)
   *     so the same file users read on GitHub drives the in-app modal.
   *   - Bullet text is rendered through the shared inline-markdown helper —
   *     `**bold**`, `[link](url)`, `` `code` `` and emoji shortcodes all
   *     work, matching how the same lines render on GitHub.
   *   - For "hero" releases an optional per-version Svelte component under
   *     `./versions/v<X.Y.Z>.svelte` is rendered above the auto-generated
   *     section list — that's where curated screenshots, GIFs or short
   *     videos live. Minor releases without a hero just get the parsed
   *     CHANGELOG view; nothing extra to maintain per release.
   */
  import type { Component } from 'svelte';
  import { Sparkles, Plus, Pencil, Bug, Trash2, ShieldAlert, ArchiveX, ExternalLink, ChevronRight } from 'lucide-svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';

  import Modal       from '../Modal.svelte';
  import ModalHeader from '../ModalHeader.svelte';
  import ModalFooter from '../ModalFooter.svelte';
  import Button      from '../ui/Button.svelte';

  import { whatsNewStore } from '$lib/stores/whats_new.svelte';
  import { findEntry, CHANGELOG_GROUPS, type ChangelogGroup } from '$lib/utils/changelog';
  import { renderInlineMarkdown } from '$lib/utils/markdown';

  // Per-version hero overrides are eagerly bundled (the modal is small and
  // hot-loaded on every upgrade — avoiding the dynamic-import round trip
  // keeps first paint instant). Each module's default export is a Svelte
  // component receiving no props. Filename convention: `vX.Y.Z.svelte`.
  const HERO_MODULES = import.meta.glob<{ default: Component }>(
    './versions/v*.svelte',
    { eager: true },
  );

  function heroFor(version: string): Component | null {
    const key = `./versions/v${version}.svelte`;
    return HERO_MODULES[key]?.default ?? null;
  }

  const version = $derived(whatsNewStore.currentVersion || 'this version');
  const entry   = $derived(findEntry(whatsNewStore.currentVersion));
  const Hero    = $derived(heroFor(whatsNewStore.currentVersion));

  // Visible groups: keep CHANGELOG ordering, drop empties, preserve unknown
  // group names at the end so future categories surface instead of vanishing.
  const visibleGroups = $derived.by(() => {
    if (!entry) return [] as Array<{ key: string; items: string[] }>;
    const ordered: Array<{ key: string; items: string[] }> = [];
    for (const g of CHANGELOG_GROUPS) {
      const items = entry.groups[g];
      if (items?.length) ordered.push({ key: g, items });
    }
    for (const [k, v] of Object.entries(entry.groups)) {
      if (!CHANGELOG_GROUPS.includes(k as ChangelogGroup) && v?.length) {
        ordered.push({ key: k, items: v });
      }
    }
    return ordered;
  });

  const totalCount = $derived(visibleGroups.reduce((n, g) => n + g.items.length, 0));

  // Group icon + accent colour, keyed on the canonical group names.
  // Unknown groups fall back to the "Added" look so they're still legible.
  function groupMeta(key: string) {
    switch (key) {
      case 'Added':      return { Icon: Plus,        accent: 'var(--success)',  rgb: '46, 160, 67'   };
      case 'Changed':    return { Icon: Pencil,      accent: 'var(--accent)',   rgb: '77, 120, 204'  };
      case 'Fixed':      return { Icon: Bug,         accent: 'var(--warning)',  rgb: '210, 153, 34'  };
      case 'Removed':    return { Icon: Trash2,      accent: 'var(--danger)',   rgb: '218, 54, 51'   };
      case 'Deprecated': return { Icon: ArchiveX,    accent: 'var(--text-muted)', rgb: '138, 145, 156' };
      case 'Security':   return { Icon: ShieldAlert, accent: 'var(--danger)',   rgb: '218, 54, 51'   };
      default:           return { Icon: Sparkles,    accent: 'var(--accent)',   rgb: '77, 120, 204'  };
    }
  }

  function close() { whatsNewStore.hide(); }

  async function openExternal(url: string) {
    try { await openUrl(url); } catch { /* ignore */ }
  }

  // Section refs let the chip-bar at the top scroll to a specific group on
  // click — JetBrains "What's New" pattern. Keyed on the group label so the
  // mapping survives unknown groups too.
  const sectionEls: Record<string, HTMLElement | undefined> = {};
  function scrollToGroup(key: string) {
    sectionEls[key]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  // Subtitle pitch — used when there's no per-version hero. Length tuned
  // to look right under the gradient title without wrapping awkwardly.
  function pitch(n: number): string {
    if (n === 0) return 'No release notes recorded for this version.';
    if (n === 1) return 'One highlight from this release — straight from the changelog.';
    return `${n} highlights from this release — straight from the changelog.`;
  }
</script>

<Modal
  onClose={close}
  width="960px"
  height="720px"
  padBody={false}
  ariaLabel="What's New in Arbor"
>
  {#snippet header()}
    <ModalHeader onClose={close}>
      <span class="header-mark"><Sparkles size={14} /></span>
      <span class="modal-title">What's New</span>
      <span class="header-version">v{version}</span>
      {#if entry?.date}
        <span class="header-date">{entry.date}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="wn-body">
    {#if Hero}
      <!-- Per-version hero (screenshots / video / curated layout). -->
      <section class="wn-hero wn-hero-custom">
        <Hero />
      </section>
    {:else}
      <section class="wn-hero wn-hero-default">
        <div class="wn-hero-grid"></div>
        <div class="wn-hero-glow wn-hero-glow-1"></div>
        <div class="wn-hero-glow wn-hero-glow-2"></div>
        <div class="wn-hero-inner">
          <span class="wn-hero-eyebrow">
            <Sparkles size={11} />
            <span>Release notes</span>
          </span>
          <h2 class="wn-hero-title">What's new in Arbor {version}</h2>
          <p class="wn-hero-sub">
            {entry?.intro || pitch(totalCount)}
          </p>
          {#if visibleGroups.length > 0}
            <div class="wn-chipbar">
              {#each visibleGroups as g (g.key)}
                {@const meta = groupMeta(g.key)}
                {@const ChipIcon = meta.Icon}
                <button
                  type="button"
                  class="wn-chip"
                  style="--c-accent: {meta.accent}; --c-rgb: {meta.rgb};"
                  onclick={() => scrollToGroup(g.key)}
                  aria-label="Jump to {g.key} section"
                >
                  <span class="wn-chip-icon"><ChipIcon size={11} /></span>
                  <span class="wn-chip-label">{g.key}</span>
                  <span class="wn-chip-count">{g.items.length}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </section>
    {/if}

    {#if !entry || visibleGroups.length === 0}
      <section class="wn-empty">
        <div class="wn-empty-mark"><Sparkles size={20} /></div>
        <p class="wn-empty-title">No release notes for this version</p>
        <p class="wn-empty-sub">The full project changelog lives on GitHub.</p>
      </section>
    {:else}
      <div class="wn-sections">
        {#each visibleGroups as group (group.key)}
          {@const meta = groupMeta(group.key)}
          {@const GroupIcon = meta.Icon}
          <section
            class="wn-group"
            bind:this={sectionEls[group.key]}
            style="--g-accent: {meta.accent}; --g-rgb: {meta.rgb};"
          >
            <header class="wn-group-head">
              <span class="wn-group-icon"><GroupIcon size={14} /></span>
              <span class="wn-group-title">{group.key}</span>
              <span class="wn-group-count">{group.items.length}</span>
              <span class="wn-group-rule"></span>
            </header>
            <ul class="wn-group-list">
              {#each group.items as item}
                <li class="wn-item">
                  <span class="wn-item-dot"><ChevronRight size={11} /></span>
                  <span class="wn-item-body md-body">{@html renderInlineMarkdown(item)}</span>
                </li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <button
        type="button"
        class="wn-link"
        onclick={() => openExternal('https://github.com/nightprint-studio/arbor/blob/main/CHANGELOG.md')}
      >
        <ExternalLink size={11} />
        <span>Full changelog on GitHub</span>
      </button>
      <Button variant="primary" onclick={close}>Got it</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  /* ── Header ─────────────────────────────────────────────────────────── */

  .header-mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: linear-gradient(135deg, var(--accent-subtle), rgba(200, 168, 255, 0.22));
    color: var(--accent);
    flex-shrink: 0;
  }
  .header-version {
    font-size: var(--font-size-2xs);
    font-family: var(--font-code);
    color: var(--accent);
    background: var(--accent-subtle);
    border: 1px solid rgba(77, 120, 204, 0.3);
    border-radius: var(--radius-sm);
    padding: 1.5px 7px;
    flex-shrink: 0;
  }
  .header-date {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    margin-left: 2px;
  }

  /* ── Body shell ─────────────────────────────────────────────────────── */

  .wn-body {
    height: 100%;
    overflow-y: auto;
    font-family: var(--font-ui-sans);
    scroll-behavior: smooth;
  }
  /* Make the scrollbar feel native to the rest of Arbor — a subtle gutter
     instead of the chunky browser default that breaks the modal's chrome. */
  .wn-body::-webkit-scrollbar           { width: 10px; }
  .wn-body::-webkit-scrollbar-track     { background: transparent; }
  .wn-body::-webkit-scrollbar-thumb     {
    background: rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }
  .wn-body::-webkit-scrollbar-thumb:hover { background: rgba(255, 255, 255, 0.16); background-clip: padding-box; border: 2px solid transparent; }

  /* ── Hero ───────────────────────────────────────────────────────────── */

  .wn-hero {
    position: relative;
    border-bottom: 1px solid var(--border);
    overflow: hidden;
  }
  .wn-hero-custom {
    padding: 28px 32px;
  }
  .wn-hero-default {
    padding: 36px 36px 30px;
    background:
      linear-gradient(135deg, rgba(20, 28, 48, 0.65), rgba(28, 22, 48, 0.5)),
      var(--bg-elevated);
  }
  .wn-hero-grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(to right, rgba(255, 255, 255, 0.025) 1px, transparent 1px),
      linear-gradient(to bottom, rgba(255, 255, 255, 0.025) 1px, transparent 1px);
    background-size: 32px 32px;
    mask-image: radial-gradient(ellipse at center, black 30%, transparent 80%);
    -webkit-mask-image: radial-gradient(ellipse at center, black 30%, transparent 80%);
    pointer-events: none;
  }
  .wn-hero-glow {
    position: absolute;
    border-radius: 50%;
    pointer-events: none;
  }
  .wn-hero-glow-1 {
    inset: -60% -20% auto auto;
    width: 380px;
    height: 380px;
    background: radial-gradient(circle, rgba(107, 158, 255, 0.22), transparent 65%);
  }
  .wn-hero-glow-2 {
    inset: auto auto -80% -10%;
    width: 320px;
    height: 320px;
    background: radial-gradient(circle, rgba(200, 168, 255, 0.18), transparent 65%);
  }
  .wn-hero-inner {
    position: relative;
    z-index: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .wn-hero-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--font-size-2xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--accent);
    align-self: flex-start;
    padding: 3px 9px 3px 7px;
    background: var(--accent-subtle);
    border: 1px solid rgba(77, 120, 204, 0.3);
    border-radius: 999px;
  }
  .wn-hero-title {
    margin: 4px 0 2px;
    font-size: 26px;
    font-weight: 700;
    letter-spacing: -0.015em;
    line-height: 1.15;
    background: linear-gradient(135deg, #8db4ff 0%, #c8a8ff 50%, #ffb4d9 100%);
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .wn-hero-sub {
    margin: 0;
    font-size: var(--font-size-md);
    line-height: 1.55;
    color: var(--text-secondary);
    max-width: 640px;
  }

  /* ── Hero chip-bar (jump-to-section) ─────────────────────────────────── */

  .wn-chipbar {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 14px;
  }
  .wn-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px 4px 7px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 999px;
    color: var(--text-secondary);
    font-size: var(--font-size-xs);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    transition:
      background var(--transition-fast),
      border-color var(--transition-fast),
      color var(--transition-fast),
      transform var(--transition-fast);
  }
  .wn-chip:hover {
    background: rgba(var(--c-rgb), 0.14);
    border-color: rgba(var(--c-rgb), 0.45);
    color: var(--text-primary);
    transform: translateY(-1px);
  }
  .wn-chip:focus-visible {
    outline: 1px solid var(--c-accent);
    outline-offset: 2px;
  }
  .wn-chip-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(var(--c-rgb), 0.18);
    color: var(--c-accent);
  }
  .wn-chip-label {
    font-weight: 500;
  }
  .wn-chip-count {
    font-size: var(--font-size-2xs);
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
    padding-left: 1px;
  }

  /* ── Sections ────────────────────────────────────────────────────────── */

  .wn-sections {
    padding: 22px 36px 28px;
    display: flex;
    flex-direction: column;
    gap: 28px;
  }

  .wn-group {
    /* Per-group accent already injected via inline `style` (--g-accent / --g-rgb). */
    scroll-margin-top: 12px;
  }
  .wn-group-head {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 14px;
  }
  .wn-group-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    background: rgba(var(--g-rgb), 0.14);
    color: var(--g-accent);
    border: 1px solid rgba(var(--g-rgb), 0.28);
    box-shadow: 0 0 0 3px rgba(var(--g-rgb), 0.06);
    flex-shrink: 0;
  }
  .wn-group-title {
    font-size: var(--font-size-md);
    font-weight: 600;
    color: var(--text-primary);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .wn-group-count {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
    padding: 1px 7px;
    background: rgba(255, 255, 255, 0.04);
    border-radius: 999px;
  }
  .wn-group-rule {
    flex: 1;
    height: 1px;
    background: linear-gradient(
      to right,
      rgba(var(--g-rgb), 0.4),
      var(--border) 35%,
      transparent
    );
  }

  .wn-group-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .wn-item {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 16px 12px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition:
      border-color var(--transition-fast),
      transform var(--transition-fast),
      box-shadow var(--transition-fast);
  }
  .wn-item:hover {
    border-color: rgba(var(--g-rgb), 0.45);
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.18);
    transform: translateY(-1px);
  }
  .wn-item-dot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: rgba(var(--g-rgb), 0.16);
    color: var(--g-accent);
    flex-shrink: 0;
    margin-top: 2px;
  }
  .wn-item-body {
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-secondary);
    overflow-wrap: anywhere;
    flex: 1;
    min-width: 0;
  }

  /* Inline markdown classes coming from `renderInlineMarkdown`. Match the
     subset emitted by the shared helper: strong / em / md-link / md-inline-code. */
  .md-body :global(strong) { color: var(--text-primary); font-weight: 600; }
  .md-body :global(em)     { color: var(--text-secondary); font-style: italic; }
  .md-body :global(.md-link) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    text-decoration-color: rgba(77, 120, 204, 0.5);
  }
  .md-body :global(.md-inline-code) {
    font-family: var(--font-code);
    font-size: 0.92em;
    padding: 1px 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
  }

  /* ── Empty state ─────────────────────────────────────────────────────── */

  .wn-empty {
    padding: 60px 24px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
  }
  .wn-empty-mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    border-radius: 50%;
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid rgba(77, 120, 204, 0.28);
  }
  .wn-empty-title {
    margin: 0;
    font-size: var(--font-size-md);
    color: var(--text-primary);
    font-weight: 500;
  }
  .wn-empty-sub {
    margin: 0;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }

  /* ── Footer link ─────────────────────────────────────────────────────── */

  .wn-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: none;
    border: none;
    padding: 0;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: color var(--transition-fast);
  }
  .wn-link:hover {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .wn-link:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
    border-radius: 2px;
  }
</style>
