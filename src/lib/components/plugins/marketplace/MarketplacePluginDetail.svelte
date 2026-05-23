<!--
  MarketplacePluginDetail — right-pane content shown when a plugin row is
  selected. Composes the detail header (icon + name + version + source badge
  + actions) on top of the detail body (description + screenshots placeholder
  + permissions + tags + source links + authored doc when present).

  All actions are passed in as callbacks — the widget owns layout, the host
  owns the cascade modals + IPC + busy bookkeeping (which lives in the
  marketplace actions composable).
-->
<script lang="ts">
  import {
    Plus, RefreshCw, Trash2, X, Eye, Shield, Tag, FolderGit2,
    ExternalLink, GitBranch, Pin,
  } from 'lucide-svelte';
  import Button   from '$lib/components/shared/ui/Button.svelte';
  import Toggle   from '$lib/components/shared/ui/Toggle.svelte';
  import Alert    from '$lib/components/shared/ui/Alert.svelte';
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import PluginDocBlock from '$lib/components/shared/internal/PluginDocBlock.svelte';
  import MarketplaceBadge from './MarketplaceBadge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    isInlineSvg, sourceBadgeLabel, permissionChips,
  } from '$lib/marketplace/ui-helpers';
  import type { MarketplacePlugin } from '$lib/types/marketplace';

  interface Props {
    plugin:           MarketplacePlugin;
    /** Plugin name currently being installed / uninstalled / updated. Used to
     *  flip the action button into its busy state for the row that matches. */
    busyId:           string | null;
    /** Bumped by the host when a cascade-confirm modal is cancelled so the
     *  Toggle can reset its DOM state without changing the underlying prop. */
    toggleResetTick:  number;
    onInstall:        (p: MarketplacePlugin) => void;
    onUninstall:      (p: MarketplacePlugin) => void;
    onToggle:         (p: MarketplacePlugin, next: boolean) => void;
    onRemoveSource:   (p: MarketplacePlugin) => void;
  }

  let { plugin: p, busyId, toggleResetTick, onInstall, onUninstall, onToggle, onRemoveSource }: Props = $props();

  const busy = $derived(busyId === p.name);
</script>

