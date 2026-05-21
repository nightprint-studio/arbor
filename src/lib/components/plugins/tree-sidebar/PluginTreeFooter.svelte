<!--
  PluginTreeFooter — renders the `<ns>:footer` contribution row at the
  bottom of the sidebar. Each contribution can be either a text label or a
  button; the wrapper auto-scrolls horizontally when a busy footer would
  otherwise push the body up. Extracted during the Phase 4 god-object
  refactor.
-->
<script lang="ts">
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon from '../PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props { ns: string; }
  let { ns }: Props = $props();
</script>

<div class="footer-row">
  <Contribution point="{ns}:footer">
    {#snippet item({ payload, fire })}
      {@const p = payload as { kind?: string; icon?: string; tooltip?: string; label?: string; accent?: boolean; disabled?: boolean; badge?: string; badge_kind?: string }}
      {#if (p.kind ?? 'button') === 'text'}
        <span class="footer-text" use:tooltip={p.tooltip ?? ''}>
          {#if p.icon}<PluginIcon name={p.icon} size={11} />{/if}
          {p.label ?? ''}
          {#if p.badge}<span class="dec-badge dec-badge-{p.badge_kind ?? 'muted'}">{p.badge}</span>{/if}
        </span>
      {:else}
        <button
          type="button"
          class="footer-btn"
          class:accent={p.accent}
          use:tooltip={p.tooltip ?? p.label ?? ''}
          disabled={!!p.disabled}
          onclick={() => fire()}
        >
          {#if p.icon}<PluginIcon name={p.icon} size={11} />{/if}
          {#if p.label}<span>{p.label}</span>{/if}
        </button>
      {/if}
    {/snippet}
  </Contribution>
</div>
