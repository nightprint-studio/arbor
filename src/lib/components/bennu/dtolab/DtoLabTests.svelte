<script lang="ts">
  /**
   * The Tests tab: which fields, which template, where — then a preview of what would be written.
   *
   * Every constrained field is chosen by default, and "all of them" is stored as no choice at all, so a
   * constraint added to the class while the lab is open is not silently left out of the next run.
   */
  import { FolderInput, Wand2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuDtoLabStore as lab } from '$lib/stores/bennu/dtolab.svelte';
  import type { DtoLabConstraint } from '$lib/ipc/bennu/dtolab';
  import DtoLabTemplateBar from './DtoLabTemplateBar.svelte';

  const constrained = $derived(lab.view?.class.fields.filter((f) => f.constraints.length > 0) ?? []);
  const chosen = $derived(lab.fields ?? constrained.map((f) => f.name));
  const targetFile = $derived(lab.target?.file ?? lab.view?.test_file ?? '');

  let picking = $state(false);

  function toggleField(name: string, on: boolean) {
    const next = on ? [...new Set([...chosen, name])] : chosen.filter((n) => n !== name);
    lab.setFields(next.length === constrained.length ? null : next);
  }

  function describe(c: DtoLabConstraint): string {
    const written = Object.entries(c.attributes)
      .filter(([key]) => key !== 'message' && key !== 'groups' && key !== 'payload')
      .map(([key, value]) => `${key} = ${value}`);
    return written.length > 0 ? `@${c.name}(${written.join(', ')})` : `@${c.name}`;
  }

  function relative(path: string): string {
    const root = projectStore.project?.root;
    return root && path.startsWith(root) ? path.slice(root.length).replace(/^[\\/]/, '') : path;
  }
</script>

<div class="tests">
  <section class="fields">
    <div class="col-head">
      <span class="col-title">Fields</span>
      <span class="n">{chosen.length} of {constrained.length}</span>
    </div>
    {#if constrained.length === 0}
      <div class="empty">
        <EmptyState
          compact
          message="No constraints in this class"
          description="Tests are generated from the Bean Validation constraints on its fields."
        />
      </div>
    {:else}
      <ul class="field-list">
        {#each constrained as field (field.name)}
          <li class="field">
            <Toggle
              size="sm"
              checked={chosen.includes(field.name)}
              ariaLabel={`Generate tests for ${field.name}`}
              onchange={(on) => toggleField(field.name, on)}
            />
            <span class="f-name">{field.name}</span>
            <span class="f-type">{field.type_name}</span>
            <span class="f-constraints">
              {#each field.constraints as constraint, i (i)}
                <span class="constraint" use:tooltip={describe(constraint)}>@{constraint.name}</span>
              {/each}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section class="output">
    <div class="col-head"><span class="col-title">Output</span></div>
    <div class="output-body">
      <FormField label="Template">
        <DtoLabTemplateBar />
      </FormField>
      <FormField
        label="Written to"
        hint={lab.target
          ? 'An existing test file — the preview lets you choose the class inside it.'
          : "The class's own test file: created, or added to when it already exists."}
      >
        <div class="target">
          <span class="target-path" use:tooltip={targetFile}>{relative(targetFile)}</span>
          <Button variant="ghost" size="xs" onclick={() => (picking = true)}>
            {#snippet iconStart()}<FolderInput size={12} />{/snippet}
            Choose a test file…
          </Button>
          {#if lab.target}
            <Button variant="ghost" size="xs" onclick={() => lab.setTarget(null)}>Use the class's own</Button>
          {/if}
        </div>
      </FormField>
      {#if lab.generateError}
        <Alert variant="error" compact text={lab.generateError} />
      {/if}
      <div class="actions">
        <Button
          variant="primary"
          size="sm"
          loading={lab.generating}
          disabled={chosen.length === 0}
          onclick={() => void lab.generate()}
        >
          {#snippet iconStart()}<Wand2 size={13} />{/snippet}
          Generate…
        </Button>
        <span class="hint">Each case is checked on the project's JVM before the preview opens.</span>
      </div>
    </div>
  </section>
</div>

{#if picking}
  <FileExplorerModal
    mode="file"
    extensions={['java']}
    title="Add the tests to…"
    initialPath={projectStore.project?.root}
    onConfirm={(path) => { picking = false; lab.setTarget({ file: path, class: null }); }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .tests { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .fields, .output { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .output { border-left: 1px solid var(--border-subtle); }
  .col-head {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    min-height: 30px; padding: 3px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .col-title {
    font-size: var(--font-size-2xs); font-weight: 700; letter-spacing: 0.4px; text-transform: uppercase;
    color: var(--text-muted);
  }
  .n { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .empty { flex: 1; display: flex; align-items: center; justify-content: center; }
  .field-list { list-style: none; margin: 0; padding: 3px 0; overflow-y: auto; flex: 1; min-height: 0; }
  .field { display: flex; align-items: center; gap: 8px; padding: 3px 10px; }
  .field:hover { background: var(--bg-hover); }
  .f-name { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .f-type { font-size: var(--font-size-2xs); color: var(--text-muted); white-space: nowrap; }
  .f-constraints { margin-left: auto; display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 3px; }
  .constraint {
    font-size: var(--font-size-3xs); font-weight: 600; letter-spacing: 0.2px;
    padding: 0 5px; border-radius: var(--radius-sm);
    color: var(--accent); background: var(--accent-subtle);
  }
  .output-body { flex: 1; min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 10px; padding: 10px 12px; }
  .target { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .target-path {
    min-width: 0; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary);
  }
  .actions { display: flex; align-items: center; gap: 10px; margin-top: 2px; }
  .hint { font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
