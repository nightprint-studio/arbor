<script lang="ts">
  /**
   * Renders the nemus project/file pickers once per window, driven by the
   * `projectActions` store. Mounted in NemusShell so the hamburger menu, the
   * titlebar, AND the keyboard shortcuts can launch the same flows without any
   * of them duplicating `FileExplorerModal` markup.
   */
  import FileExplorerModal from '$lib/components/shared/FileExplorerModal.svelte';
  import ExportOptionsModal from './ExportOptionsModal.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { projectStore } from '../stores/project.svelte';
</script>

{#if projectActions.exportOptionsOpen}
  <ExportOptionsModal />
{/if}

{#if projectActions.picker === 'new' || projectActions.picker === 'open-project'}
  <FileExplorerModal
    mode="folder"
    title={projectActions.picker === 'new' ? 'New nemus project — pick a folder' : 'Open nemus project'}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'new-file'}
  <FileExplorerModal
    mode="save"
    title="New .nemus file"
    extensions={['nemus']}
    initialFilename="untitled.nemus"
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'open-file'}
  <FileExplorerModal
    mode="file"
    title="Open .nemus file"
    extensions={['nemus']}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export'}
  <FileExplorerModal
    mode="save"
    title={`Export to ${projectActions.exportFormat.toUpperCase()}`}
    extensions={[projectActions.exportFormat]}
    initialFilename={`${projectStore.project?.name ?? 'nemus'}.${projectActions.exportFormat}`}
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export-region'}
  <FileExplorerModal
    mode="save"
    title={`Export region to ${projectActions.exportFormat.toUpperCase()}`}
    extensions={[projectActions.exportFormat]}
    initialFilename={`${projectStore.project?.name ?? 'nemus'}-region.${projectActions.exportFormat}`}
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export-stems'}
  <FileExplorerModal
    mode="folder"
    title="Export stems — pick a folder"
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export-midi'}
  <FileExplorerModal
    mode="save"
    title="Export to MIDI"
    extensions={['mid']}
    initialFilename={`${projectStore.project?.name ?? 'nemus'}.mid`}
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{/if}
