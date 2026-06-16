<script lang="ts">
  /**
   * Renders the nemus audio/MIDI import dialogs once per window, driven by the
   * `importActions` store. Mounted in NemusShell next to NemusProjectActions so
   * the waveform toolbar AND the command palette launch the same flow without
   * duplicating modal markup.
   */
  import FileExplorerModal from '$lib/components/shared/FileExplorerModal.svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { importActions, AUDIO_EXTS, MIDI_EXTS } from '../stores/import-actions.svelte';
  import { projectStore } from '../stores/project.svelte';
</script>

{#if importActions.picker === 'input'}
  <FileExplorerModal
    mode="file"
    title="Import audio or MIDI"
    extensions={[...AUDIO_EXTS, ...MIDI_EXTS]}
    initialPath={projectStore.project?.path}
    onConfirm={importActions.onInput}
    onCancel={importActions.cancel}
    onClose={importActions.cancel}
  />
{:else if importActions.picker === 'midi-out'}
  <FileExplorerModal
    mode="save"
    title="Convert to MIDI — save as"
    extensions={['mid']}
    initialFilename={importActions.midiOutName()}
    initialPath={projectStore.project?.path}
    onConfirm={importActions.onMidiOut}
    onCancel={importActions.cancel}
    onClose={importActions.cancel}
  />
{/if}

{#if importActions.choiceFor}
  <Modal onClose={importActions.cancel} size="sm" ariaLabel="Import audio">
    {#snippet header()}
      <ModalHeader title="Import audio" onClose={importActions.cancel} />
    {/snippet}

    <p class="lead">How should this audio be brought in?</p>
    <ul class="opts">
      <li><strong>Import as .nemus</strong> — transcribe it and open an editable
        <code>.nemus</code> file (the intermediate MIDI stays in memory).</li>
      <li><strong>Convert to MIDI file</strong> — transcribe it and save a
        <code>.mid</code> on disk.</li>
    </ul>

    {#snippet footer()}
      <ModalFooter align="between">
        <Button variant="ghost" onclick={importActions.cancel}>Cancel</Button>
        <div style="display:flex; gap:8px">
          <Button variant="secondary" onclick={importActions.chooseConvert}>Convert to MIDI file</Button>
          <Button variant="primary" onclick={importActions.chooseImport}>Import as .nemus</Button>
        </div>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .lead { margin: 0 0 10px; font-size: 13px; color: var(--text-primary); }
  .opts { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 8px; }
  .opts li { font-size: 12px; color: var(--text-secondary); line-height: 1.45; }
  .opts strong { color: var(--text-primary); font-weight: 600; }
  .opts code { font-family: var(--font-code); font-size: 11px; color: var(--text-secondary); }
</style>
