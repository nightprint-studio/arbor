<script lang="ts">
  /**
   * Bennu Run Configuration — the minimal run target for `java -cp target/classes:deps
   * <mainClass>`.
   *
   * Main-class discovery isn't a thing yet (a later wave), so ▶ Run needs the fully
   * qualified main class typed once. It's remembered per project in {@link bennuRunStore}
   * so subsequent ▶ Run reuses it without reopening this. Running here builds first
   * (the store gates run on a clean compile) and streams to the Build dock.
   *
   * Keyboard-first: the field auto-focuses (first control, via <Modal>), Enter or
   * Ctrl/Cmd+Enter runs, Esc cancels (handled by <Modal>).
   */
  import { Play } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuRunStore } from '$lib/stores/bennu/run.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);

  // Seed from the remembered class for this project.
  let mainClass = $state(root ? (bennuRunStore.mainClassFor(root) ?? '') : '');
  const canRun = $derived(!!root && mainClass.trim().length > 0 && !bennuRunStore.active);

  function launch() {
    if (!root || !canRun) return;
    const cls = mainClass.trim();
    onClose();
    void bennuRunStore.run(root, cls);
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' || ((e.ctrlKey || e.metaKey) && e.key === 'Enter')) {
      e.preventDefault();
      launch();
    }
  }
</script>

<Modal {onClose} width="560px" height="300px" ariaLabel="Bennu Run Configuration">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Play size={14} />
      <span class="modal-title">Run Configuration</span>
      {#if project}<span class="hdr-name">{project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    {#if !project}
      <EmptyState message="Open a project to configure a run." />
    {:else}
      <FormField
        label="Main class"
        hint="Fully qualified class with a public static void main. Bennu compiles the project, then runs it with the project classpath."
      >
        <Input bind:value={mainClass} placeholder="com.example.Main" />
      </FormField>
      <p class="cmd-preview">
        <span class="cmd">java -cp target/classes:&lt;deps&gt; {mainClass.trim() || 'com.example.Main'}</span>
      </p>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="secondary" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        onclick={launch}
        disabled={!canRun}
        tooltip={{ content: 'Build & run', shortcut: 'Enter' }}
      >
        Run
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .hdr-name { font-size: 11px; color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .body { display: flex; flex-direction: column; gap: 12px; }
  .cmd-preview { margin: 0; }
  .cmd {
    font-family: var(--font-code); font-size: 11px; color: var(--text-muted);
    background: var(--bg-elevated); border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm); padding: 6px 9px; display: block;
    overflow-x: auto; white-space: nowrap;
  }
</style>
