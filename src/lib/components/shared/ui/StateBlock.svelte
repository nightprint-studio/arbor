<!--
  StateBlock — centered block-level status message used inside content panes.

  Pattern: a tall card area needs to communicate "loading", "error", "empty",
  "ok" or "info" without a dedicated illustration. EmptyState is a thin inline
  pill for sparse lists; this one fills the parent vertically, centers an
  icon + label, and ships the four tone palettes that recur across studio
  panels, schema picker stubs, etc.

  Pass a `spinner` for the loading case (so the consumer keeps full control
  over the spinner's size/label), or `icon` for the others. Children override
  the default label when richer content is needed (links, retry buttons, …).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { AlertCircle, CheckCircle2, Info } from 'lucide-svelte';

  type Tone = 'loading' | 'error' | 'success' | 'info' | 'neutral';

  interface Props {
    tone?:      Tone;
    label?:     string;
    /** Override the default icon for non-loading tones. */
    icon?:      Snippet;
    /** Show this snippet instead of the default tone icon (e.g. a Spinner
     *  for the 'loading' tone — we don't bundle one to keep this component
     *  free of layout-flicker on the consumer's chosen size). */
    spinner?:   Snippet;
    /** Replace the label with arbitrary content (richer messages, retry CTA). */
    children?:  Snippet;
    /** Stretch to fill the parent vertically (default). Set false for a
     *  compact inline block. */
    fill?:      boolean;
  }

  let { tone = 'neutral', label, icon, spinner, children, fill = true }: Props = $props();

  const DefaultIcon = $derived(
    tone === 'error'   ? AlertCircle :
    tone === 'success' ? CheckCircle2 :
    tone === 'info'    ? Info :
                         null,
  );
</script>

<div class="state-block tone-{tone}" class:fill role={tone === 'error' ? 'alert' : 'status'}>
  {#if tone === 'loading' && spinner}
    {@render spinner()}
  {:else if icon}
    {@render icon()}
  {:else if DefaultIcon}
    <DefaultIcon size={16} />
  {/if}
  {#if children}
    {@render children()}
  {:else if label}
    <span>{label}</span>
  {/if}
</div>

<style>
  .state-block {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 24px;
    font-size: var(--font-size-sm);
    color: var(--text-muted);
  }
  .state-block.fill { height: 100%; }

  .tone-error   { color: var(--error); }
  .tone-success { color: var(--success); }
  .tone-info    { color: var(--info); }
  .tone-loading,
  .tone-neutral { color: var(--text-muted); }
</style>
