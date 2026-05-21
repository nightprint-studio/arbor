<script lang="ts">
  import { nodeX, nodeY, NODE_RADIUS, laneColor } from '$lib/utils/graph-renderer';
  import { avatarUrl } from '$lib/stores/avatars.svelte';
  import type { CommitNode } from '$lib/types/git';

  let {
    node,
    selected = false,
    highlighted = false,
    synced = false,
    bisectMark = null,
    onclick,
    oncontextmenu,
    onmouseenter,
    onmouseleave,
  }: {
    node: CommitNode;
    selected?: boolean;
    highlighted?: boolean;
    synced?: boolean;
    /** 'bad' | 'good' | 'next' — bisect mark indicator, null when not bisecting. */
    bisectMark?: 'bad' | 'good' | 'next' | 'result' | null;
    onclick?: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onmouseenter?: () => void;
    onmouseleave?: () => void;
  } = $props();

  const cx         = $derived(nodeX(node.lane));
  const cy         = $derived(nodeY(node.row));
  const color      = $derived(laneColor(node.color_index));
  const dotOpacity = 1;
  // Selected-node highlight follows the theme: white on dark, dark on light.
  // Anything that was "#ffffff" in the original SVG now resolves through the
  // CSS variable layer, so light-theme presets get the correct contrast for
  // free without each preset having to declare the token.
  const SELECTED_FILL = 'var(--graph-selected-fill, #ffffff)';
  const ringStroke = $derived(selected ? SELECTED_FILL : color);
  const ringWidth  = $derived(selected ? 2.5 : 1.8);

  // Bisect indicator colors — bound to theme tokens so the banner and the
  // inline node markers always stay in sync.
  const BISECT_BAD  = 'var(--color-bisect, #e05252)';
  const BISECT_GOOD = 'var(--color-bisect-good, #3fb950)';
  const BISECT_NEXT = 'var(--color-bisect-next, #e0a93a)';

  // Merge commits are rendered at half scale so they don't visually compete
  // with avatar nodes — they just mark topology, not authorship.
  const MR = $derived(node.is_merge ? Math.round(NODE_RADIUS * 0.65) : NODE_RADIUS);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<g
  class="graph-node"
  class:selected
  class:merge={node.is_merge}
  {onclick}
  {oncontextmenu}
  {onmouseenter}
  {onmouseleave}
  role="button"
  tabindex="-1"
