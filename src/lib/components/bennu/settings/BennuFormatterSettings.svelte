<script lang="ts">
  /**
   * Settings › Java › Formatter — every option of the Java formatter, rendered from the catalogue
   * (`format-options.ts`), with a live preview of the style being edited.
   *
   * Two scopes share the one set of rows: **Global** edits the style every project starts from;
   * **This project** edits the sparse override saved in the project's own `.arbor/bennu/config.toml`.
   * Which layer decides a value (`.editorconfig`, detection, the project) is badged on the row.
   */
  import { RotateCcw, ScanSearch, Wand2 } from 'lucide-svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuFormatStyleStore } from '$lib/stores/bennu/format-style.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { FORMAT_GROUPS, optionsOf, type FormatGroupId } from '../format-options';
  import FormatDetectionResult from './FormatDetectionResult.svelte';
  import FormatOptionRow from './FormatOptionRow.svelte';
  import FormatPreview from './FormatPreview.svelte';

  const s = bennuFormatStyleStore;

  let group = $state<FormatGroupId>('indentation');
  let confirmReset = $state(false);

  const groupTabs = FORMAT_GROUPS.map((g) => ({ id: g.id, label: g.label }));
  const current = $derived(FORMAT_GROUPS.find((g) => g.id === group) ?? FORMAT_GROUPS[0]);
  const options = $derived(optionsOf(group));

  const scopeOptions = $derived([
    { value: 'global', label: 'Global' },
    { value: 'project', label: 'This project', disabled: !s.hasProject },
  ]);

  // Re-read every layer when the project changes (and when the page opens: `.editorconfig` may have
  // been edited since). Edits still waiting out their debounce are written as the page goes away.
  $effect(() => {
    void s.load(projectStore.project?.root);
    return () => s.flush();
  });

  const resetMessage = $derived(
    s.scope === 'project'
      ? 'Remove this project’s formatter overrides? It goes back to the global style, .editorconfig and detection.'
      : 'Put every formatter option back to its default? This changes the style of every project that has no override of its own.',
  );
</script>

<div class="section-header">
  <h2>Formatter</h2>
  <p>
    How a Java file is formatted by <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>, on a selection, and on save. By default it only
    re-indents; every rewrite below is opt-in.
  </p>
</div>

<div class="fmt-host">
<div class="fmt">
  <div class="fmt-main">
    <div class="fmt-bar">
      <RadioGroup value={s.scope} options={scopeOptions} size="sm" nowrap onchange={(v) => s.setScope(v as 'global' | 'project')} />
      <span class="fmt-spacer"></span>
      <Button variant="outline" size="sm" disabled={!s.hasProject || s.detecting} onclick={() => s.detect()}>
        {#snippet iconStart()}{#if s.detecting}<Spinner size="xs" delay={250} />{:else}<ScanSearch size={13} />{/if}{/snippet}
        Detect from project…
      </Button>
      <Button variant="ghost" size="sm" onclick={() => (confirmReset = true)}>
        {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
        Reset
      </Button>
    </div>

    {#if s.scope === 'project'}
      <Alert variant="info" compact
             text="Only what differs from the style below is saved, in .arbor/bennu/config.toml of this project." />
    {:else if !s.hasProject}
      <Alert variant="info" compact text="Open a project to give it a style of its own." />
    {/if}

    {#if s.detection}
      <FormatDetectionResult detection={s.detection} onapply={() => s.applyDetection()} ondismiss={() => s.dismissDetection()} />
    {/if}

    <Tabs items={groupTabs} value={group} variant="pill" size="sm" overflow ariaLabel="Formatter option groups"
          onSelect={(id) => (group = id as FormatGroupId)} />

    <div class="card">
      <div class="card-section-title"><Wand2 size={12} /> {current.label}</div>
      <p class="set-hint">{current.description}</p>
      {#each options as option (option.key)}
        <FormatOptionRow
          {option}
          value={s.style[option.key]}
          inert={!!option.dependsOn && !s.style[option.dependsOn]}
          layer={s.layerOf(option.key)}
          effectiveValue={s.effective[option.key]}
          scope={s.scope}
          modified={s.isModified(option.key)}
          onchange={(v) => s.set(option.key, v as never)}
          onreset={() => s.resetKey(option.key)}
        />
      {/each}
    </div>
  </div>

  <div class="fmt-preview">
    <FormatPreview text={s.previewText} error={s.previewError} />
  </div>
</div>
</div>

{#if confirmReset}
  <ConfirmModal
    title="Reset formatter style"
    message={resetMessage}
    variant="warning"
    confirmLabel="Reset"
    onConfirm={() => { confirmReset = false; s.resetAll(); }}
    onCancel={() => (confirmReset = false)}
  />
{/if}

<style>
  /* Container, not viewport: the page sits in a settings pane of varying width. Wide enough, the
     preview stands beside the options; otherwise it sticks above them so an edit is always seen. */
  .fmt-host { container-type: inline-size; }
  .fmt { display: flex; flex-direction: column; }
  .fmt-main { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .fmt-bar { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .fmt-spacer { flex: 1; }

  .fmt-preview {
    position: sticky;
    top: 0;
    z-index: 1;
    order: -1;
    height: 230px;
    margin-bottom: 12px;
    padding-bottom: 4px;
    background: var(--bg-base);
  }

  @container (min-width: 860px) {
    .fmt { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 16px; align-items: start; }
    .fmt-preview { order: 0; height: min(560px, 75vh); margin-bottom: 0; }
  }
</style>
