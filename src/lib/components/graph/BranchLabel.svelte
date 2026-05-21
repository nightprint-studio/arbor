<script lang="ts">
  import { Tag, GitBranch } from 'lucide-svelte';
  import { laneColor } from '$lib/utils/graph-renderer';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { RefLabel } from '$lib/types/git';

  let { ref, colorIndex }: { ref: RefLabel; colorIndex?: number } = $props();

  const isLocalBranch  = $derived(ref.ref_type === 'local_branch');
  const isRemoteBranch = $derived(ref.ref_type === 'remote_branch');
  const isBranch       = $derived(isLocalBranch || isRemoteBranch);
  const isTag          = $derived(ref.ref_type === 'tag');

  async function copyTag(e: MouseEvent) {
    // Don't hijack ctx-menu / middle-click handlers higher up.
    if (e.button !== 0) return;
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(ref.name);
      uiStore.showToast(`Copied "${ref.name}"`, 'info');
    } catch {
      uiStore.showToast('Copy failed', 'error');
    }
  }

  // `is_current` from the backend means "this ref's target OID equals HEAD's
  // target OID" — which is true for tags AND remote branches that happen to
  // sit on the checked-out commit. The green HEAD pill should only apply to
  // the actual local branch HEAD points at, so we gate on `isLocalBranch`.
  const cls = $derived(
    ref.is_current && isLocalBranch
      ? 'badge badge-head'
      : isBranch
        ? colorIndex !== undefined
          ? 'badge badge-lane'
          : 'badge badge-branch'
        : isTag
          ? 'badge badge-tag'
          : 'badge badge-branch'
  );

  const laneStyle = $derived(
    isBranch && colorIndex !== undefined
      ? `--lc: ${laneColor(colorIndex)}`
      : undefined
  );

  // Remote branches keep the full "origin/branch" name so they're visually
  // distinct from the matching local branch when both appear in the graph.
  const displayName = $derived(ref.name);
</script>

{#if isTag}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <span
    class={cls}
    style={laneStyle}
    use:tooltip={{ content: ref.name, description: 'Click to copy' }}
    role="button"
    tabindex="0"
    onclick={copyTag}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); copyTag(new MouseEvent('click')); } }}
  >
    <Tag size={11} /><span class="label-text">{displayName}</span>
  </span>
{:else}
  <span class={cls} style={laneStyle} use:tooltip={ref.name}>
    <GitBranch size={10} /><span class="label-text">{displayName}</span>
  </span>
{/if}

<style>
  .badge-lane {
    color: var(--lc);
    background: color-mix(in srgb, var(--lc) 28%, transparent);
    border: 1px solid color-mix(in srgb, var(--lc) 55%, transparent);
    font-weight: 600;
    letter-spacing: 0.3px;
    padding: 0 8px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--lc) 12%, transparent);
  }

  /* Small breathing room between icon and label so the Tag glyph reads as
     a tag at small sizes, not a chevron.
     Long names are truncated from the LEFT (via `direction: rtl` +
     `text-align: left`) so the meaningful suffix — e.g. the bit after
     `feature/` — stays visible when the row runs out of room. */
  .label-text {
    margin-left: 3px;
    display: inline-block;
    max-width: 180px;
    vertical-align: bottom;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  /* Tag chips are click-to-copy — give them cursor + hover affordance. */
  .badge-tag[role='button'] {
    cursor: pointer;
    transition: filter var(--transition-fast), transform var(--transition-fast);
  }
  .badge-tag[role='button']:hover   { filter: brightness(1.15); }
  .badge-tag[role='button']:active  { transform: translateY(1px); }
  .badge-tag[role='button']:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 1px;
  }
</style>