<header class="head">
  {#if p.icon}
    {#if isInlineSvg(p.icon)}
      <span class="icon-art icon-art-lg" aria-hidden="true">{@html p.icon}</span>
    {:else}
      <img class="icon-art icon-art-lg" src={p.icon} alt="" />
    {/if}
  {:else}
    <Monogram name={p.name} initials={p.name[0].toUpperCase()}
              color="var(--accent-subtle)" fg="var(--accent)" size={56} />
  {/if}

  <div class="headtext">
    <div class="name-row">
      <h2 class="name">{p.name}</h2>
      {#if p.update_available}
        <span class="version version-old">v{p.installed_version ?? p.version}</span>
        <span class="version-arrow">→</span>
        <span class="version version-new">v{p.update_available}</span>
      {:else}
        <span class="version">v{p.installed_version ?? p.version}</span>
      {/if}
      <MarketplaceBadge tone={p.source}>{sourceBadgeLabel(p.source)}</MarketplaceBadge>
      {#if p.experimental}<MarketplaceBadge tone="experimental">Experimental</MarketplaceBadge>{/if}
    </div>
    <div class="meta">
      <span>by <strong>{p.author}</strong></span>
      {#if p.category}<span class="dot">·</span><span>{p.category}</span>{/if}
      {#if p.min_arbor_version}<span class="dot">·</span><span>requires Arbor ≥ {p.min_arbor_version}</span>{/if}
    </div>
  </div>

  <div class="actions">
    {#if p.installed}
      <div class="enable-toggle">
        <!-- `{#key}` forces a Toggle remount when the user cancels a
             cascade-confirm modal. Without this the underlying
             <input bind:checked> keeps its just-clicked DOM state because
             the parent prop (`p.enabled`) never changed — and the switch
             visually stays on while the backend is still off. -->
        {#key toggleResetTick}
          <Toggle
            checked={p.enabled ?? false}
            size="md"
            label={p.enabled ? 'Enabled' : 'Disabled'}
            labelPosition="before"
            onchange={(v) => onToggle(p, v)}
          />
        {/key}
      </div>
      {#if p.update_available}
        <span use:tooltip={`Re-install at v${p.update_available}`}>
          <Button
            variant="primary" size="md"
            disabled={busy}
            loading={busy}
            onclick={() => onInstall(p)}
          >
            {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
            {busy ? 'Updating…' : `Update to v${p.update_available}`}
          </Button>
        </span>
      {/if}
      <Button
        variant="secondary" size="md"
        disabled={busy}
        onclick={() => onUninstall(p)}
      >
        {#snippet iconStart()}<Trash2 size={13} />{/snippet}
        {busy ? 'Removing…' : 'Uninstall'}
      </Button>
    {:else}
      <Button
        variant="primary" size="md"
        disabled={busy}
        loading={busy}
        onclick={() => onInstall(p)}
      >
        {#snippet iconStart()}<Plus size={13} />{/snippet}
        {busy ? 'Installing…' : 'Install'}
      </Button>
    {/if}
    {#if p.source === 'custom'}
      <span use:tooltip={'Forget this custom source (installed plugins stay)'}>
        <Button
          variant="ghost" size="md"
          onclick={() => onRemoveSource(p)}
        >
          {#snippet iconStart()}<X size={13} />{/snippet}
          Remove source
        </Button>
      </span>
    {/if}
  </div>
</header>

<div class="body">
  <p class="desc">{p.description}</p>

  <!-- Screenshots (mock placeholder) -->
  <section class="section">
    <h4><Eye size={11} /> Preview</h4>
    <div class="screenshots">
      <div class="shot-placeholder">
        <span>Screenshots ship with the plugin's repo.</span>
        <small>Set <code>screenshots = ["docs/1.png", …]</code> in <code>plugin.toml</code> to surface them here.</small>
      </div>
    </div>
  </section>

  <!-- Permissions -->
  {#if p.permissions}
    {@const chips = permissionChips(p)}
    <section class="section">
      <h4><Shield size={11} /> Permissions requested</h4>
      {#if chips.length === 0}
        <p class="muted">This plugin requests no elevated permissions.</p>
      {:else}
        <div class="perm-list">
          {#each chips as c, i (i)}
            <span class="perm-chip perm-{c.tone}">
              <c.icon size={10} />{c.label}
            </span>
          {/each}
        </div>
        <p class="muted small">
          Arbor will show a confirmation dialog with the resolved list before installing.
          Plugins are <strong>disabled by default</strong> after install — you'll need to enable them manually
          from the Plugin Manager.
        </p>
      {/if}
    </section>
  {/if}

  <!-- Tags -->
  {#if (p.tags ?? []).length > 0}
    <section class="section">
      <h4><Tag size={11} /> Tags</h4>
      <div class="tag-list">
        {#each p.tags ?? [] as tg (tg)}
          <span class="tag-mini">{tg}</span>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Source links -->
  <section class="section">
    <h4><FolderGit2 size={11} /> Source</h4>
    <div class="source-rows">
      <div class="src-row">
        <span class="src-key">Repository</span>
        <a href={p.entry.repo} target="_blank" rel="noreferrer" class="src-link">
          {p.entry.repo}
          <ExternalLink size={10} />
        </a>
      </div>
      {#if p.entry.subpath}
        <div class="src-row">
          <span class="src-key">Subpath</span>
          <code>{p.entry.subpath}</code>
        </div>
      {/if}
      {#if p.entry.ref}
        <div class="src-row">
          <span class="src-key">Ref</span>
          <code><GitBranch size={9} /> {p.entry.ref}</code>
        </div>
      {:else}
        <div class="src-row">
          <span class="src-key">Ref</span>
          <span class="muted small">latest tag (fallback: <code>main</code>)</span>
        </div>
      {/if}
      {#if p.entry.pinned_sha}
        <div class="src-row">
          <span class="src-key">Pinned SHA</span>
          <code><Pin size={9} /> {p.entry.pinned_sha.slice(0, 12)}</code>
        </div>
      {/if}
      {#if p.homepage}
        <div class="src-row">
          <span class="src-key">Homepage</span>
          <a href={p.homepage} target="_blank" rel="noreferrer" class="src-link">
            {p.homepage}<ExternalLink size={10} />
          </a>
        </div>
      {/if}
    </div>
  </section>

  {#if p.source === 'custom'}
    <section class="section">
      <Alert variant="warning" title="Third-party source">
        This plugin lives outside the curated registry. Review its <code>plugin.toml</code>
        and <code>main.lua</code> on GitHub before enabling it — declared permissions
        describe what the plugin <em>can</em> do once enabled.
      </Alert>
    </section>
  {/if}

  {#if p.doc}
    <!-- Authored HTML from plugin.toml's doc_file. Lives at the bottom of
         the detail body because the doc is reference material — permissions,
         source and install actions are the primary decision surface above.
         The typography baseline lives in the shared `PluginDocBlock` so a
         plugin authored once renders identically here and in the docs panel. -->
    <section class="section">
      <h4><Eye size={11} /> Documentation</h4>
      <PluginDocBlock html={p.doc} card />
    </section>
  {/if}
</div>

<style>
  /* ── Header ──────────────────────────────────────────────────────────── */
  .head {
    display: flex;
    gap: 14px;
    align-items: flex-start;
    padding: 18px 22px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .headtext { flex: 1; min-width: 0; }
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
    color: var(--text-primary);
  }
  .version {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-mono);
  }
  .version-old {
    color: var(--text-disabled);
    text-decoration: line-through;
    text-decoration-thickness: 1px;
  }
  .version-new {
    color: var(--warning);
    border-color: color-mix(in srgb, var(--warning) 40%, transparent);
  }
  .version-arrow {
    color: var(--text-disabled);
    font-size: 10px;
  }
  .meta {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .meta strong { color: var(--text-secondary); font-weight: 500; }
  .dot { opacity: 0.5; }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }
  .enable-toggle {
    display: inline-flex;
    align-items: center;
    padding: 4px 12px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
  }

  /* ── Icon art (large) ────────────────────────────────────────────────── */
  .icon-art {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    background: var(--accent-subtle);
    color: var(--accent);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .icon-art-lg { width: 56px; height: 56px; }
  .icon-art-lg :global(svg) { width: 32px; height: 32px; display: block; }

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
  .muted { color: var(--text-muted); font-size: var(--font-size-xs); margin: 0; }
  .muted.small { font-size: 11px; margin-top: 6px; }

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

  /* Permissions */
  .perm-list { display: flex; flex-wrap: wrap; gap: 5px; }
  .perm-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-mono);
    border: 1px solid var(--border-subtle);
  }
  .perm-safe   { background: color-mix(in srgb, var(--success) 12%, transparent); color: var(--success); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .perm-warn   { background: color-mix(in srgb, var(--warning) 12%, transparent); color: var(--warning); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .perm-danger { background: color-mix(in srgb, var(--error) 12%, transparent);   color: var(--error);   border-color: color-mix(in srgb, var(--error) 35%, transparent); }

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

  /* Screenshots placeholder */
  .screenshots { display: flex; gap: 8px; }
  .shot-placeholder {
    flex: 1;
    border: 1px dashed var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 18px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    text-align: center;
  }
  .shot-placeholder small { color: var(--text-disabled); font-size: 10.5px; }

</style>