>
  {#if !node.is_merge}
    <!-- ── Regular commit: circular avatar ── -->

    {#if node.is_head}
      <!-- HEAD: soft halo (no blur filter — would create a GPU layer per HEAD) -->
      <circle
        cx={cx} cy={cy}
        r={NODE_RADIUS + 8}
        fill={color}
        style="opacity: calc(0.18 * var(--graph-halo-intensity, 1));"
      />
      <circle
        cx={cx} cy={cy}
        r={NODE_RADIUS + 4}
        fill="none"
        stroke={color}
        stroke-width="2"
        opacity={selected ? 1 : 0.75}
      />
      <!-- Downward-pointing triangle pin: always visible, no halo-intensity dependency -->
      <polygon
        points="{cx},{cy - NODE_RADIUS - 2} {cx - 5},{cy - NODE_RADIUS - 10} {cx + 5},{cy - NODE_RADIUS - 10}"
        fill={color}
        opacity="0.95"
        pointer-events="none"
      />
    {:else}
      <!-- Ambient glow halo for all non-HEAD regular commits.
           Selected commits get a slightly larger, brighter halo; others are subtle.
           Halo intensity is zeroed out in light themes via --graph-halo-intensity. -->
      <circle
        cx={cx} cy={cy}
        r={selected ? NODE_RADIUS + 5 : NODE_RADIUS + 3}
        fill={selected ? SELECTED_FILL : color}
        style="opacity: calc({selected ? 0.22 : 0.11} * var(--graph-halo-intensity, 1));"
      />
    {/if}

    <!-- Colored border ring (turns white when selected) -->
    <circle
      cx={cx} cy={cy}
      r={NODE_RADIUS + 1}
      fill="none"
      stroke={ringStroke}
      stroke-width={node.is_head ? 2.5 : ringWidth}
      opacity={dotOpacity}
    />

    <!-- Avatar image — clipped by the <clipPath id="ac-{oid}"> defined in CommitGraph -->
    <image
      href={avatarUrl(node.author.email, node.author.name)}
      x={cx - NODE_RADIUS}
      y={cy - NODE_RADIUS}
      width={NODE_RADIUS * 2}
      height={NODE_RADIUS * 2}
      clip-path="url(#ac-{node.oid})"
      preserveAspectRatio="xMidYMid slice"
      opacity={dotOpacity}
    />

  {:else}
    <!-- ── Merge commit: diamond dot (no avatar) ── -->

    {#if node.is_head}
      <!-- HEAD: soft halo (no blur filter — avoids a GPU layer per HEAD) -->
      <circle
        cx={cx} cy={cy}
        r={MR + 8}
        fill={color}
        style="opacity: calc(0.18 * var(--graph-halo-intensity, 1));"
      />
      <circle
        cx={cx} cy={cy}
        r={MR + 3}
        fill="none"
        stroke={color}
        stroke-width="2"
        opacity={selected ? 1 : 0.75}
      />
      <!-- Downward-pointing triangle pin: always visible, no halo-intensity dependency -->
      <polygon
        points="{cx},{cy - MR - 2} {cx - 5},{cy - MR - 10} {cx + 5},{cy - MR - 10}"
        fill={color}
        opacity="0.95"
        pointer-events="none"
      />
    {:else}
      <!-- Ambient glow halo for non-HEAD merge nodes.
           Halo intensity is zeroed out in light themes via --graph-halo-intensity. -->
      <circle
        cx={cx} cy={cy}
        r={selected ? MR + 4 : MR + 2}
        fill={selected ? SELECTED_FILL : color}
        style="opacity: calc({selected ? 0.20 : 0.09} * var(--graph-halo-intensity, 1));"
      />
    {/if}

    <!-- Simple filled dot -->
    <circle
      cx={cx} cy={cy}
      r={MR}
      fill={selected ? SELECTED_FILL : color}
      opacity={dotOpacity}
    />
  {/if}

  <!-- ── Bisect mark ring ── rendered on top of everything else in this node -->
  {#if bisectMark}
    {@const baseR    = node.is_merge ? MR : NODE_RADIUS}
    {@const isResult = bisectMark === 'result'}
    {@const bisectR  = baseR + (isResult ? 7 : 5)}
    {@const bisectC  = bisectMark === 'good' ? BISECT_GOOD
                     : bisectMark === 'next'  ? BISECT_NEXT
                     : BISECT_BAD}
    {@const isDashed = bisectMark === 'next'}

    {#if isResult}
      <!-- Strong outer glow for the culprit commit (no blur — layer cost) -->
      <circle
        cx={cx} cy={cy}
        r={bisectR + 8}
        fill={BISECT_BAD}
        style="opacity: calc(0.12 * var(--graph-halo-intensity, 1));"
        pointer-events="none"
      />
      <!-- Second inner glow ring (stays visible even in light themes — it's
           structural, not a halo) -->
      <circle
        cx={cx} cy={cy}
        r={bisectR + 4}
        fill="none"
        stroke={BISECT_BAD}
        stroke-width="1"
        opacity="0.4"
        pointer-events="none"
      />
    {:else}
      <!-- Standard soft glow (no blur — layer cost) -->
      <circle
        cx={cx} cy={cy}
        r={bisectR + 4}
        fill={bisectC}
        style="opacity: calc(0.10 * var(--graph-halo-intensity, 1));"
        pointer-events="none"
      />
    {/if}

    <!-- The ring itself -->
    <circle
      cx={cx} cy={cy}
      r={bisectR}
      fill="none"
      stroke={bisectC}
      stroke-width={isResult ? 2.5 : bisectMark === 'next' ? 1.5 : 2}
      stroke-dasharray={isDashed ? '3 2' : null}
      opacity={bisectMark === 'next' ? 0.9 : 1}
      class={isResult ? 'bisect-result-ring' : bisectMark === 'next' ? 'bisect-next-ring' : ''}
      pointer-events="none"
    />
  {/if}
</g>

<style>
  .graph-node { cursor: pointer; }
  /* No opacity transition: it would promote every circle/image to a GPU layer
     on state change, which during scroll/selection piles up as Layerize cost. */

  .bisect-next-ring {
    animation: bisect-pulse 1.8s ease-in-out infinite;
    transform-origin: center;
    transform-box: fill-box;
  }

  .bisect-result-ring {
    animation: bisect-result-pulse 1.4s ease-in-out infinite;
    transform-origin: center;
    transform-box: fill-box;
  }

  @keyframes bisect-pulse {
    0%, 100% { opacity: 0.9; }
    50%       { opacity: 0.35; }
  }

  @keyframes bisect-result-pulse {
    0%, 100% { opacity: 1; }
    50%       { opacity: 0.5; }
  }
</style>
