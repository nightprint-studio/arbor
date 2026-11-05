<script lang="ts">
  /**
   * The two thin strips on the window's left and right edge that bring a hidden activity bar back
   * while the pointer rests on them. Pair with `createEdgePeek()`: it owns the state and the
   * closing watcher, this only draws the surfaces that start it. Their geometry (below the title
   * bar, 4px wide) is the `.activity-bar-edge-trigger` rule in `app.css`.
   */
  interface Props {
    onLeft: () => void;
    onRight: () => void;
  }

  let { onLeft, onRight }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions a11y_mouse_events_have_key_events -->
<div class="activity-bar-edge-trigger edge-left" aria-hidden="true" onmouseenter={onLeft}></div>
<!-- svelte-ignore a11y_no_static_element_interactions a11y_mouse_events_have_key_events -->
<div class="activity-bar-edge-trigger edge-right" aria-hidden="true" onmouseenter={onRight}></div>
