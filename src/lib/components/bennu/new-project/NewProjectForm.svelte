<script lang="ts">
  /**
   * The right column of the wizard: the common fields (name, location, Git), the generator's own
   * fields, and the live "what will be created" panel.
   *
   * Everything the generator needs is described by the backend (`GeneratorField`), so this file
   * knows no generator: a new project type shows up here with no change.
   */
  import { Check, FolderOpen, TriangleAlert, Info } from 'lucide-svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import FormSection from '$lib/components/shared/ui/FormSection.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { newProjectStore as store } from '$lib/stores/bennu/new-project.svelte';
  import { joinForDisplay, namePlaceholder, problemFor, splitFields, strayProblems } from './new-project-helpers';
  import NewProjectField from './NewProjectField.svelte';
  import NewProjectIcon from './NewProjectIcon.svelte';
  import NewProjectSummary from './NewProjectSummary.svelte';

  let { oncreate }: { oncreate: () => void } = $props();

  let pickerOpen = $state(false);
  let moreOpen = $state(false);

  const generator = $derived(store.generator);
  const preview = $derived(store.preview);
  const problems = $derived(preview?.problems ?? []);
  const split = $derived(splitFields(store.fields, store.values));
  const drawn = $derived(new Set(['name', 'location', 'generator', ...[...split.basic, ...split.advanced].map((f) => f.key)]));
  const stray = $derived(strayProblems(problems, drawn));
  const gitInstalled = $derived(store.catalog?.tools.find((t) => t.name === 'git')?.found ?? false);
  const target = $derived(preview?.target ?? null);
  /** Held back until the user has typed, or tried to create. */
  const nameError = $derived(store.attempted || store.name.trim() ? problemFor(problems, 'name') : null);
  const generatorProblem = $derived(problemFor(problems, 'generator'));
  const targetBlocked = $derived(!!target && ['non_empty_dir', 'is_file', 'bad_location'].includes(target.state));
  const hasProject = $derived(!!projectStore.project);

  function enterCreates(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      oncreate();
    }
  }
</script>

