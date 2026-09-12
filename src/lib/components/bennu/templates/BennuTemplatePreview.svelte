<script lang="ts">
  /**
   * A code template, rendered beside its source while it is being written.
   *
   * What renders is the **unsaved** text: a preview of the file on disk would always be one save
   * behind the line being typed, which is the line the preview is for. It renders against what the
   * template's kind needs — a class for most (the first class of a Java file open in a tab, or any class picked here by name),
   * a name for a New file template, a configuration file and a prefix for a configuration class — and
   * the kind comes from the folder the template is kept in; a
   * Jinja file kept anywhere else picks one here.
   *
   * **Parameters** — a JSON object laid over that data — show the branches the class at hand does not
   * take, without hunting for a class that would. Kept per template for the session.
   *
   * Rendering is debounced and a slower answer never overwrites a newer one, so typing through a
   * half-written tag shows the last output that worked, dimmed under the error, instead of flicker.
   *
   * **Linked lines**: the caret's line in the template lights up the lines it wrote here, scrolled into view
   * when they are off screen; a selection here lights up, in the template, the lines that wrote it —
   * `template-link.svelte.ts`.
   */
  import { onMount } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { Braces, Eye, FolderInput, Search, X } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import CodeEditor from '$lib/components/shared/ui/code-editor/CodeEditor.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { baseName } from '$lib/utils/paths';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuTemplatesStore as templates } from '$lib/stores/bennu/templates.svelte';
  import { renderTemplate, type RenderedTemplate, type TemplateKindId } from '$lib/ipc/bennu/templates';
  import { languageForPath } from '../languages';
  import type { ClassEntry } from '$lib/types/bennu';
  import BennuClassPicker from '../BennuClassPicker.svelte';
  import { templateFileParts, templateKindOfPath } from './template-paths';
  import BennuTemplateParameters from './BennuTemplateParameters.svelte';
  import { createTemplateLink } from './template-link.svelte';
  import { isSpringConfigFile } from '../file-kind';

  let {
    path,
    text,
    caret,
    onSourceLines,
    onClose,
  }: {
    /** The template file. */
    path: string;
    /** Its buffer, as it is now. */
    text: string;
    /** The caret in the template — a new object for every move, a click back on the same line included. */
    caret: { line: number; col: number };
    /** The template lines that wrote what is selected here; `[]` once the caret is back in the template. */
    onSourceLines: (lines: number[]) => void;
    onClose: () => void;
  } = $props();

  onMount(() => { void templates.load(); });

  let chosenKind = $state<TemplateKindId>('class');
  let chosenClass = $state<ClassEntry | null>(null);
  let name = $state('Example');
  let picking = $state(false);
  let chosenConfig = $state<string | null>(null);
  let prefix = $state('');
  let pickingConfig = $state(false);
  let rendered = $state<RenderedTemplate | null>(null);
  let error = $state<string | null>(null);
  let paramsOpen = $state(false);
  /** The parameters typed for each template, by path — the component outlives a tab switch. */
  const paramsByPath = new SvelteMap<string, string>();
  /** Plain, not reactive: bumped inside the effect, and read only by the answers it outlives. */
  let generation = 0;

  const folderKind = $derived(templateKindOfPath(path));
  const kind = $derived(folderKind ?? chosenKind);
  const parts = $derived(templateFileParts(path));
  const root = $derived(projectStore.project?.root ?? null);
  const needsConfig = $derived(kind === 'config-class');
  const needsClass = $derived(kind !== 'new-file' && !needsConfig);
  const configTabs = $derived(projectStore.openFilePaths.filter((p) => isSpringConfigFile(p)));
  /** The configuration file to render from: the one chosen, else the last one opened. */
  const configFile = $derived(chosenConfig ?? configTabs[configTabs.length - 1] ?? null);
  const configReady = $derived(!!configFile && prefix.trim() !== '');
  const javaTabs = $derived(projectStore.openFilePaths.filter((p) => p.toLowerCase().endsWith('.java')));
  /** The file to render from: the picked class's, else the last Java file opened. */
  const file = $derived(chosenClass?.file ?? javaTabs[javaTabs.length - 1] ?? null);
  /** What the row names: the class picked, or the file's own name — which is its first class's too. */
  const className = $derived(chosenClass?.simple ?? (file ? baseName(file).replace(/\.java$/i, '') : null));
  const kindTitle = $derived(templates.all.find((k) => k.kind === kind)?.title ?? kind);
  const language = $derived(languageForPath(`preview.${parts.extension}`));
  const shown = $derived(rendered ? rendered.insertion?.text ?? rendered.text : '');
  const paramsText = $derived(paramsByPath.get(path) ?? '');
  const parameters = $derived.by((): { value: Record<string, unknown> | null; error: string | null } => {
    const raw = paramsText.trim();
    if (!raw) return { value: null, error: null };
    try {
      const value: unknown = JSON.parse(raw);
      return value !== null && typeof value === 'object' && !Array.isArray(value)
        ? { value: value as Record<string, unknown>, error: null }
        : { value: null, error: 'Parameters are a JSON object: { "name": value }' };
    } catch (e) {
      return { value: null, error: `Parameters: ${e instanceof Error ? e.message : String(e)}` };
    }
  });
  const paramsCount = $derived(parameters.value ? Object.keys(parameters.value).length : 0);

  const link = createTemplateLink({
    caret: () => caret,
    sourceLines: () => rendered?.source_lines ?? null,
    shown: () => shown,
    onSourceLines: (lines) => onSourceLines(lines),
  });

  $effect(() => {
    // Every input read here, synchronously, so any of them changing renders again.
    const request = {
      root: root ?? '',
      kind,
      template: parts.name,
      text,
      extension: parts.extension,
      file: needsClass ? file : needsConfig ? configFile : null,
      // By name, so a nested class — or any but the file's first — is the one rendered.
      class: needsClass ? chosenClass?.simple ?? null : null,
      // A class open in a tab renders from its buffer, like the template does. An empty one is a tab
      // not loaded yet, and sending it would render a file with no class in it: the disk answers then.
      source: needsClass && file
        ? projectStore.sourceOf(file) || null
        : needsConfig && configFile ? projectStore.sourceOf(configFile) || null : null,
      directory: needsClass || needsConfig ? null : file ? file.replace(/[\\/][^\\/]*$/, '') : root,
      name: needsClass || needsConfig ? null : name.trim() || 'Example',
      prefix: needsConfig ? prefix.trim() || null : null,
      parameters: parameters.value,
      traceLines: true,
    };
    const seq = ++generation;
    if (!root || (needsClass && !file) || (needsConfig && !configReady)) {
      rendered = null;
      error = null;
      return;
    }
    // Half-typed JSON keeps the last output, dimmed under what is wrong — the same as a template.
    if (parameters.error) {
      error = parameters.error;
      return;
    }
    const timer = setTimeout(async () => {
      try {
        const answer = await renderTemplate(request);
        if (seq !== generation) return;
        rendered = answer;
        error = null;
      } catch (e) {
        if (seq === generation) error = String(e);
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  function relative(target: string): string {
    return root && target.startsWith(root) ? target.slice(root.length).replace(/^[\\/]/, '') : baseName(target);
  }

  const destination = $derived.by(() => {
    if (!rendered) return '';
    switch (rendered.output) {
      case 'file': return `${rendered.exists ? 'Would overwrite' : 'A new file'} · ${rendered.file ? relative(rendered.file) : ''}`;
      case 'members': return `Inserted into the class · ${rendered.file ? baseName(rendered.file) : ''}`;
      default: return 'Text to copy or paste';
    }
  });
</script>

<div class="tp">
  <div class="tp-bar">
    <span class="tp-title"><Eye size={12} /> Preview</span>
    {#if folderKind}
      <span class="tp-kind" use:tooltip={'The kind comes from the folder the template is kept in'}>{kindTitle}</span>
    {:else}
      <div class="tp-kind-select">
        <Select
          value={chosenKind}
          options={templates.all.map((k) => ({ value: k.kind, label: k.title }))}
          size="sm"
          ariaLabel="Render as"
          onchange={(next) => (chosenKind = next as TemplateKindId)}
        />
      </div>
    {/if}
    <span class="tp-spacer"></span>
    <IconButton tooltip="Close the preview" size={22} onclick={onClose}><X size={13} /></IconButton>
  </div>

  <div class="tp-inputs">
    {#if needsClass}
      <span class="tp-label">Class</span>
      <span class="tp-file" use:tooltip={chosenClass?.fqcn ?? (file ? relative(file) : '')}>{className ?? 'none chosen'}</span>
      <Button variant="ghost" size="xs" onclick={() => (picking = true)}>
        {#snippet iconStart()}<Search size={12} />{/snippet}
        Choose…
      </Button>
    {:else if needsConfig}
      <span class="tp-label">File</span>
      <span class="tp-file" use:tooltip={configFile ?? ''}>{configFile ? baseName(configFile) : 'none chosen'}</span>
      <Button variant="ghost" size="xs" onclick={() => (pickingConfig = true)}>
        {#snippet iconStart()}<FolderInput size={12} />{/snippet}
        Choose…
      </Button>
      <span class="tp-label">Keys under</span>
      <div class="tp-name"><Input bind:value={prefix} size="sm" placeholder="app.mail" ariaLabel="The prefix the class binds" /></div>
    {:else}
      <span class="tp-label">Name</span>
      <div class="tp-name"><Input bind:value={name} size="sm" ariaLabel="The name typed in New file" /></div>
    {/if}
    <span class="tp-spacer"></span>
    <Button variant="ghost" size="xs" onclick={() => (paramsOpen = !paramsOpen)}>
      {#snippet iconStart()}<Braces size={12} />{/snippet}
      Parameters{#if paramsCount > 0} · {paramsCount}{/if}
    </Button>
  </div>

  {#if paramsOpen}
    <BennuTemplateParameters {kind} value={paramsText} onchange={(text) => paramsByPath.set(path, text)} />
  {/if}

  {#if error}
    <div class="tp-alert"><Alert variant="error" compact text={error} /></div>
  {/if}

  {#if rendered}
    <div class="tp-out">
      <span class="tp-dest">{destination}</span>
      {#each rendered.notes as note, i (i)}<span class="tp-note">{note}</span>{/each}
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="tp-code" class:stale={!!error} onpointerup={link.fromPreview} onkeyup={link.fromPreview}>
      {#key language}<CodeEditor bind:this={link.editor} value={shown} {language} readOnly lineHighlights={link.marks} />{/key}
    </div>
  {:else if !root}
    <div class="tp-empty">
      <EmptyState compact message="Open a project to preview" description="A template renders against the project's classes." />
    </div>
  {:else if needsConfig && !configReady}
    <div class="tp-empty">
      <EmptyState compact message="Choose a configuration file and its keys" description="Open application.yml in a tab or pick one, then type the prefix the class binds — app.mail." />
    </div>
  {:else if needsClass && !file}
    <div class="tp-empty">
      <EmptyState compact message="Choose a class to render with" description="Open a Java file in a tab, or pick a class with Choose…" />
    </div>
  {/if}
</div>

{#if pickingConfig}
  <FileExplorerModal
    mode="file"
    extensions={['yml', 'yaml', 'properties']}
    title="Render the template with…"
    initialPath={root ?? undefined}
    onConfirm={(picked) => { pickingConfig = false; chosenConfig = picked; }}
    onCancel={() => (pickingConfig = false)}
    onClose={() => (pickingConfig = false)}
  />
{/if}

{#if picking}
  <BennuClassPicker
    placeholder="Render the template with a class — type its name"
    kinds={['class', 'record']}
    onPick={(entry) => { picking = false; chosenClass = entry; }}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .tp { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--bg-base); border-left: 1px solid var(--border-subtle); }
  .tp-bar { display: flex; align-items: center; gap: 8px; padding: 4px 6px 4px 10px; border-bottom: 1px solid var(--border-subtle); }
  .tp-title { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-xs); font-weight: 600; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.04em; }
  .tp-kind { font-size: var(--font-size-xs); color: var(--accent); padding: 1px 7px; border-radius: var(--radius-sm); background: var(--accent-subtle); }
  .tp-kind-select { min-width: 170px; }
  .tp-spacer { flex: 1; }
  .tp-inputs { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--border-subtle); min-width: 0; }
  .tp-label { font-size: var(--font-size-xs); color: var(--text-muted); flex-shrink: 0; }
  .tp-file { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tp-name { flex: 1; min-width: 0; max-width: 260px; }
  .tp-alert { padding: 6px 10px 0; }
  .tp-out { display: flex; flex-direction: column; gap: 2px; padding: 6px 10px; }
  .tp-dest { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .tp-note { font-size: var(--font-size-xs); color: var(--text-muted); }
  .tp-code { flex: 1; min-height: 0; display: flex; flex-direction: column; transition: opacity 120ms ease; }
  .tp-code.stale { opacity: 0.45; }
  .tp-empty { flex: 1; display: flex; align-items: center; justify-content: center; padding: 16px; }
  /* The lines at either end of the link — the template's in its editor, the output's here. Global because each
     is a CodeMirror row; one rule because it is one link. */
  :global(.cm-template-link) {
    background: color-mix(in srgb, var(--syntax-annotation, #bbb529) 16%, transparent);
    box-shadow: inset 3px 0 0 color-mix(in srgb, var(--syntax-annotation, #bbb529) 70%, transparent);
  }
  @media (prefers-reduced-motion: reduce) { .tp-code { transition: none; } }
</style>
