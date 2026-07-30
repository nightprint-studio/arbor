<script lang="ts">
  /**
   * Say what **one script** is — without a mouse, and without touching the
   * folder around it.
   *
   * The sibling of `ClassifyFolderModal`, one level down and deliberately
   * narrower. It exists for the repositories where the folder cannot answer:
   * `4_12_ORA.sql` beside `4_12_POS.sql` in one directory, which a folder
   * declaration could only describe by lying about half its contents.
   *
   * ## Why there is no Role here
   *
   * A role is what a *directory of scripts* is for, and the file beside another
   * in the same directory is for the same thing. The engine is the only axis
   * that genuinely varies file by file, so it is the only one offered — a role
   * control here would be a control that is always wrong to use.
   *
   * ## "Inherit from the folder" is the normal answer
   *
   * Almost every file in almost every repository should inherit, and the dialog
   * says so rather than implying that a blank row is unfinished business. Setting
   * an engine here is the exception, and the standing sentence names what the
   * file would inherit *if* it were cleared, so the exception can be undone
   * knowingly.
   */
  import { Check, FileCog, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import ClassifyPicker from './ClassifyPicker.svelte';
  import PicusDialectChip from './PicusDialectChip.svelte';
  import { CLEAR_ID, engineFromChoice, engineSelectOptions } from './engine-choices';
  import { classifyFile } from './file-classify';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import {
    declaredFileEngine,
    engineLabel,
    fileEngine,
    fileEngineIsUnknown,
    folderEngine,
  } from '$lib/types/picus';

  let { path = '', onClose }: { path?: string; onClose: () => void } = $props();

  // The dialog can be opened on a file, so the selector has to start on what
  // that file declares — Apply writes the declaration, not the effective answer.
  // svelte-ignore state_referenced_locally
  const opened = picusProjectStore.fileByPath(path);

  let query = $state('');
  // svelte-ignore state_referenced_locally
  let selectedPath = $state(path);
  let engine = $state<string>(opened ? declaredFileEngine(opened) ?? CLEAR_ID : CLEAR_ID);

  const needle = $derived(query.trim().toLowerCase());
  const visible = $derived(
    picusProjectStore.allFiles.filter((f) => !needle || f.path.toLowerCase().includes(needle)),
  );
  const rows = $derived(visible.map((f) => ({ id: f.path })));
  const selected = $derived(picusProjectStore.fileByPath(selectedPath));
  const selectedFolder = $derived(selected ? picusProjectStore.folderOfFile(selected.path) : null);

  const engineOptions = engineSelectOptions('Inherit from the folder');

  /** Where this file currently stands, in words — the sentence Apply changes. */
  const standing = $derived.by(() => {
    const file = selected;
    if (!file) return '';
    const folder = selectedFolder;
    const declared = declaredFileEngine(file);
    const inherited = folder ? folderEngine(folder.node) : null;
    const wouldInherit = inherited
      ? `Cleared, it would inherit ${engineLabel(inherited)} from ${folder?.node.path}.`
      : 'Cleared, it would have no engine: nothing above it declares one either.';

    if (declared) {
      return `Today: ${engineLabel(declared)}, declared on this file. ${wouldInherit}`;
    }
    const effective = fileEngine(file);
    if (!effective) {
      return 'Today: no engine — nothing is generated into it and nothing about it is '
        + 'compared. Its folder says nothing either, so classifying the folder fixes every '
        + 'script in it at once.';
    }
    return `Today: ${engineLabel(effective)}, inherited from ${folder?.node.path}. `
      + 'Setting one here overrides that, for this file only.';
  });

  function pick(filePath: string) {
    selectedPath = filePath;
    const file = picusProjectStore.fileByPath(filePath);
    engine = file ? declaredFileEngine(file) ?? CLEAR_ID : CLEAR_ID;
  }

  let applying = $state(false);

  async function apply() {
    const file = selected;
    if (!file || applying) return;
    applying = true;
    const ok = await classifyFile(file, engineFromChoice(engine));
    applying = false;
    if (ok) onClose();
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void apply(); }
  }
</script>

<Modal {onClose} width="680px" height="560px" padBody={false} ariaLabel="Classify a script">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FileCog size={14} />
      <span class="modal-title">Classify a script</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="cf" role="group" onkeydown={onKeydown}>
    <ClassifyPicker
      {rows}
      bind:query
      selectedId={selectedPath}
      onPick={pick}
      onSubmit={() => void apply()}
      placeholder="Filter scripts by path…"
      ariaLabel="Filter scripts"
    >
      {#snippet row(item)}
        {@const file = picusProjectStore.fileByPath(item.id)}
        {#if file}
          {#if fileEngineIsUnknown(file)}
            <!-- The one state this dialog exists to end: a script no engine
                 covers, so nothing is generated into it and nothing about it is
                 compared. -->
            <span class="cf-warn"><TriangleAlert size={12} /></span>
          {:else if selectedPath === file.path}
            <span class="cf-tick"><Check size={12} /></span>
          {:else}
            <span class="cf-gap"></span>
          {/if}
          <span class="cf-name">{file.name}</span>
          <span class="cf-path">{file.path}</span>
          <span class="cf-spacer"></span>
          <PicusDialectChip
            engine={fileEngine(file)}
            terse
            subject="file"
            inherited={declaredFileEngine(file) === null}
            from={picusProjectStore.folderOfFile(file.path)?.node.path ?? ''}
          />
        {/if}
      {/snippet}

      {#snippet empty()}
        <p class="cf-empty-text">
          {picusProjectStore.fileCount
            ? `No script matches “${query.trim()}”.`
            : 'This repository holds no SQL scripts.'}
        </p>
      {/snippet}
    </ClassifyPicker>

    <div class="cf-editor">
      {#if !selected}
        <StateBlock tone="info" fill={false} label="Pick a script above — type part of its path, then use ↑ ↓." />
      {:else}
        <p class="cf-target">{selected.path}</p>
        <p class="cf-standing">{standing}</p>

        <FormField
          label="Engine of this script"
          hint="Almost every script should inherit its folder — that is what the blank answer means, and it is the right one. Set an engine here only when the folder cannot say: a directory holding an Oracle script beside a PostgreSQL one, or the single script written for an engine Picus does not read."
        >
          <Select bind:value={engine} options={engineOptions} />
        </FormField>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <span class="cf-foot">
      Applying writes the repository's own configuration. A script that declares its own
      engine keeps it whatever its folder, or any rule, says afterwards.
    </span>
    <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
    <Button
      variant="primary"
      size="sm"
      disabled={!selected || applying}
      tooltip={{ content: 'Save this classification with the repository', shortcut: 'Ctrl+Enter' }}
      onclick={() => void apply()}
    >
      Apply
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .cf { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .cf-tick { display: inline-flex; color: var(--accent); flex-shrink: 0; }
  .cf-warn { display: inline-flex; color: var(--warning); flex-shrink: 0; }
  .cf-gap { width: 12px; flex-shrink: 0; }

  .cf-name { font-family: var(--font-code); font-size: var(--font-size-xs); white-space: nowrap; }
  .cf-path {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .cf-spacer { flex: 1; }

  .cf-editor {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 16px 14px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .cf-target { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .cf-standing { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }

  .cf-empty-text { margin: 0; font-size: var(--font-size-sm); }

  .cf-foot {
    flex: 1;
    font-size: var(--font-size-xs);
    line-height: 1.45;
    color: var(--text-muted);
    text-align: left;
  }
</style>
