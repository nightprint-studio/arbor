<script lang="ts">
  /**
   * BennuFormRow — one JSP `<form>` header in the Forms tool window.
   *
   * Shows the action reference (or "no action"), the HTTP method as a small pill, and
   * — when resolved — the action class simple name (full FQCN in a tooltip). A leading
   * chevron toggles the field list; the rest of the header jumps the editor to the
   * `<form>` open tag. When the action resolves to a config fragment, a secondary
   * button opens that fragment.
   *
   * The chevron + open-config are separate real buttons INSIDE the row wrapper (not the
   * header button) to avoid nested `<button>`s; the header itself is the jump button. All
   * three are in natural tab order so the tree is keyboard-traversable via Tab.
   *
   * Presentational — imports only shared/ui + the forms IPC type.
   */
  import { ChevronRight, FileCog, CornerLeftUp } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { FormInfo } from '$lib/ipc/bennu/forms';

  let {
    form,
    open = true,
    focusFile,
    onjump,
    ontoggle,
    onopenconfig,
  }: {
    form: FormInfo;
    /** Whether the field list under this form is expanded. */
    open?: boolean;
    /** The file the panel is analysing — when it differs from the form's `host_file`, the form
     *  lives on another page (that includes the open fragment) and we show a host chip. */
    focusFile?: string | null;
    /** Jump the editor to the `<form>` open tag (opens `host_file` first if cross-file). */
    onjump: () => void;
    /** Toggle the field list. */
    ontoggle: () => void;
    /** Open the config fragment the `<action>` is declared in. */
    onopenconfig?: () => void;
  } = $props();

  /** The simple class name from an FQCN (for the compact chip; FQCN goes in a tooltip). */
  const simpleClass = $derived(
    form.action_class ? (form.action_class.split('.').pop() ?? form.action_class) : null,
  );
  const method = $derived(form.method ? form.method.toUpperCase() : null);

  /** The form lives on a DIFFERENT page than the one being analysed (an ancestor that includes
   *  the open fragment) — surface which page, so the user knows where the `<form>` really is. */
  const remoteHost = $derived(
    focusFile && form.host_file && form.host_file !== focusFile
      ? (form.host_file.split('/').pop() ?? form.host_file)
      : null,
  );
</script>

<div class="fr-row">
  <button
    type="button"
    class="fr-chevron"
    class:open
    onclick={ontoggle}
    aria-label={open ? 'Collapse form' : 'Expand form'}
    aria-expanded={open}
  >
    <ChevronRight size={12} />
  </button>

  <button
    type="button"
    class="fr-header"
    onclick={onjump}
  >
    <span class="fr-action" class:muted={!form.action}>
      {form.action ?? 'no action'}
    </span>
    {#if method}
      <Badge variant="tone" size="sm" tone="accent" label={method} />
    {/if}
    {#if simpleClass}
      <span class="fr-class" use:tooltip={form.action_class ?? undefined}>{simpleClass}</span>
    {/if}
    {#if remoteHost}
      <span class="fr-host" use:tooltip={`Declared on ${form.host_file}`}>
        <CornerLeftUp size={11} />
        <span class="fr-host-name">{remoteHost}</span>
      </span>
    {/if}
  </button>

  {#if form.config_file && onopenconfig}
    <button
      type="button"
      class="fr-config"
      onclick={onopenconfig}
      use:tooltip={'Open the action config fragment'}
      aria-label="Open action config"
    >
      <FileCog size={13} />
    </button>
  {/if}
</div>

<style>
  .fr-row {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-right: 6px;
    border-left: 2px solid transparent;
  }
  .fr-row:hover { background: var(--bg-hover); }
  .fr-row:focus-within { background: var(--bg-selected, var(--accent-subtle)); border-left-color: var(--accent); }

  .fr-chevron {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 26px; flex-shrink: 0;
    background: transparent; border: none; cursor: pointer;
    color: var(--text-muted);
  }
  .fr-chevron :global(svg) { transition: transform var(--transition-fast); }
  .fr-chevron.open :global(svg) { transform: rotate(90deg); }
  .fr-chevron:hover { color: var(--text-primary); }

  .fr-header {
    display: flex; align-items: center; gap: 6px;
    flex: 1; min-width: 0;
    height: 26px; padding: 0 4px;
    background: transparent; border: none;
    text-align: left; cursor: pointer;
  }
  .fr-action {
    min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-primary);
  }
  .fr-action.muted { color: var(--text-muted); font-style: italic; font-family: var(--font-ui-sans); }

  .fr-class {
    flex-shrink: 0;
    max-width: 45%;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-size: 10.5px;
    color: var(--text-muted);
  }

  /* The page the `<form>` is declared on, when analysing an included fragment. */
  .fr-host {
    display: inline-flex; align-items: center; gap: 3px; flex-shrink: 0;
    max-width: 40%;
    padding: 0 5px; height: 16px;
    border-radius: var(--radius-sm);
    color: var(--info, #61afef);
    background: color-mix(in srgb, var(--info, #61afef) 12%, transparent);
    font-size: 10px;
  }
  .fr-host-name {
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code);
  }

  .fr-config {
    display: flex; align-items: center; justify-content: center;
    width: 24px; height: 24px; flex-shrink: 0;
    background: transparent; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .fr-config:hover { background: var(--bg-hover); color: var(--text-primary); }
</style>
