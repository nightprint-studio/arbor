<script lang="ts">
  /**
   * Renders the merula project/file pickers once per window, driven by the
   * `projectActions` store. Mounted in MerulaShell so the hamburger menu, the
   * titlebar, AND the keyboard shortcuts can launch the same flows without any
   * of them duplicating `FileExplorerModal` markup.
   */
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
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
    title={projectActions.picker === 'new' ? 'New merula project — pick a folder' : 'Open merula project'}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'new-file'}
  <FileExplorerModal
    mode="save"
    title="New .merula file"
    extensions={['merula']}
    initialFilename="untitled.merula"
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'open-file'}
  <FileExplorerModal
    mode="file"
    title="Open .merula file"
    extensions={['merula']}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{:else if projectActions.picker === 'export'}
  <FileExplorerModal
    mode="save"
    title={`Export to ${projectActions.exportFormat.toUpperCase()}`}
    extensions={[projectActions.exportFormat]}
    initialFilename={`${projectStore.project?.name ?? 'merula'}.${projectActions.exportFormat}`}
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
    initialFilename={`${projectStore.project?.name ?? 'merula'}-region.${projectActions.exportFormat}`}
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
    initialFilename={`${projectStore.project?.name ?? 'merula'}.mid`}
    initialPath={projectStore.project?.path}
    onConfirm={projectActions.onConfirm}
    onCancel={projectActions.cancel}
    onClose={projectActions.cancel}
  />
{/if}
