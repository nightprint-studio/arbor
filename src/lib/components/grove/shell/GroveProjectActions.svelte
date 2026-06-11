<script lang="ts">
  /**
   * Renders the grove project/file pickers once per window, driven by the
   * `projectActions` store. Mounted in GroveShell so the hamburger menu, the
   * titlebar, AND the keyboard shortcuts can launch the same flows without any
   * of them duplicating `FileExplorerModal` markup.
   */
  import FileExplorerModal from '$lib/components/shared/FileExplorerModal.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { projectStore } from '../stores/project.svelte';
</script>

{#if projectActions.picker === 'new' || projectActions.picker === 'open-project'}
  <FileExplorerModal
    mode="folder"
    title={projectActions.picker === 'new' ? 'New grove project — pick a folder' : 'Open grove project'}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'new-file'}
  <FileExplorerModal
    mode="save"
    title="New .grove file"
    extensions={['grove']}
    initialFilename="untitled.grove"
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'open-file'}
  <FileExplorerModal
    mode="file"
    title="Open .grove file"
    extensions={['grove']}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export'}
  <FileExplorerModal
    mode="save"
    title="Export to WAV"
    extensions={['wav']}
    initialFilename={`${projectStore.project?.name ?? 'grove'}.wav`}
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{/if}
