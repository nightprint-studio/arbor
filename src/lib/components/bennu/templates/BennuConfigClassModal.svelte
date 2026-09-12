<script lang="ts">
  /**
   * A `@ConfigurationProperties` class written from the configuration file at the caret: the keys of a
   * group become fields typed by their values, and every group below becomes a nested type.
   *
   * A selection decides where it starts — the keys on the lines selected, under the group they share — and
   * without one the caret does: the innermost group it is in, with the groups around it offered instead.
   * Either way every key under the prefix has a box, so what the class gets is chosen, not taken whole. The class's shape is a template, so a record, a Lombok class or a plain one is a choice,
   * made once per project with "Use for this project". Nothing is written before Create.
   */
  import { onMount } from 'svelte';
  import { SvelteMap, SvelteSet } from 'svelte/reactivity';
  import { FilePlus2, FolderInput, Wand2 } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { baseName } from '$lib/utils/paths';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import {
    configClassKeys,
    configClassOrigin,
    createFileFromTemplate,
    renderTemplate,
    type ConfigClassOrigin,
    type ConfigKeyNode,
    type RenderedTemplate,
  } from '$lib/ipc/bennu/templates';
  import { languageForPath } from '../languages';
  import { isSpringConfigFile } from '../file-kind';
  import BennuTemplateBar from './BennuTemplateBar.svelte';
  import BennuTemplateOutput from './BennuTemplateOutput.svelte';
  import BennuConfigKeyList from './BennuConfigKeyList.svelte';

  let {
    caretContext,
    selection,
    onClose,
  }: {
    caretContext: () => { source: string; offset: number } | null;
    /** The editor's selection, in bytes; `null` when nothing is selected. */
    selection: () => { start: number; end: number } | null;
    onClose: () => void;
  } = $props();

  interface Origin { root: string; file: string; source: string; offset: number }

  let origin = $state<Origin | null>(null);
  let info = $state<ConfigClassOrigin | null>(null);
  let prefix = $state('');
  let className = $state('');
  let directory = $state('');
  let profiles = $state(true);
  let template = $state<string | null>(null);
  let rendered = $state<RenderedTemplate | null>(null);
  let error = $state<string | null>(null);
  let rendering = $state(false);
  let writing = $state(false);
  let picking = $state(false);
  let nodes = $state<ConfigKeyNode[]>([]);
  /** The keys ticked: single keys and lists — a group is whatever is below it. */
  const chosen = new SvelteSet<string>();
  /** A group's shape where it was chosen, by key: `true` a Map, `false` a type per key. */
  const maps = new SvelteMap<string, boolean>();
  /** What the selection named, and under which prefix — applied when that prefix's keys first arrive. */
  let seeded: string[] | null = null;
  let seededPrefix: string | null = null;
  /** The prefix whose keys are listed, so a reload for the profiles keeps what was ticked. */
  let listedPrefix: string | null = null;
  /** Plain, not reactive: bumped inside the effect, read only by the answers it outlives. */
  let generation = 0;

  const language = languageForPath('Preview.java');
  const leafCount = $derived(nodes.filter((n) => !n.group).length);

  onMount(() => {
    const root = projectStore.project?.root;
    const file = projectStore.activeFilePath;
    const ctx = caretContext();
    if (!root || !file || !isSpringConfigFile(file) || !ctx) {
      error = 'Put the caret in application.yml, or another Spring configuration file.';
      return;
    }
    // A selection names the keys; without one, the caret names the group.
    const range = selection();
    const at = { root, file, source: ctx.source, offset: range?.start ?? ctx.offset };
    configClassOrigin(root, file, ctx.source, at.offset, range?.end ?? null)
      .then((found) => {
        info = found;
        seeded = found.selected;
        seededPrefix = found.prefix;
        prefix = found.prefix ?? found.prefixes[0] ?? '';
        directory = found.directory;
        origin = at;
      })
      .catch((e) => { error = String(e); });
  });

  // The keys under the prefix, whenever it — or whether the profiles count — changes.
  $effect(() => {
    const at = origin;
    const current = prefix.trim();
    const withProfiles = profiles;
    if (!at || !current) return;
    let live = true;
    configClassKeys(at.file, at.source, current, withProfiles)
      .then((found) => {
        if (!live) return;
        const known = new Set(nodes.map((n) => n.key));
        const leaves = found.filter((n) => !n.group).map((n) => n.key);
        const picks = current === listedPrefix
          // Same prefix, new files: keep the choice, and tick what was not there before.
          ? leaves.filter((k) => chosen.has(k) || !known.has(k))
          : current === seededPrefix && seeded
            ? leaves.filter((k) => seeded!.some((s) => s === '' || k === s || k.startsWith(`${s}.`)))
            : leaves;
        if (current !== listedPrefix) maps.clear();
        listedPrefix = current;
        nodes = found;
        chosen.clear();
        for (const key of picks) chosen.add(key);
      })
      .catch(() => { if (live) nodes = []; });
    return () => { live = false; };
  });

  $effect(() => {
    const at = origin;
    if (!at) return;
    // Every input read here, synchronously, so any of them changing renders again.
    const request = {
      root: at.root,
      kind: 'config-class' as const,
      template,
      file: at.file,
      source: at.source,
      offset: at.offset,
      prefix: prefix.trim() || null,
      name: className.trim() || null,
      directory: directory || null,
      profiles,
      // Every key while the list is loading or all are ticked — the render needs no list to say "all".
      keys: nodes.length === 0 || chosen.size === leafCount ? null : [...chosen],
      maps: maps.size > 0 ? Object.fromEntries(maps) : null,
    };
    const seq = ++generation;
    rendering = true;
    const timer = setTimeout(() => {
      renderTemplate(request)
        .then((answer) => { if (seq === generation) { rendered = answer; error = null; } })
        .catch((e) => { if (seq === generation) error = String(e); })
        .finally(() => { if (seq === generation) rendering = false; });
    }, 200);
    return () => clearTimeout(timer);
  });

  /** The name the backend chose while none is typed — the prefix's last key, and `Properties`. */
  const suggestedName = $derived(rendered?.file ? baseName(rendered.file).replace(/\.java$/, '') : '');
  const folder = $derived(
    origin && directory.startsWith(origin.root) ? directory.slice(origin.root.length).replace(/^[\\/]/, '') : directory,
  );

  async function create() {
    const result = rendered;
    const at = origin;
    if (!result?.file || !at || writing || rendering) return;
    writing = true;
    try {
      if (!result.exists) {
        await createFileFromTemplate(at.root, result.file, result.text);
        void projectStore.refreshTree();
        toastStore.show(`Created ${baseName(result.file)}`, 'success');
      }
      await projectStore.openFile(result.file);
      onClose();
    } catch (e) {
      toastStore.show(`Couldn't write it: ${e}`, 'error');
    } finally {
      writing = false;
    }
  }

  function onWindowKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && !picking) {
      e.preventDefault();
      void create();
    }
  }
