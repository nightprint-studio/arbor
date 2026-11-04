<script lang="ts">
  /**
   * GeneratorShell — the frame every code-generator dialog shares: options on the left (the caller's
   * `form`), the live preview on the right (one tab per file, the members inserted into the open file,
   * and the snippets meant for other files), the notes the generator raised, and the write.
   *
   * Each generator keeps its own modal — its icon, its form, its defaults — and only this frame is common,
   * so a change to how a preview, a warning or the write behaves is made once.
   *
   * Keyboard: the first field is focused on open, Tab walks the form, Ctrl/Cmd+Enter generates and Esc
   * cancels (the Modal owns it).
   */
  import type { Component, Snippet } from 'svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { baseName } from '$lib/utils/paths';
  import type { GeneratorSession } from '$lib/stores/bennu/generator-session.svelte';
  import { languageForPath } from '../languages';

  let {
    session,
    icon: Icon,
    title,
    onClose,
    form,
    width = '1120px',
    height = '700px',
    verb = 'Generate',
  }: {
    session: GeneratorSession<any>;
    icon: Component<any>;
    title: string;
    onClose: () => void;
    /** The option form; rendered inside the scrolling left column. */
    form: Snippet;
    width?: string;
    height?: string;
    /** The primary button's label. */
    verb?: string;
  } = $props();

  const opened = $derived(session.opened);
  const preview = $derived(session.preview);

  // ── Preview tabs ────────────────────────────────────────────────────────────
  interface Shown { id: string; label: string; text: string; ext: string }

  const EXTENSIONS: Record<string, string> = {
    java: 'java', rust: 'rs', yaml: 'yaml', typescript: 'ts', xml: 'xml', json: 'json', toml: 'toml',
  };

  const shown = $derived.by<Shown[]>(() => {
    if (!preview) return [];
    const out: Shown[] = preview.files.map((f, i) => ({
      id: `file:${i}`,
      label: f.name,
      text: f.text,
      ext: f.name.split('.').pop() ?? 'java',
    }));
    if (preview.members) {
      const target = preview.members.class ?? (preview.members.file ? baseName(preview.members.file) : 'the open file');
      out.push({
        id: 'members',
        label: `into ${target}`,
        text: preview.members.text,
        ext: EXTENSIONS[opened?.meta.language ?? 'java'] ?? 'java',
      });
    }
    preview.extras.forEach((x, i) => out.push({ id: `extra:${i}`, label: x.label, text: x.text, ext: EXTENSIONS[x.language] ?? 'txt' }));
    return out;
  });

  let picked = $state<string | null>(null);
  const active = $derived(shown.find((s) => s.id === picked) ?? shown[0] ?? null);
  const tabs = $derived<TabItem[]>(shown.map((s) => ({ id: s.id, label: s.label })));
  const language = $derived(active ? languageForPath(`Preview.${active.ext}`) : null);

  // ── Summary line ────────────────────────────────────────────────────────────
  const summary = $derived.by(() => {
    if (!preview) return '';
    const parts: string[] = [];
    const files = preview.files.length;
    if (files > 0) parts.push(`creates ${files} file${files === 1 ? '' : 's'}`);
    if (preview.edits.length > 0) parts.push(`edits ${preview.members?.file ? baseName(preview.members.file) : 'the open file'}`);
    return parts.length ? parts.join(' · ') : '';
  });

  // ── Focus + keyboard ────────────────────────────────────────────────────────
  let formEl = $state<HTMLElement | null>(null);
  let focused = false;
  $effect(() => {
    if (!opened || !formEl || focused) return;
    focused = true;
    formEl.querySelector<HTMLElement>('input:not([disabled]), button:not([disabled]), [tabindex="0"]')?.focus();
  });

  async function generate() {
    if (!session.canApply) return;
    if (await session.apply()) onClose();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void generate();
    }
  }
</script>

