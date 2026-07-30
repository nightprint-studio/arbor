<script lang="ts">
  /**
   * One product on the welcome page — the loudest element there, because
   * "what do I start" is the question this screen exists to answer.
   *
   * Everything the product needs is on the card: identity, state, version and
   * the action. No selection, no detail panel elsewhere to keep in sync. The
   * accent is the product's own, used only on the tile, the running edge and
   * the action, so five cards read as a family.
   */
  import Monogram from '$lib/components/shared/ui/Monogram.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import type { DecoratedTool } from '$lib/stores/launcher/state.svelte';

  interface Props {
    tool: DecoratedTool;
    onlaunch: () => void;
    onstop: () => void;
  }
  let { tool, onlaunch, onstop }: Props = $props();

  // `verMenu` entries are `{ v, active }`, not bare strings — the version string
  // is `v`, and mapping the whole record in gave the Select an object where it
  // wanted a value.
  const versions = $derived(tool.verMenu.map(({ v }) => ({ value: v, label: v })));
</script>

<article class="pc" class:running={tool.isRunning} style="--pc:{tool.accent}">
  <div class="pc-top">
    <Monogram name={tool.name} color={tool.accent} size={46} />
    <div class="pc-ident">
      <div class="pc-name">{tool.name}</div>
      <div class="pc-role">{tool.role}</div>
    </div>
  </div>

  {#if tool.blurb}
    <p class="pc-blurb">{tool.blurb}</p>
  {/if}

  <div class="pc-meta">
    <!-- Only the states worth reporting get a badge: "not running" on three of
         five cards is noise, and the Launch button already says it. Coloured
         explicitly so the state reads from across the grid. -->
    {#if tool.isRunning}
      <Badge size="md" color="var(--success)" dot label="Running" />
    {:else if tool.isUpd}
      <Badge size="md" color="var(--warning)" label="Update available" />
    {/if}
    <div class="pc-ver">
      {#if versions.length > 1}
        <Select value={tool.versionLabel} options={versions} narrow />
      {:else}
        <span class="pc-ver-label">{tool.versionLabel}</span>
      {/if}
    </div>
  </div>

  <!-- Real buttons, not full-width hairlines: the action keeps a comfortable
       size and sits left, with Stop beside it in the danger colour. -->
  <div class="pc-actions">
    <Button variant="tonal" color={tool.accent} size="md" onclick={onlaunch}>
      {tool.actionLabel}
    </Button>
    {#if tool.isRunning}
      <Button variant="danger" size="md" onclick={onstop}>Stop</Button>
    {/if}
  </div>
</article>

<style>
  .pc {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 17px 17px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border-subtle, var(--border));
    background: var(--bg-elevated);
    transition: border-color var(--anim-dur-fast), background var(--anim-dur-fast),
                transform var(--anim-dur-fast);
  }
  .pc:hover {
    background: var(--bg-hover);
    border-color: color-mix(in srgb, var(--pc) 45%, transparent);
    transform: translateY(-1px);
  }
  /* A running product carries a hairline of its own colour — readable across
     the whole grid without another badge. */
  .pc.running { box-shadow: inset 3px 0 0 var(--pc); }

  .pc-top { display: flex; align-items: center; gap: 12px; }
  .pc-ident { min-width: 0; }
  .pc-name {
    font-size: 17px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-primary);
  }
  .pc-role {
    font-size: 13px;
    color: var(--text-muted);
    margin-top: 2px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The blurb is what makes a card worth reading; it gets the room to be a
     sentence, and every card reserves the same height so the grid stays even. */
  .pc-blurb {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-secondary);
    min-height: calc(1.5em * 3);
  }

  .pc-meta { display: flex; align-items: center; gap: 8px; min-height: 24px; }
  .pc-ver { margin-left: auto; display: flex; align-items: center; }
  .pc-ver-label {
    font-family: var(--font-code);
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .pc-actions { display: flex; align-items: center; gap: 8px; }
  /* Give the primary action presence without stretching it across the card. */
  .pc-actions :global(.btn-tonal) { min-width: 108px; justify-content: center; }
</style>
