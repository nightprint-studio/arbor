<!--
  PluginActivityBar — routing-only activity bar rendered inside a plugin
  modal's `leftRail` / `rightRail` snippet of `<Modal>`. Maps a flat
  `FormActivityBarItem[]` to `<button class="ab-btn">` elements that fit
  the shared `.activity-bar` chrome already provided by `<ActivityBar>`.

  Routing-only by design (Q2 round 3 decision): clicking an item flips
  the active sidecar id, never fires a plugin action. Action-style buttons
  (Open file…, Save As…) belong in `form.header.left` / `form.header.right`
  as regular `button` FormNodes.
-->
<script lang="ts">
  import { PLUGIN_ICONS } from '$lib/utils/plugin-icons';
  import { Circle } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { FormActivityBarItem } from '$lib/types/plugin';

  interface Props {
    items:      FormActivityBarItem[];
    activeId:   string | null;
    onSelect:   (id: string) => void;
  }

  let { items, activeId, onSelect }: Props = $props();

  // Resolve a Lucide name to a component, with a `Circle` fallback so a
  // typo never renders an invisible 24×24 click target.
  function iconFor(name: string) {
    return PLUGIN_ICONS[name] ?? Circle;
  }

  function isAction(it: FormActivityBarItem): it is Extract<FormActivityBarItem, { id: string }> {
    return !(it as any).separator;
  }
</script>

{#each items as it, i (i)}
  {#if isAction(it)}
    {@const Ic = iconFor(it.icon)}
    {@const tip = it.tooltip ?? it.label}
    {@const cnt = typeof it.count === 'number' && it.count > 0 ? it.count : null}
    <button
      type="button"
      class="ab-btn"
      class:ab-active={activeId === it.id}
      class:pf-ab-disabled={!!it.disabled}
      disabled={!!it.disabled}
      aria-label={it.label}
      aria-pressed={activeId === it.id}
      use:tooltip={tip}
      onclick={() => { if (!it.disabled) onSelect(it.id); }}
    >
      <Ic size={18} />
      {#if cnt !== null}
        <span class="pf-ab-badge pf-ab-tone-{it.tone ?? 'accent'}">{cnt}</span>
      {:else if it.dot}
        <span class="pf-ab-dot pf-ab-tone-{it.tone ?? 'accent'}" aria-hidden="true"></span>
      {/if}
    </button>
  {:else}
    <div class="ab-separator" aria-hidden="true"></div>
  {/if}
{/each}

<style>
  /* The base `.ab-btn` rules are emitted globally by ActivityBar.svelte.
     Only the count/dot decorators are local to this widget. */
  .pf-ab-badge {
    position: absolute;
    top: 1px;
    right: 1px;
    min-width: 14px;
    height: 14px;
    padding: 0 4px;
    border-radius: 8px;
    font-size: 9px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
    color: var(--text-on-accent);
    box-shadow: 0 0 0 1px var(--bg-elevated);
  }
  .pf-ab-dot {
    position: absolute;
    top: 4px;
    right: 4px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--bg-elevated);
  }

  .pf-ab-tone-info,
  .pf-ab-tone-accent  { background: var(--accent); }
  .pf-ab-tone-success { background: var(--success, #2ea043); }
  .pf-ab-tone-warning { background: var(--warning, #d29922); color: #1c1c1c; }
  .pf-ab-tone-error   { background: var(--error, #e06c75); }
  .pf-ab-tone-muted   { background: var(--text-muted, #6c7280); }

  :global(.ab-btn.pf-ab-disabled) {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