<Modal {onClose} {width} {height} padBody={false} bodyBorder ariaLabel={title}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Icon size={14} />
      <span class="modal-title">{title}</span>
      {#if opened?.class}<span class="gs-target">on <code>{opened.class.name}</code></span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="gs" onkeydown={onKey} role="presentation">
    <div class="gs-left" bind:this={formEl}>
      {#if session.loading}
        <div class="gs-center"><Spinner /></div>
      {:else if !opened}
        <Alert variant="error" text={session.error ?? 'The generator could not be opened.'} />
      {:else}
        {#if opened.detected.length > 0}
          <div class="gs-detected" aria-label="Detected in the project">
            {#each opened.detected as chip (chip)}
              <Badge variant="tone" tone="neutral" size="sm" label={chip} />
            {/each}
          </div>
        {/if}
        {@render form()}
      {/if}
    </div>

    <div class="gs-right">
      <div class="gs-tabs">
        {#if tabs.length > 0}
          <Tabs items={tabs} value={active?.id ?? null} variant="segmented" size="sm" onSelect={(id) => (picked = id)} ariaLabel="Preview" />
        {/if}
        <span class="gs-busy">{#if session.busy}<Spinner size={12} />{/if}</span>
      </div>

      <div class="gs-preview">
        {#if active && language}
          {#key `${active.id}.${active.ext}`}
            <CodeEditor value={active.text} {language} readOnly />
          {/key}
        {:else if session.error}
          <div class="gs-center"><Alert variant="error" text={session.error} /></div>
        {:else}
          <div class="gs-center"><EmptyState message={session.loading ? '' : 'Nothing to generate yet.'} /></div>
        {/if}
      </div>

      {#if preview && (preview.wiring.length > 0 || preview.notes.length > 0 || preview.dependencies.length > 0 || session.exists.length > 0 || (session.error && active))}
        <div class="gs-notes">
          {#if session.error && active}
            <Alert variant="error" compact text={session.error} />
          {/if}
          {#each session.exists as f (f.path)}
            <Alert variant="error" compact text={`${f.name} already exists — choose another name or package.`} />
          {/each}
          {#if preview.wiring.length > 0}
            <Alert variant="info" compact title="Also adds">
              {#each preview.wiring as line (line)}<div class="gs-wire"><code>{line}</code></div>{/each}
            </Alert>
          {/if}
          {#if preview.dependencies.length > 0}
            <Alert variant="warning" compact title="Add to the build">
              {#each preview.dependencies as dep (dep)}<div class="gs-wire"><code>{dep}</code></div>{/each}
            </Alert>
          {/if}
          {#each preview.notes as note (note)}
            <Alert variant="info" compact text={note} />
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="gs-summary">{summary}</span>
      <div class="gs-actions">
        <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          disabled={!session.canApply}
          loading={session.applying}
          tooltip={{ content: verb, shortcut: 'Ctrl+Enter' }}
          onclick={() => void generate()}
        >
          {verb}
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .gs {
    display: grid;
    grid-template-columns: minmax(360px, 440px) minmax(0, 1fr);
    height: 100%;
    min-height: 0;
  }

  .gs-left {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    overflow-x: hidden;
    overflow-y: auto;
    border-right: 1px solid var(--border-subtle);
    min-height: 0;
    min-width: 0;
  }

  .gs-detected { display: flex; flex-wrap: wrap; gap: 5px; }

  .gs-right {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px 12px;
    min-height: 0;
    min-width: 0;
  }

  .gs-tabs { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-height: 28px; }
  .gs-busy { display: inline-flex; width: 16px; justify-content: center; }

  .gs-preview {
    flex: 1;
    min-height: 0;
    display: flex;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-base);
  }
  .gs-preview > :global(.code-editor) { flex: 1; min-width: 0; min-height: 0; }
  .gs-preview :global(.cm-editor) { height: 100%; }

  .gs-center { flex: 1; display: flex; align-items: center; justify-content: center; padding: 14px; }

  .gs-notes { display: flex; flex-direction: column; gap: 6px; max-height: 32%; overflow-y: auto; flex-shrink: 0; }
  .gs-wire code { font-family: var(--font-code); font-size: var(--font-size-xs); }

  .gs-target { font-size: var(--font-size-xs); color: var(--text-muted); }
  .gs-target code { font-family: var(--font-code); color: var(--text-secondary); }

  .gs-summary { font-size: var(--font-size-xs); color: var(--text-muted); }
  .gs-actions { display: flex; align-items: center; gap: 8px; }
</style>
