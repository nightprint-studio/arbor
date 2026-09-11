<script lang="ts">
  /**
   * What a generation would write, before anything is written — and the things worth a look first:
   * whether the expectations came from the JVM, and every warning the generation raised.
   */
  import { ShieldAlert, ShieldCheck, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { languageForPath } from '../languages';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';

  const JAVA = languageForPath('Preview.java');

  let whole = $state(false);

  const preview = $derived(lab.preview);
  const shownText = $derived(
    !preview ? '' : preview.exists && !whole ? preview.inserted.replace(/^\n+/, '') : preview.text,
  );
  const classOptions = $derived((preview?.classes ?? []).map((c) => ({ value: c.path, label: c.path })));
  const targetClass = $derived(lab.target?.class ?? preview?.classes[0]?.path ?? '');

  function close() {
    lab.closePreview();
  }

  function chooseClass(path: string) {
    if (!preview) return;
    lab.setTarget({ file: preview.file, class: path });
    void lab.generate();
  }

  function relative(path: string): string {
    const root = projectStore.project?.root;
    return root && path.startsWith(root) ? path.slice(root.length).replace(/^[\\/]/, '') : path;
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void lab.apply();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<Modal onClose={close} width="860px" height="640px" padBody={false} ariaLabel="Generated tests">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <Wand2 size={14} />
      <span class="modal-title">{preview?.exists ? 'Add tests' : 'New test file'}</span>
      {#if preview}<span class="file" use:tooltip={preview.file}>{relative(preview.file)}</span>{/if}
    </ModalHeader>
  {/snippet}

  {#if preview}
    <div class="body">
      <div class="summary">
        <span class="stat"><strong>{preview.cases}</strong> case{preview.cases === 1 ? '' : 's'}</span>
        <span class="stat">template <strong>{preview.template}</strong></span>
        {#if preview.verified}
          <span class="verified ok" use:tooltip={"Every expectation is what the project's own validator reported"}>
            <ShieldCheck size={12} /> Checked on the JVM
          </span>
        {:else}
          <span class="verified predicted" use:tooltip={'The JVM could not answer, so the expectations are predicted from the source'}>
            <ShieldAlert size={12} /> Predicted from the source
          </span>
        {/if}
        <span class="spacer"></span>
        {#if preview.exists && classOptions.length > 1}
          <span class="label">Add to</span>
          <div class="class-select">
            <Select
              value={targetClass}
              options={classOptions}
              size="sm"
              ariaLabel="Class to add the tests to"
              disabled={lab.generating}
              onchange={chooseClass}
            />
          </div>
        {/if}
        {#if preview.exists}
          <Button variant="ghost" size="xs" onclick={() => (whole = !whole)}>
            {whole ? 'Show what is added' : 'Show the whole file'}
          </Button>
        {/if}
      </div>
      {#if preview.warnings.length > 0}
        <div class="warnings">
          {#each preview.warnings as warning, i (i)}
            <Alert variant="warning" compact text={warning} />
          {/each}
        </div>
      {/if}
      <div class="code">
        {#key shownText}<CodeEditor value={shownText} language={JAVA} readOnly />{/key}
      </div>
    </div>
  {/if}

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">Nothing is written until you apply — Ctrl+Enter</span>
      <div class="buttons">
        <Button variant="ghost" onclick={close}>Cancel</Button>
        <Button variant="primary" loading={lab.applying || lab.generating} onclick={() => void lab.apply()}>
          {preview?.exists ? 'Add the tests' : 'Create the file'}
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .file {
    margin-left: 8px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted);
  }
  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .summary {
    display: flex; align-items: center; gap: 12px; flex-shrink: 0;
    padding: 8px 14px; border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .stat strong { color: var(--text-primary); font-weight: 600; }
  .verified { display: inline-flex; align-items: center; gap: 4px; padding: 1px 7px; border-radius: 999px; font-weight: 600; }
  .verified.ok { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); }
  .verified.predicted { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .spacer { flex: 1; }
  .label { color: var(--text-muted); }
  .class-select { min-width: 200px; }
  .warnings { display: flex; flex-direction: column; gap: 4px; padding: 8px 14px 0; max-height: 140px; overflow-y: auto; flex-shrink: 0; }
  .code { flex: 1; min-height: 0; display: flex; flex-direction: column; margin-top: 8px; border-top: 1px solid var(--border-subtle); }
  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .buttons { display: flex; gap: 8px; }
</style>
