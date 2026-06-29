<!--
  FormNodeAlert — banner/inline alert wrapper with local dismiss + collapse
  state. Extracted from FormNodeLayout so each alert instance owns its own
  `dismissed` / `isCollapsed` flags (a top-level `$state` in FormNodeLayout
  would be shared across every alert in the form).

  Dismiss and collapse are PURELY local: no plugin round-trip. A plugin that
  needs to track dismissal can re-render the node via `arbor.ui.form.patch`
  to bring it back, or simply omit `dismissable` and drive visibility itself.
-->
<script lang="ts">
  import { untrack } from 'svelte';
  import { X, ChevronDown } from 'lucide-svelte';
  import Alert   from '$lib/components/shared/ui/Alert.svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';

  type Variant = 'info' | 'warning' | 'error' | 'success';
  type Style   = 'banner' | 'inline';

  interface Props {
    title?:       string;
    text?:        string;
    variant?:     Variant;
    style?:       Style;
    dismissable?: boolean;
    collapsible?: boolean;
    collapsed?:   boolean;
    /** Pass-through wrapper class from the FormNode. */
    class?:       string;
  }

  let {
    title,
    text = '',
    variant = 'info',
    style = 'banner',
    dismissable = false,
    collapsible = false,
    collapsed = false,
    class: klass,
  }: Props = $props();

  let dismissed   = $state(false);
  let isCollapsed = $state(untrack(() => collapsed));

  const calloutVariant = $derived(
    variant === 'error'   ? 'danger' :
    variant === 'success' ? 'tip'    :
    variant
  );
  // Hide the body text while collapsed; the underlying widget still
  // renders the title row so the user has something to click on.
  const bodyText  = $derived(collapsible && isCollapsed ? '' : text);
  const showChrome = $derived(dismissable || collapsible);
</script>

{#if !dismissed}
  <div class="pf-alert-wrap {klass ?? ''}" class:has-chrome={showChrome}>
    {#if style === 'inline'}
      <Callout variant={calloutVariant} {title}>{bodyText}</Callout>
    {:else}
      <Alert {variant} {title} text={bodyText} />
    {/if}
    {#if showChrome}
      <div class="pf-alert-chrome">
        {#if collapsible}
          <button
            type="button"
            class="pf-alert-chip"
            class:rot={isCollapsed}
            aria-label={isCollapsed ? 'Expand' : 'Collapse'}
            aria-expanded={!isCollapsed}
            onclick={() => (isCollapsed = !isCollapsed)}
          ><ChevronDown size={13} /></button>
        {/if}
        {#if dismissable}
          <button
            type="button"
            class="pf-alert-chip"
            aria-label="Dismiss"
            onclick={() => (dismissed = true)}
          ><X size={13} /></button>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .pf-alert-wrap { position: relative; }
  /* Reserve room under the absolutely-positioned chrome cluster so the
     alert body text doesn't slide under the buttons. */
  .pf-alert-wrap.has-chrome :global(.alert),
  .pf-alert-wrap.has-chrome :global(.callout) { padding-right: 40px; }

  .pf-alert-chrome {
    position: absolute;
    top: 6px;
    right: 6px;
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .pf-alert-chip {
    background: transparent;
    border: none;
    color: inherit;
    cursor: pointer;
    padding: 2px;
    border-radius: var(--radius-sm);
    opacity: 0.65;
    transition: opacity var(--transition-fast), background var(--transition-fast), transform 120ms;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .pf-alert-chip:hover { opacity: 1; background: rgba(255, 255, 255, 0.06); }
  .pf-alert-chip.rot { transform: rotate(-90deg); }
</style>
