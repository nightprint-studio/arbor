<script lang="ts">
  import { Tag, Monitor, Globe } from 'lucide-svelte';
  import { laneColor } from '$lib/utils/graph-renderer';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import type { RefLabel } from '$lib/types/corvus/git';

  let { ref, colorIndex }: { ref: RefLabel; colorIndex?: number } = $props();

  const isLocalBranch  = $derived(ref.ref_type === 'local_branch');
  const isRemoteBranch = $derived(ref.ref_type === 'remote_branch');
  const isBranch       = $derived(isLocalBranch || isRemoteBranch);
  const isTag          = $derived(ref.ref_type === 'tag');

  async function copyTag(e: MouseEvent) {
    // Don't hijack ctx-menu / middle-click handlers higher up.
    if (e.button !== 0) return;
    e.stopPropagation();
    await copyToClipboard(ref.name, { successToast: `Copied "${ref.name}"`, errorToast: 'Copy failed' });
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

  // Remote branches drop the remote-name prefix (`origin/`, `upstream/`, …)
  // because the Globe icon already says "this is a remote", and on a narrow
  // Branches column those 8+ characters were just pushing the meaningful
  // suffix off the right edge. The full ref name still lives in the tooltip
  // and is what gets copied to the clipboard.
  const displayName = $derived(
    isRemoteBranch
      ? ref.name.slice(ref.name.indexOf('/') + 1) || ref.name
      : ref.name
  );
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
    {#if isRemoteBranch}
      <Globe size={10} />
    {:else}
      <Monitor size={10} />
    {/if}
    <span class="label-text">{displayName}</span>
  </span>
{/if}

<style>
  .badge-lane {
    color: var(--lc);
    background: color-mix(in srgb, var(--lc) 28%, transparent);
    border: 1px solid color-mix(in srgb, var(--lc) 55%, transparent);
    border-radius: 3px;
    font-weight: 600;
    letter-spacing: 0.3px;
    padding: 0 6px;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--lc) 12%, transparent);
  }

  /* Small breathing room between icon and label so the Tag glyph reads as
     a tag at small sizes, not a chevron.
     Long names are truncated from the LEFT (via `direction: rtl` +
     `text-align: left`) so the meaningful suffix — e.g. the bit after
     `feature/` — stays visible when the row runs out of room.
     No explicit `max-width` — the label fills whatever the parent column
     gives it, so resizing the Branches/Tags column expands the visible name. */
  .label-text {
    margin-left: 3px;
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
    text-align: left;
  }

  /* Lock the leading icon to its declared size. Without an explicit
     `flex-shrink: 0` the SVG (which is a flex item alongside `.label-text`)
     gets compressed when the chip shrinks to fit a narrow Branches/Tags
     column — the user reported icons squashed into thin slivers. The label
     is the only thing that should shrink; the glyph stays crisp. */
  .badge :global(svg) {
    flex-shrink: 0;
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
