<script lang="ts">
  /**
   * One instrument row in the Sound bank. Clicking the row **copies the voice
   * name** (ready to paste into `s("…")` / `.inst("…")`), with a transient
   * "copied" tick. An info toggle reveals an inline panel: the catalogue
   * description, the voice kind, and any named articulations.
   *
   * Nemus-local — composes shared/ui chrome + the tooltip action; the data shape
   * (`NemusInstrument`) is nemus-specific, so this lives under nemus/panels.
   */
  import { slide } from 'svelte/transition';
  import { Waves, Piano, Drum, Copy, Check, Info, Volume2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { animStore } from '$lib/stores/animations.svelte';
  import { previewStore } from '../stores/preview.svelte';
  import type { NemusInstrument } from '$lib/ipc/nemus';

  let { inst }: { inst: NemusInstrument } = $props();

  const kindLabel = $derived(
    inst.kind === 'synth' ? 'Synth preset'
    : inst.kind === 'sfz' ? 'Multisample · SFZ'
    : 'Sample one-shot',
  );
  const hasInfo = $derived(!!inst.description || inst.articulations.length > 0);

  let expanded = $state(false);
  let copied = $state(false);
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  async function copyName() {
    await copyToClipboard(inst.name);
    copied = true;
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { copied = false; }, 1400);
  }
</script>

<div class="sbi" class:expanded>
  <div class="sbi-row">
    <button class="sbi-main" onclick={copyName}
            use:tooltip={copied ? 'Copied!' : `Copy “${inst.name}”`}>
      <span class="sbi-icon">
        {#if inst.kind === 'synth'}<Waves size={13} />
        {:else if inst.kind === 'sfz'}<Piano size={13} />
        {:else}<Drum size={13} />{/if}
      </span>
      <span class="sbi-name">{inst.name}</span>
      <span class="sbi-copy" class:ok={copied}>
        {#if copied}<Check size={12} />{:else}<Copy size={12} />{/if}
      </span>
    </button>

    <button class="sbi-info-btn" aria-label={`Preview ${inst.name}`}
            use:tooltip={'Preview instrument'}
            onclick={() => previewStore.show(inst)}>
      <Volume2 size={13} />
    </button>

    {#if hasInfo}
      <button class="sbi-info-btn" class:on={expanded} aria-expanded={expanded}
              aria-label={`Info for ${inst.name}`} use:tooltip={'Show details'}
              onclick={() => { expanded = !expanded; }}>
        <Info size={13} />
      </button>
    {/if}
  </div>

  {#if expanded}
    <div class="sbi-info" transition:slide={{ duration: animStore.dFast }}>
      {#if inst.description}<p class="sbi-desc">{inst.description}</p>{/if}
      <div class="sbi-meta">
        <span class="sbi-kind">{kindLabel}</span>
        {#if inst.articulations.length}
          <span class="sbi-arts-label">articulations</span>
          {#each inst.articulations as a (a)}<span class="sbi-art">{a}</span>{/each}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  /* A sampler bank (drum machines, Dirt-Samples) can run to hundreds of rows;
     expanding the section would otherwise lay out + paint them all in one frame.
     `content-visibility: auto` lets WebView2 skip layout/paint for rows scrolled
     out of view, and `contain-intrinsic-size: auto …` remembers each row's real
     size after its first render — so the inline info-expand (variable height)
     stays accurate without a fixed-height windowing scheme. */
  .sbi {
    display: flex; flex-direction: column;
    content-visibility: auto;
    contain-intrinsic-size: auto 26px;
  }

  .sbi-row {
    display: flex; align-items: stretch; gap: 2px;
    border-radius: var(--radius-sm);
  }
  .sbi-row:hover { background: var(--bg-hover); }
  .sbi.expanded .sbi-row { background: var(--bg-elevated); }

  .sbi-main {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 8px;
    padding: 4px 8px 4px 6px;
    background: transparent; border: none; border-radius: var(--radius-sm);
    cursor: pointer; color: var(--text-primary); text-align: left;
  }
  .sbi-main:focus-visible { outline: none; box-shadow: inset 0 0 0 1px var(--accent); }

  .sbi-icon { display: flex; color: var(--text-muted); flex-shrink: 0; }
  .sbi-name {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: 11.5px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .sbi-copy { display: flex; color: var(--text-disabled); flex-shrink: 0; opacity: 0; transition: opacity var(--transition-fast); }
  .sbi-main:hover .sbi-copy, .sbi-main:focus-visible .sbi-copy { opacity: 1; }
  .sbi-copy.ok { opacity: 1; color: var(--success); }

  .sbi-info-btn {
    display: flex; align-items: center; justify-content: center;
    width: 26px; flex-shrink: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-disabled); cursor: pointer;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .sbi-info-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
  .sbi-info-btn.on { color: var(--accent); }

  .sbi-info { padding: 2px 10px 8px 26px; }
  .sbi-desc {
    margin: 0 0 5px; font-size: 11px; line-height: 1.5; color: var(--text-secondary);
  }
  .sbi-meta { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; }
  .sbi-kind {
    font-size: 9px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase;
    color: var(--text-muted); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    padding: 1px 5px;
  }
  .sbi-arts-label {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-disabled);
  }
  .sbi-art {
    font-family: var(--font-code); font-size: 9px; line-height: 1.5;
    padding: 0 5px; border-radius: var(--radius-sm);
    color: var(--text-secondary); background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
  }
</style>
