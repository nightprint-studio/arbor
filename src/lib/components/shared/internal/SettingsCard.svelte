<script lang="ts">
  /**
   * The framed block a settings section's rows sit in.
   *
   * Exists because the frame used to be ambient: `SettingsShell` and Corvus's `SettingsPanel`
   * each declare a `:global(.card)` inside their content pane, and a section written for either
   * of them picked it up for free. That held exactly as long as every settings section lived in
   * one of those two shells. The moment the shared sections started rendering in the explorer's
   * in-page settings and in Tyto's modal — neither of which has such a rule — the same component
   * arrived framed in three places and as a bare stack of rows in two.
   *
   * A component rather than a fourth copy of the rule: the alternative was the same five
   * declarations in every section that wants to be portable, which is how two of them end up
   * with different corner radii a year from now.
   *
   * `shared/ui/Card` is the general-purpose one and is not this: it pads its content, and these
   * hold `FormRow`s that draw their own dividers edge to edge.
   */
  import type { Snippet } from 'svelte';

  let { spaced = false, children }: {
    /** Leave a gap under it. For a section that stacks several cards rather than one;
     *  a shell that lays its sections out with a `gap` does not want this. */
    spaced?: boolean;
    children: Snippet;
  } = $props();
</script>

<div class="settings-card" class:spaced>{@render children()}</div>

<style>
  .settings-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .spaced { margin-bottom: 12px; }
</style>
