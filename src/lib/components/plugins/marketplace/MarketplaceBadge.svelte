<!--
  MarketplaceBadge — pill badge used to label sources (Community / Custom /
  Local) and per-entry markers (Experimental, theme variant). Centralised so
  the colour palette stays in one place across DetailEmpty + PluginDetail +
  ThemeDetail and never drifts.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  type Tone = 'community' | 'custom' | 'local' | 'experimental' | 'variant';

  interface Props {
    tone:     Tone;
    children: Snippet;
  }

  let { tone, children }: Props = $props();
</script>

<span class="badge badge-{tone}">{@render children()}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.3px;
  }
  .badge-community {
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .badge-custom {
    background: color-mix(in srgb, var(--warning) 15%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 35%, transparent);
  }
  /* Local — info tone. Sideloaded / dev plugins aren't trusted by us but
     they're not third-party either: the user put them there themselves.
     The `--info` palette makes them visually distinct from community
     (accent) and custom (warning) without flagging them as "warn". */
  .badge-local {
    background: color-mix(in srgb, var(--info) 16%, transparent);
    color: var(--info);
    border: 1px solid color-mix(in srgb, var(--info) 38%, transparent);
  }
  .badge-experimental {
    background: color-mix(in srgb, var(--warning) 20%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 40%, transparent);
  }
  .badge-variant {
    background: var(--bg-overlay);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    text-transform: capitalize;
  }
</style>
