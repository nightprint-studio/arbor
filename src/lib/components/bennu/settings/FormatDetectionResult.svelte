<script lang="ts">
  /**
   * What «Detect from project…» found: each measured habit and how consistently the code follows it,
   * with the choice of storing the lot as the project's style or dropping it.
   */
  import { ScanSearch } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import PropertyList, { type PropertyRow } from '$lib/components/shared/ui/PropertyList.svelte';
  import { describeValue, optionFor } from '../format-options';
  import type { FormatDetection } from '$lib/types/bennu-format';

  interface Props {
    detection: FormatDetection;
    onapply: () => void;
    ondismiss: () => void;
  }

  let { detection, onapply, ondismiss }: Props = $props();

  const rows = $derived<PropertyRow[]>(
    detection.evidence.map((e) => ({
      label: optionFor(e.key)?.label ?? e.key,
      value: `${describeValue(e.key, e.value)} — ${Math.round(e.share * 100)}% of ${e.samples}`,
    })),
  );
</script>

<div class="card">
  <div class="card-section-title"><ScanSearch size={12} /> Detected from {detection.files} {detection.files === 1 ? 'file' : 'files'}</div>
  {#if rows.length}
    <PropertyList {rows} />
  {:else}
    <p class="set-hint">Nothing in this project’s code points one way strongly enough to call it a style.</p>
  {/if}
  <div class="fd-actions">
    <Button variant="ghost" size="sm" onclick={ondismiss}>Dismiss</Button>
    <Button variant="primary" size="sm" disabled={!rows.length} onclick={onapply}>Apply as project style</Button>
  </div>
</div>

<style>
  .fd-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
</style>
