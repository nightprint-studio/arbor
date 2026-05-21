<!--
  PluginTreeNode — renders one row inside the plugin tree. Lays out the host-
  side `icon · label · badge · decorator · actions` segments, then defers the
  `<ns>:node_decorator` and `<ns>:node_action` contribution points to any
  plugin that wants to inject badges/buttons on a matching node.

  This is the body of the `<Tree>` `row` snippet, extracted during the
  Phase 4 god-object refactor.
-->
<script lang="ts">
  import Contribution from '$lib/components/shared/Contribution.svelte';
  import PluginIcon from '../PluginIcon.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { TreeNode } from '$lib/types/contribution';

  interface Props {
    ns:   string;
    node: TreeNode;
  }
  let { ns, node }: Props = $props();
</script>

{#if node.icon}
  <span class="tree-icon"><PluginIcon name={node.icon} size={13} /></span>
{/if}
<span class="tree-label">{node.label}</span>
{#if node.badge}
  <span class="tree-badge tree-badge-{node.badge_kind ?? 'muted'}">{node.badge}</span>
{/if}

<!-- always-on decorator slot (icons / badges contributed by other plugins) -->
<span class="tree-decorator">
  <Contribution point="{ns}:node_decorator" whenContext={node}>
    {#snippet item({ payload })}
      {@const p = payload as { icon?: string; tooltip?: string; badge?: string; badge_kind?: string }}
      {#if p.icon}
        <span class="dec-icon" use:tooltip={p.tooltip ?? ''}>
          <PluginIcon name={p.icon} size={12} />
        </span>
      {/if}
      {#if p.badge}
        <span class="dec-badge dec-badge-{p.badge_kind ?? 'muted'}">{p.badge}</span>
      {/if}
    {/snippet}
  </Contribution>
</span>

<!-- hover-reveal action zone -->
<span class="tree-actions">
  <Contribution point="{ns}:node_action" whenContext={node}>
    {#snippet item({ payload, fire })}
      {@const p = payload as { icon?: string; tooltip?: string; label?: string; accent?: boolean; danger?: boolean; disabled?: boolean }}
      <button
        type="button"
        class="tree-row-action"
        class:accent={p.accent}
        class:danger={p.danger}
        use:tooltip={p.tooltip ?? p.label ?? ''}
        disabled={!!p.disabled}
        onclick={(e) => { e.stopPropagation(); fire({ node_id: node.id, data: node.data }); }}
      >
        {#if p.icon}<PluginIcon name={p.icon} size={12} />{/if}
      </button>
    {/snippet}
  </Contribution>
</span>
