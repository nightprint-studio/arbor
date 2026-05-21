<!--
  InfoCard — hero header card for "you're looking at X" panels.

  Layout:
    ┌─────────────────────────────────────────────────────────────────┐
    │  ┌──┐  Title-text  [Badge] [Badge]    [action] [action] [action] │
    │  │ M│  subtitle • meta1 N • meta2 N • meta3 N                    │
    │  └──┘                                                            │
    └─────────────────────────────────────────────────────────────────┘

  All slots are optional except `title`. Avatar accepts either a Lucide
  icon name (resolved via PLUGIN_ICONS) or a 1-2 letter monogram string.
  Meta items render as "label value" pairs with a subtle separator.
-->
<script module lang="ts">
  // ── Public types (module scope so consumers can `import type` them).
  //    Svelte 5 disallows `export type/interface` from a regular <script>. ──
  export type InfoBadgeKind = 'info' | 'success' | 'warning' | 'error' | 'accent' | 'muted';

  export interface InfoBadge {
    text: string;
    kind?: InfoBadgeKind;
  }

  export interface InfoMeta {
    label?: string;
    value:  string;
    /** Tooltip on hover (typical use: full type path when value is shortened). */
    tooltip?: string;
  }

  export interface InfoAction {
    /** Lucide icon name (PLUGIN_ICONS). */
    icon:    string;
    label?:  string;
    tooltip?: string;
    variant?: 'default' | 'primary' | 'danger';
    disabled?: boolean;
    onClick: () => void;
  }
</script>

<script lang="ts">
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import Badge from './Badge.svelte';

  interface Props {
    title:       string;
    subtitle?:   string;
    /** Either an icon name or a short monogram string. When omitted, no avatar is rendered. */
    icon?:       string;
    monogram?:   string;
    /** Right-aligned status pill next to the title (e.g. "Live"). */
    status?:     { text: string; kind?: InfoBadgeKind };
    badges?:     InfoBadge[];
    meta?:       InfoMeta[];
    actions?:    InfoAction[];
    /** Custom accent for the avatar — defaults to --accent. */
    accentColor?: string;
  }

  let {
    title, subtitle, icon, monogram, status,
    badges = [], meta = [], actions = [],
    accentColor,
  }: Props = $props();

  const IconComp = $derived(icon ? PLUGIN_ICONS[icon] : null);
</script>

<div class="info-card">
  {#if IconComp || monogram}
    <div
      class="info-avatar"
      style={accentColor ? `--info-accent: ${accentColor};` : undefined}
    >
      {#if IconComp}
        <IconComp size={18} />
      {:else}
        <span class="info-monogram">{monogram?.slice(0, 2).toUpperCase() ?? ''}</span>
      {/if}
    </div>
  {/if}

  <div class="info-main">
    <div class="info-titlerow">
      <span class="info-title">{title}</span>
      {#if status}
        <Badge variant="tone" tone={(status.kind ?? 'accent') as any} size="md">{status.text}</Badge>
      {/if}
      {#each badges as b}
        <Badge variant="tone" tone={(b.kind ?? 'muted') as any} size="md">{b.text}</Badge>
      {/each}
    </div>

    {#if subtitle || meta.length > 0}
      <div class="info-meta">
        {#if subtitle}
          <span class="info-subtitle">{subtitle}</span>
        {/if}
        {#each meta as m, i}
          {#if i > 0 || subtitle}<span class="info-meta-sep">·</span>{/if}
          <span class="info-meta-item" title={m.tooltip ?? undefined}>
            {#if m.label}<span class="info-meta-label">{m.label}</span>{/if}
            <span class="info-meta-value">{m.value}</span>
          </span>
        {/each}
      </div>
    {/if}
  </div>

  {#if actions.length > 0}
    <div class="info-actions">
      {#each actions as act}
        {@const AIcon = PLUGIN_ICONS[act.icon] ?? null}
        <button
          type="button"
          class="info-action"
          class:primary={act.variant === 'primary'}
          class:danger={act.variant === 'danger'}
          disabled={!!act.disabled}
          title={act.tooltip ?? act.label ?? undefined}
          onclick={act.onClick}
        >
          {#if AIcon}<AIcon size={12} />{/if}
          {#if act.label}<span class="info-action-label">{act.label}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .info-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.18);
  }

  .info-avatar {
    --info-accent: var(--accent);
    width: 44px;
    height: 44px;
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--info-accent) 16%, transparent);
    color: var(--info-accent);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid color-mix(in srgb, var(--info-accent) 28%, transparent);
    font-weight: 700;
    flex-shrink: 0;
    box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.03) inset;
  }
  .info-monogram { font-size: 15px; letter-spacing: 0.4px; }

  .info-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .info-titlerow {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .info-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-ui-sans);
  }

  .info-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
    font-size: 11px;
    line-height: 1.4;
    min-width: 0;
  }
  .info-subtitle {
    color: var(--text-secondary);
  }
  .info-meta-item {
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
  }
  .info-meta-label {
    color: var(--text-disabled);
    text-transform: uppercase;
    font-size: 9px;
    letter-spacing: 0.5px;
    font-weight: 600;
  }
  .info-meta-value {
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-feature-settings: "tnum";
  }
  .info-meta-sep {
    color: var(--text-disabled);
    user-select: none;
  }

  .info-actions {
    display: flex;
    gap: 4px;
    align-items: center;
    flex-shrink: 0;
  }
  .info-action {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    height: 24px;
    border-radius: 5px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    color: var(--text-secondary);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 500;
  }
  .info-action:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .info-action.primary {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: color-mix(in srgb, var(--accent) 32%, transparent);
  }
  .info-action.primary:hover:not(:disabled) { background: color-mix(in srgb, var(--accent) 18%, transparent); }
  .info-action.danger {
    color: var(--error);
    background: color-mix(in srgb, var(--error) 10%, transparent);
    border-color: color-mix(in srgb, var(--error) 30%, transparent);
  }
  .info-action.danger:hover:not(:disabled) { background: color-mix(in srgb, var(--error) 18%, transparent); }
  .info-action:disabled { opacity: 0.45; cursor: default; }
  .info-action-label { white-space: nowrap; }
</style>
