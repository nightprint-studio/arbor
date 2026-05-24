<!--
  MarketplaceThemeDetail — right-pane content shown when a theme row is
  selected. Coloured header rendered with the theme's own bg/fg so the user
  sees an instant identity preview, plus the showcase mock, palette swatches,
  tags and source links in the body.
-->
<script lang="ts">
  import { Plus, Trash2, Tag, FolderGit2, ExternalLink } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import MarketplaceBadge         from './MarketplaceBadge.svelte';
  import MarketplaceThemeShowcase from './MarketplaceThemeShowcase.svelte';
  import type { MarketplaceTheme } from '$lib/types/marketplace';

  interface Props {
    theme:       MarketplaceTheme;
    busyId:      string | null;
    onInstall:   (t: MarketplaceTheme) => void;
    onUninstall: (t: MarketplaceTheme) => void;
  }

  let { theme: t, busyId, onInstall, onUninstall }: Props = $props();

  const busy = $derived(busyId === t.id);
</script>

<header class="head" style="background: {t.preview.bg};">
  <div
    class="preview-lg"
    style="background: {t.preview.bg}; color: {t.preview.fg}; border-color: {t.preview.accent};"
  >
    <span class="letter-lg" style="color: {t.preview.fg};">Aa</span>
    <div class="swatches-lg">
      <span class="sw" style="background: {t.preview.accent};"  use:tooltip={'accent'}></span>
      <span class="sw" style="background: {t.preview.success};" use:tooltip={'success'}></span>
      <span class="sw" style="background: {t.preview.warning};" use:tooltip={'warning'}></span>
      <span class="sw" style="background: {t.preview.error};"   use:tooltip={'error'}></span>
    </div>
  </div>

  <div class="headtext" style="color: {t.preview.fg};">
    <div class="name-row">
      <h2 class="name" style="color: {t.preview.fg};">{t.name}</h2>
      {#if t.variant}<MarketplaceBadge tone="variant">{t.variant}</MarketplaceBadge>{/if}
    </div>
    <div class="meta" style="color: {t.preview.fg}; opacity: 0.75;">
      {#if t.author}<span>by <strong>{t.author}</strong></span>{/if}
    </div>
  </div>

  <div class="actions">
    {#if t.installed}
      <Button variant="secondary" size="md"
              disabled={busy}
              onclick={() => onUninstall(t)}>
        {#snippet iconStart()}<Trash2 size={13} />{/snippet}
        {busy ? 'Removing…' : 'Remove'}
      </Button>
    {:else}
      <Button variant="primary" size="md"
              disabled={busy}
              loading={busy}
              onclick={() => onInstall(t)}>
        {#snippet iconStart()}<Plus size={13} />{/snippet}
        {busy ? 'Installing…' : 'Install'}
      </Button>
    {/if}
  </div>
</header>

<div class="body">
  <p class="desc">{t.description}</p>

  <MarketplaceThemeShowcase theme={t} />

  {#if (t.tags ?? []).length > 0}
    <section class="section">
      <h4><Tag size={11} /> Tags</h4>
      <div class="tag-list">
        {#each t.tags ?? [] as tg (tg)}
          <span class="tag-mini">{tg}</span>
        {/each}
      </div>
    </section>
  {/if}

  <section class="section">
    <h4><FolderGit2 size={11} /> Source</h4>
    <div class="source-rows">
      <div class="src-row">
        <span class="src-key">Repository</span>
        <a href={t.entry.repo} target="_blank" rel="noreferrer" class="src-link">
          {t.entry.repo}<ExternalLink size={10} />
        </a>
      </div>
      {#if t.entry.subpath}
        <div class="src-row">
          <span class="src-key">File</span>
          <code>{t.entry.subpath}</code>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  /* ── Coloured header — painted with the theme's own bg/fg so it doubles
       as an identity preview. ─────────────────────────────────────────── */
  .head {
    position: relative;
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .headtext { flex: 1; min-width: 0; z-index: 1; }
  .name-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .name {
    margin: 0;
    font-size: var(--font-size-lg);
    font-weight: 600;
  }
  .meta {
    font-size: var(--font-size-xs);
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .meta strong { font-weight: 500; }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }

  .preview-lg {
    width: 100px;
    height: 80px;
    border-radius: var(--radius-md);
    border: 2px solid;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    padding: 10px;
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.25);
  }
  .letter-lg {
    font-size: 24px;
    font-weight: 700;
    font-family: var(--font-mono);
    line-height: 1;
  }
  .swatches-lg { display: flex; gap: 4px; }
  .swatches-lg .sw {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.25);
  }

  /* ── Body ────────────────────────────────────────────────────────────── */
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 22px 28px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .desc {
    margin: 0;
    font-size: var(--font-size-sm);
    line-height: 1.55;
    color: var(--text-primary);
  }
  .section h4 {
    margin: 0 0 8px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
    font-weight: 600;
  }

  /* Tags */
  .tag-list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 5px;
    margin-top: 2px;
  }
  .tag-mini {
    font-size: 9.5px;
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-tag) 32%, transparent);
    color: var(--color-tag);
    padding: 1px 7px;
    border-radius: 999px;
    text-transform: lowercase;
    font-weight: 500;
    letter-spacing: 0.02em;
  }

  /* Source rows */
  .source-rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
  }
  .src-row {
    display: grid;
    grid-template-columns: 100px 1fr;
    gap: 12px;
    align-items: center;
    font-size: var(--font-size-xs);
  }
  .src-key {
    color: var(--text-muted);
    text-transform: uppercase;
    font-size: 10px;
    letter-spacing: 0.5px;
  }
  .src-link {
    color: var(--accent);
    text-decoration: none;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .src-link:hover { text-decoration: underline; }
  .src-row code {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    background: var(--bg-overlay);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }
</style>
