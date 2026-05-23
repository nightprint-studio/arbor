<!--
  MarketplaceStatusIcons — compact glyph cluster shown on marketplace list rows
  (and reused on detail headers later) to surface a row's status without
  taking the space of verbose pills.

  The cluster always carries the source-of-truth marker (Community / Custom /
  Local). When `installed === true` it also shows an installed indicator —
  plugins use `enabledState` to flip between "enabled" (green check) and
  "disabled" (muted power-off); themes omit `enabledState` and get the plain
  "installed" check. Plugin-only props (`updateAvailable`, `experimental`)
  add their own glyphs as needed.

  Every glyph carries its own tooltip — the full labels live in the detail
  pane, so the row stays scannable.
-->
<script lang="ts">
  import { CheckCircle2, PowerOff, RefreshCw, FlaskConical } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { sourceIcon, sourceBadgeTooltip } from '$lib/marketplace/ui-helpers';
  import type { MarketplaceSource } from '$lib/types/marketplace';

  interface Props {
    source:           MarketplaceSource;
    installed?:       boolean;
    /** Plugin-only. When set, the "installed" glyph reflects enable state
     *  (CheckCircle2 vs PowerOff). Omit on themes — they get the plain
     *  "Installed" check with the matching tooltip. */
    enabledState?:    'enabled' | 'disabled';
    /** Plugin-only. When set, the update glyph is shown with a tooltip
     *  showing the version delta. */
    updateAvailable?: string;
    installedVersion?: string;
    /** Plugin-only. */
    experimental?:    boolean;
  }

  let {
    source,
    installed = false,
    enabledState,
    updateAvailable,
    installedVersion,
    experimental = false,
  }: Props = $props();

  const SrcIcon = $derived(sourceIcon(source));
</script>

<span class="row-icons">
  <span class="rowicon rowicon-source rowicon-{source}"
        use:tooltip={sourceBadgeTooltip(source)}>
    <SrcIcon size={12} />
  </span>

  {#if installed}
    {#if enabledState === 'disabled'}
      <span class="rowicon rowicon-off"
            use:tooltip={'Installed but disabled — flip the toggle in the detail pane to activate'}>
        <PowerOff size={12} />
      </span>
    {:else}
      <span class="rowicon rowicon-installed"
            use:tooltip={enabledState === 'enabled' ? 'Installed and enabled' : 'Installed'}>
        <CheckCircle2 size={12} />
      </span>
    {/if}
  {/if}

  {#if updateAvailable}
    <span class="rowicon rowicon-update"
          use:tooltip={`Update available — installed v${installedVersion ?? '?'} · catalog v${updateAvailable}`}>
      <RefreshCw size={12} />
    </span>
  {/if}

  {#if experimental}
    <span class="rowicon rowicon-experimental"
          use:tooltip={'Flagged experimental in its manifest'}>
      <FlaskConical size={12} />
    </span>
  {/if}
</span>

<style>
  .row-icons {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 2px;
  }
  .rowicon {
    display: inline-flex;
    align-items: center;
    cursor: help;
    color: var(--text-disabled);
    line-height: 0;
  }
  .rowicon-installed    { color: var(--success);     }
  .rowicon-off          { color: var(--text-muted);  }
  .rowicon-update       { color: var(--color-stash); }
  .rowicon-experimental { color: var(--warning);     }
  .rowicon-community    { color: var(--accent);      }
  .rowicon-custom       { color: var(--warning);     }
  .rowicon-local        { color: var(--info);        }
</style>
