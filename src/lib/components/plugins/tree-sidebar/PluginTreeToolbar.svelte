<!--
  PluginTreeToolbar — renders the `<ns>:toolbar` contribution point. Used
  twice by the dispatcher (once inside BottomPanelHeader.actions when docked
  at the bottom, once inside PanelShell.actions when docked as a sidebar).
  Extracted during the Phase 4 god-object refactor to kill the duplication.
-->
<script lang="ts">
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon from '../PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props { ns: string; }
  let { ns }: Props = $props();
</script>

<Contribution point="{ns}:toolbar">
  {#snippet item({ payload, fire })}
    {@const p = payload as { icon?: string; tooltip?: string; label?: string; accent?: boolean; success?: boolean; danger?: boolean; disabled?: boolean; divider_before?: boolean }}
    {#if p.divider_before}
      <span class="toolbar-divider"></span>
    {/if}
    <button
      type="button"
      class="ps-btn"
      class:ps-btn-accent={p.accent}
      class:ps-btn-success={p.success}
      class:ps-btn-danger={p.danger}
      use:tooltip={p.tooltip ?? p.label ?? ''}
      disabled={!!p.disabled}
      onclick={() => fire()}
    >
      {#if p.icon}<PluginIcon name={p.icon} size={13} />{/if}
    </button>
  {/snippet}
</Contribution>