</script>

<svelte:window onkeydown={onWindowKeydown} />

<Modal {onClose} width="980px" height="720px" padBody={false} ariaLabel="Configuration class">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Wand2 size={14} />
      <span class="modal-title">Configuration class</span>
      {#if origin}<span class="file" use:tooltip={origin.file}>{baseName(origin.file)}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="body">
    {#if info}
      <div class="form">
        <FormField label="Class name">
          <Input bind:value={className} placeholder={suggestedName || 'MailProperties'} autofocus />
        </FormField>
        <FormField label="Keys under" hint="The prefix it binds: every key below it is a field.">
          {#if info.prefixes.length > 0}
            <Select
              value={prefix}
              options={info.prefixes.map((p) => ({ value: p, label: p }))}
              ariaLabel="Prefix"
              onchange={(next) => (prefix = next)}
            />
          {:else}
            <Input bind:value={prefix} placeholder="app.mail" ariaLabel="Prefix" />
          {/if}
        </FormField>
        <FormField label="Folder">
          <div class="folder">
            <span class="path" use:tooltip={directory}>{folder || directory}</span>
            <Button variant="ghost" size="xs" onclick={() => (picking = true)}>
              {#snippet iconStart()}<FolderInput size={12} />{/snippet}
              Choose…
            </Button>
          </div>
        </FormField>
        <FormField label="Profiles">
          <Toggle
            checked={profiles}
            disabled={info.profiles.length === 0}
            label={info.profiles.length === 0 ? 'No profile files beside it' : `Read ${info.profiles.join(', ')} too`}
            onchange={(on) => (profiles = on)}
          />
        </FormField>
        <div class="wide">
          <FormField label="Template">
            <BennuTemplateBar kind="config-class" value={template} onchange={(next) => (template = next)} />
          </FormField>
        </div>
      </div>
    {/if}
    {#if error}
      <div class="alert"><Alert variant="error" compact text={error} /></div>
    {/if}
    <div class="split">
      {#if nodes.length > 0}
        <div class="keys">
          <BennuConfigKeyList
            {nodes}
            {chosen}
            {maps}
            onMap={(key, asMap) => maps.set(key, asMap)}
            onToggle={(keys, on) => { for (const key of keys) { if (on) chosen.add(key); else chosen.delete(key); } }}
          />
        </div>
      {/if}
      <div class="out">
        {#if rendered}
          <BennuTemplateOutput {rendered} root={origin?.root ?? null} text={rendered.text} {language} />
        {/if}
      </div>
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="hint">Nothing is written until you create it — Ctrl+Enter</span>
      <div class="buttons">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button variant="primary" loading={writing || rendering} disabled={!rendered} onclick={() => void create()}>
          {#snippet iconStart()}<FilePlus2 size={13} />{/snippet}
          {rendered?.exists ? 'Open the existing file' : 'Create the file'}
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <FileExplorerModal
    mode="folder"
    title="Put the class in…"
    initialPath={directory || origin?.root}
    onConfirm={(path) => { picking = false; directory = path; }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

<style>
  .body { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .form { display: grid; grid-template-columns: 1fr 1fr; gap: 10px 14px; padding: 10px 12px 6px; }
  .wide { grid-column: 1 / -1; }
  .folder { display: flex; align-items: center; gap: 6px; min-width: 0; min-height: 28px; }
  .path { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .alert { padding: 0 12px 6px; }
  .split { flex: 1; min-height: 0; display: flex; }
  .keys { width: 280px; flex-shrink: 0; display: flex; flex-direction: column; min-height: 0; border-top: 1px solid var(--border-subtle); border-right: 1px solid var(--border-subtle); }
  .out { flex: 1; min-width: 0; display: flex; flex-direction: column; }
  .file { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-muted); }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .buttons { display: flex; align-items: center; gap: 6px; }
</style>
