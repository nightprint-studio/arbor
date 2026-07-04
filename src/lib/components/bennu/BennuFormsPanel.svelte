<script lang="ts">
  /**
   * BennuFormsPanel — the Forms tool window (right rail, active-file-scoped like Structure).
   *
   * Shows every `<form>` RELEVANT to the open JSP with its COMPLETE parameter set — not a tree
   * of files. A legacy form is split across `<jsp:include>`s, so each form here lists its own
   * inputs PLUS every `<input>`/`<select>`/hidden a fragment inside it contributes, aggregated
   * across the include graph both ways:
   *
   *   - on a parent page → the form shows its children's parameters too;
   *   - on an included fragment → the parent form it feeds surfaces (a host chip names the page),
   *     with the whole parameter set — the fragment's own fields highlighted as "yours".
   *
   * Each field is correlated against the resolved action class (bound / validated badges) and,
   * when pulled in from an include, tagged with its source fragment. Clicking a form/field opens
   * its (possibly other) file THEN jumps the editor there. Fetches are race-guarded; a header
   * Refresh re-runs. Imports only shared/ui + bennu stores + the forms IPC + the two row parts.
   */
  import { TextCursorInput, RefreshCw } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { formAnalysis, type FormAnalysis } from '$lib/ipc/bennu/forms';
  import BennuFormRow from './BennuFormRow.svelte';
  import BennuFormFieldRow from './BennuFormFieldRow.svelte';

  let {
    /** When true (in the bottom dock) the panel renders headerless — the dock's tab strip is
     *  the identity and owns the Refresh action (via the exported {@link refresh}). */
    hideHeader = false,
  }: { hideHeader?: boolean } = $props();

  const JSP_EXT = /\.(jsp|jspf|tag|tagx)$/i;

  const activePath = $derived(projectStore.activeFilePath);
  const isJsp = $derived(!!activePath && JSP_EXT.test(activePath));

  /** Whether a Refresh is currently possible — the dock header mirrors the inline button. */
  export function canRefresh(): boolean { return isJsp && !loading; }

  let analysis = $state<FormAnalysis | null>(null);
  let loading = $state(false);

  /** Per-form collapse state, keyed by form index. */
  let collapsed = $state<Set<number>>(new Set());
  function isOpen(i: number): boolean { return !collapsed.has(i); }
  function toggle(i: number) {
    const next = new Set(collapsed);
    if (next.has(i)) next.delete(i); else next.add(i);
    collapsed = next;
  }

  /** Fetch the include-aware form analysis for `path`, dropping the result if the active file
   *  changed while in flight (the race guard). `full` forces a full include-graph re-walk (the
   *  Refresh button); the reactive per-tab fetch stays incremental. */
  async function load(path: string, full = false) {
    loading = true;
    try {
      const res = await formAnalysis(path, full);
      if (projectStore.activeFilePath !== path) return; // stale → drop
      analysis = res;
      collapsed = new Set();
    } catch {
      if (projectStore.activeFilePath === path) analysis = null;
    } finally {
      if (projectStore.activeFilePath === path) loading = false;
    }
  }

  // Re-run when the active file changes; non-JSP / no file → clear.
  $effect(() => {
    const path = activePath;
    if (!path || !isJsp) { analysis = null; loading = false; return; }
    void load(path);
  });

  export function refresh() {
    // The manual Refresh forces a full include-graph re-walk (catch new files / parent includes).
    if (activePath && isJsp) void load(activePath, true);
  }

  const forms = $derived(analysis?.forms ?? []);
  const isEmpty = $derived(forms.length === 0);

  function openFile(file: string) {
    void projectStore.openFile(file);
  }

  /** Open the (possibly cross-file) target JSP, THEN ask the editor to jump. The goto relay
   *  bumps a nonce the editor reacts to on render, so ordering it after the open→active swap is
   *  what makes a jump into a fragment other than the active file land correctly. */
  async function jump(file: string, offset: number) {
    if (file !== projectStore.activeFilePath) {
      await projectStore.openFile(file);
    }
    bennuUiStore.requestGotoOffset(offset);
  }
</script>

{#snippet body()}
  {#if !isJsp}
    <EmptyState message="Open a JSP to see its forms." />
  {:else if isEmpty}
    <EmptyState message={loading ? 'Analysing forms…' : 'No form here — this page posts no parameters and none of its includes feed one.'} />
  {:else}
    <div class="ff-list" aria-label="Forms and their parameters">
      {#each forms as form, fi (form.host_file + ':' + form.start)}
        <BennuFormRow
          {form}
          open={isOpen(fi)}
          focusFile={activePath}
          onjump={() => void jump(form.host_file, form.start)}
          ontoggle={() => toggle(fi)}
          onopenconfig={form.config_file ? () => openFile(form.config_file!) : undefined}
        />
        {#if isOpen(fi)}
          {#if form.fields.length === 0}
            <p class="ff-none">No input parameters.</p>
          {/if}
          {#each form.fields as field, xi (field.source_file + ':' + field.start + ':' + xi)}
            <BennuFormFieldRow
              {field}
              hostFile={form.host_file}
              focusFile={activePath}
              onjump={() => void jump(field.source_file, field.start)}
            />
          {/each}
        {/if}
      {/each}

      {#if analysis?.truncated}
        <p class="ff-more" use:tooltip={'The include graph is large; some related pages were not walked'}>
          …more (graph truncated)
        </p>
      {/if}
    </div>
  {/if}
{/snippet}

{#if hideHeader}
  <!-- Bottom-dock mode: the dock tab strip is the header; the Refresh action lives there. -->
  <div class="ff-dock">{@render body()}</div>
{:else}
  <PanelShell title="Forms">
    {#snippet icon()}<TextCursorInput size={13} />{/snippet}
    {#snippet actions()}
      <button
        class="ps-btn"
        type="button"
        onclick={refresh}
        disabled={!isJsp || loading}
        use:tooltip={'Refresh'}
        aria-label="Refresh forms"
      >
        <RefreshCw size={14} />
      </button>
    {/snippet}
    {@render body()}
  </PanelShell>
{/if}

<style>
  /* Bottom-dock mode: own the full dock body height and scroll the form list. */
  .ff-dock {
    flex: 1; min-height: 0;
    height: 100%;
    overflow-y: auto;
  }
  .ff-list {
    padding: 4px 0 6px;
    display: flex;
    flex-direction: column;
  }
  .ff-none {
    margin: 0;
    padding: 2px 10px 4px 30px;
    font-size: 11px;
    font-style: italic;
    color: var(--text-muted);
  }
  .ff-more {
    margin: 4px 0 0;
    padding: 2px 10px;
    font-size: 11px;
    font-style: italic;
    color: var(--text-muted);
  }
</style>
