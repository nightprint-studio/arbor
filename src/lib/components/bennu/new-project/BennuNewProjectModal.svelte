<script lang="ts">
  /**
   * BennuNewProjectModal — the New Project wizard: project types on the left, the chosen type's
   * form on the right, a live "what will be created" panel, and Create.
   *
   * Shaped for the common case to be three gestures — pick a type, type a name, Ctrl+Enter —
   * because everything else (location, Git, options) is remembered or defaulted:
   *
   * - **Ctrl+Enter** creates from anywhere; **Enter** creates from the name and location fields.
   * - **Ctrl+↑ / Ctrl+↓** switch the project type without leaving the form; in the type list the
   *   plain arrows do it, and Enter or → moves into the form.
   * - **Esc** closes. While a run is going it cancels it and removes what it made.
   *
   * The wizard itself is the widgets of `shared/ui` composed over a generic description of each
   * form (see `NewProjectForm`); it knows no generator by name.
   */
  import { onDestroy, onMount } from 'svelte';
  import { FolderPlus } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { newProjectStore as store } from '$lib/stores/bennu/new-project.svelte';
  import { neighbour } from './new-project-helpers';
  import NewProjectGenerators from './NewProjectGenerators.svelte';
  import NewProjectForm from './NewProjectForm.svelte';
  import NewProjectProgress from './NewProjectProgress.svelte';

  let { onClose }: { onClose: () => void } = $props();

  /** The tools worth a glance in the footer; pnpm and Yarn show up in the package-manager choice. */
  const SHOWN_TOOLS = ['git', 'node', 'npm', 'cargo', 'java', 'mvn'];

  const catalog = $derived(store.catalog);
  const tools = $derived((catalog?.tools ?? []).filter((t) => SHOWN_TOOLS.includes(t.name)));
  const ordered = $derived(
    catalog ? catalog.categories.flatMap((c) => catalog.generators.filter((g) => g.category === c.id)) : [],
  );

  onMount(() => {
    void store.init();
  });
  onDestroy(() => {
    void store.dispose();
  });

  function close() {
    onClose();
  }

  function focusForm() {
    document.querySelector<HTMLElement>('[data-np-form] input, [data-np-form] button')?.focus();
  }

  function onKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key === 'Enter') {
      e.preventDefault();
      void store.create();
    } else if (mod && (e.key === 'ArrowDown' || e.key === 'ArrowUp') && store.phase === 'form') {
      e.preventDefault();
      const next = neighbour(ordered, store.generatorId, e.key === 'ArrowDown' ? 1 : -1);
      if (next) void store.selectGenerator(next.id);
    }
  }
</script>

<Modal onClose={close} width="min(95vw, 1080px)" height="min(90vh, 760px)" padBody={false} ariaLabel="New Project">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <FolderPlus size={14} />
      <span class="modal-title">New Project</span>
    </ModalHeader>
  {/snippet}

  <div class="np" onkeydown={onKey} role="presentation">
    <aside class="left">
      {#if catalog}
        <NewProjectGenerators
          {catalog}
          selected={store.generatorId}
          onselect={(id) => void store.selectGenerator(id)}
          onenter={focusForm}
        />
      {:else if store.catalogError}
        <div class="side-note"><Alert variant="error" compact text={store.catalogError} /></div>
      {:else}
        <div class="side-note"><Spinner size="sm" /> Loading…</div>
      {/if}
    </aside>

    <section class="right" data-np-form>
      {#if store.phase === 'form'}
        <NewProjectForm oncreate={() => void store.create()} />
      {:else}
        <NewProjectProgress />
      {/if}
    </section>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <div class="tools" aria-label="Installed tools">
        {#each tools as t (t.name)}
          <span use:tooltip={t.found ? (t.path ?? t.label) : `${t.label} was not found on your PATH`}>
            <Badge variant="tone" size="sm" tone={t.found ? 'success' : 'neutral'} dot label={t.name} />
          </span>
        {/each}
      </div>
      <div class="actions">
        {#if store.phase === 'running'}
          <Button variant="ghost" size="sm" onclick={() => void store.cancel()}>Cancel the run</Button>
        {:else}
          <Button variant="ghost" size="sm" onclick={close}>Cancel</Button>
          {#if store.phase === 'form'}
            <Button
              variant="primary"
              size="sm"
              disabled={!store.canCreate}
              tooltip={{ content: 'Create the project', shortcut: 'Ctrl+Enter' }}
              onclick={() => void store.create()}
            >Create</Button>
          {/if}
        {/if}
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .np { display: grid; grid-template-columns: 236px minmax(0, 1fr); height: 100%; min-height: 0; }
  .left { min-height: 0; overflow: hidden; background: var(--bg-elevated); border-right: 1px solid var(--border-subtle); }
  .right { min-height: 0; min-width: 0; overflow-y: auto; }
  .side-note { display: flex; align-items: center; gap: 8px; padding: 14px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .tools { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; }
  .actions { display: flex; gap: 8px; }
</style>