{#if generator}
  <div class="form">
    <div class="head">
      <span class="head-icon"><NewProjectIcon name={generator.icon} size={20} /></span>
      <div class="head-text">
        <h2>{generator.label}</h2>
        <p>{generator.description}</p>
      </div>
    </div>

    {#if generatorProblem}
      <Alert variant="warning" compact text={generatorProblem} />
    {/if}
    {#if store.fieldsStale}
      <Alert variant="info" compact text="Showing the last list that was downloaded — you appear to be offline. Creating the project needs a connection." />
    {/if}

    <div class="row2">
      <FormField label="Name" required error={nameError}>
        <Input
          value={store.name}
          block
          autofocus
          placeholder={namePlaceholder(generator.name_rule)}
          error={nameError}
          ariaLabel="Project name"
          oninput={(v) => store.setName(v)}
          onkeydown={enterCreates}
        />
      </FormField>
      <FormField label="Location" error={problemFor(problems, 'location')}>
        <div class="loc">
          <Input
            value={store.location}
            block
            placeholder="Folder to create the project in"
            error={problemFor(problems, 'location')}
            ariaLabel="Project location"
            oninput={(v) => store.setLocation(v)}
            onkeydown={enterCreates}
          />
          <Button variant="outline" tooltip={{ content: 'Choose a folder' }} ariaLabel="Browse for a folder" onclick={() => (pickerOpen = true)}>
            {#snippet iconStart()}<FolderOpen size={14} />{/snippet}
          </Button>
        </div>
      </FormField>
    </div>

    <!-- One line, always present, so the form doesn't jump as the path becomes valid. -->
    <div class="target" class:bad={targetBlocked}>
      {#if !target || target.state === 'incomplete'}
        <span class="muted">Choose a name and a location.</span>
      {:else if targetBlocked}
        <TriangleAlert size={12} /> <span>{target.message}</span>
      {:else}
        <Check size={12} />
        <span>
          {target.state === 'empty_dir' ? 'Uses the existing empty folder' : 'Will be created at'}
          <code>{target.path || joinForDisplay(store.location, store.name)}</code>
          {#if target.message}<span class="muted"> — {target.message.toLowerCase()}</span>{/if}
        </span>
      {/if}
    </div>

    {#if store.fieldsError}
      <Alert variant="error" title="Couldn't load the options" text={store.fieldsError}>
        {#snippet actions()}
          <Button variant="outline" size="sm" onclick={() => void store.retryFields()}>Try again</Button>
        {/snippet}
      </Alert>
    {:else if store.fieldsLoading && !store.fields.length}
      <div class="loading"><Spinner size="sm" /> Loading the options…</div>
    {:else}
      <div class="grid">
        {#each split.basic as f (f.key)}
          <div class:wide={f.kind === 'multi'}>
            <NewProjectField
              field={f}
              value={store.values[f.key]}
              error={problemFor(problems, f.key)}
              loading={store.fieldsLoading}
              onchange={(v) => store.setValue(f.key, v)}
            />
          </div>
        {/each}
      </div>

      {#if split.advanced.length}
        <Collapsible bind:open={moreOpen} chevron>
          {#snippet header()}<span class="more">More options</span>{/snippet}
          <div class="grid more-body">
            {#each split.advanced as f (f.key)}
              <div class:wide={f.kind === 'multi'}>
                <NewProjectField
                  field={f}
                  value={store.values[f.key]}
                  error={problemFor(problems, f.key)}
                  onchange={(v) => store.setValue(f.key, v)}
                />
              </div>
            {/each}
          </div>
        </Collapsible>
      {/if}
    {/if}

    {#each stray as p (p.field + p.message)}
      <Alert variant="warning" compact text={p.message} />
    {/each}

    <FormSection label="Project">
      <div class="common">
        <Toggle
          checked={store.git && gitInstalled}
          disabled={!gitInstalled}
          label="Create Git repository"
          description={gitInstalled ? 'Runs git init and adds a .gitignore' : 'git was not found on your PATH'}
          onchange={(v) => store.setGit(v)}
        />
        {#if hasProject}
          <FormField label="After creating">
            <Select
              value={store.openMode}
              fill
              ariaLabel="After creating"
              options={[
                { value: 'add', label: 'Add to the current workspace' },
                { value: 'replace', label: 'Open instead of the current workspace' },
              ]}
              onchange={(v) => store.setOpenMode(v === 'replace' ? 'replace' : 'add')}
            />
          </FormField>
        {/if}
      </div>
    </FormSection>

    <NewProjectSummary {preview} />
    <p class="foot"><Info size={11} /> The project is built in a scratch folder next to the destination and moved into place only when complete.</p>
  </div>
{/if}

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Project location"
    initialPath={store.location || undefined}
    onConfirm={(dir) => { store.setLocation(dir.replace(/\\/g, '/')); pickerOpen = false; }}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}

<style>
  .form { display: flex; flex-direction: column; gap: 12px; padding: 14px 18px 18px; }
  .head { display: flex; align-items: center; gap: 12px; }
  .head-icon {
    display: inline-flex; align-items: center; justify-content: center;
    width: 38px; height: 38px; flex-shrink: 0;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 11%, var(--bg-base));
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
    border-radius: var(--radius-lg);
  }
  .head-text h2 { margin: 0; font-size: var(--font-size-lg); font-weight: 600; color: var(--text-primary); }
  .head-text p { margin: 1px 0 0; font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }

  .row2 { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.4fr); gap: 12px; }
  .loc { display: flex; gap: 6px; align-items: stretch; }

  .target {
    display: flex; align-items: center; gap: 6px; min-height: 18px;
    font-size: var(--font-size-xs); color: var(--success);
  }
  .target.bad { color: var(--error); }
  .target code { font-family: var(--font-code); color: var(--text-secondary); }
  .muted { color: var(--text-disabled); }

  .grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 10px 12px; align-items: start; }
  .grid > .wide { grid-column: 1 / -1; }
  .more { font-size: var(--font-size-sm); color: var(--text-secondary); }
  .more-body { padding-top: 8px; }

  .loading { display: flex; align-items: center; gap: 8px; padding: 18px 2px; font-size: var(--font-size-sm); color: var(--text-muted); }
  .common { display: flex; flex-direction: column; gap: 10px; }
  .foot { display: flex; align-items: center; gap: 5px; margin: 0; font-size: var(--font-size-2xs); color: var(--text-disabled); }
</style>
