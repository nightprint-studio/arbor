<script lang="ts">
  /**
   * Name a code template: a new one, created as a copy of another of its kind and opened in the
   * editor, or one of yours being renamed.
   *
   * The copy is the point: a template written from nothing has to rediscover what its kind is
   * rendered with, and one copied from a working template starts out producing something.
   *
   * Renaming shares this dialog because the rules for a name are the rules for a name — an
   * abbreviation has to stay a word you can type whether it is being created or corrected.
   */
  import { FilePlus2, TextCursorInput } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuTemplatesStore as templates } from '$lib/stores/bennu/templates.svelte';
  import { jinjaLanguages } from '$lib/utils/jinja-words';
  import type { TemplateKindId } from '$lib/ipc/bennu/templates';

  let {
    kind,
    from = null,
    rename = null,
    onClose,
    onCreated,
  }: {
    kind: TemplateKindId;
    /** The template to copy; the kind's first when absent. */
    from?: string | null;
    /** The template being renamed — this is a rename dialog when it is set. */
    rename?: string | null;
    onClose: () => void;
    onCreated?: (name: string) => void;
  } = $props();

  const info = $derived(templates.of(kind));
  const sources = $derived(info?.templates ?? []);
  /** Whether this kind writes what it is told to — a New file template, an abbreviation — as opposed
   *  to writing a Java class or a property file because that is what the kind is. */
  const picksLanguage = $derived(info?.picks_language ?? false);
  const languages = jinjaLanguages();

  // The starting point, not a binding: the dialog is mounted for one template and unmounted when it
  // closes, so `rename` cannot change under it.
  // svelte-ignore state_referenced_locally
  let name = $state(rename ?? '');
  let chosen = $state<string | null>(null);
  /** What it writes; `null` until chosen, when the copied template's language is kept. */
  let language = $state<string | null>(null);
  let saving = $state(false);

  const renaming = $derived(!!rename);
  const source = $derived(chosen ?? from ?? sources[0]?.name ?? null);
  /** The language shown: the one picked, else the one the template being copied writes. */
  const sourceLanguage = $derived(sources.find((t) => t.name === source)?.extension ?? '');
  const extension = $derived(language ?? sourceLanguage);
  const trimmed = $derived(name.trim());
  // An abbreviation's name is what gets typed, so it has to be a word the editor completes.
  const pattern = $derived(kind === 'live' ? /^\w+$/ : /^[A-Za-z0-9_-]+$/);
  // Its own name is not taken — and a change of case alone is a rename, so the comparison is exact.
  const taken = $derived(sources.some((t) => t.name === trimmed && t.name !== rename));
  const error = $derived(
    !trimmed
      ? null
      : !pattern.test(trimmed)
        ? kind === 'live'
          ? 'An abbreviation is one word: letters, digits and “_”.'
          : 'Letters, digits, “-” and “_”.'
        : taken
          ? `There is already a template called “${trimmed}”.`
          : null,
  );
  const valid = $derived(!!trimmed && !error && trimmed !== rename);

  async function submit() {
    if (!valid || saving) return;
    saving = true;
    const done = rename
      ? await templates.rename(kind, rename, trimmed)
      : await templates.create(kind, trimmed, source, picksLanguage ? extension : null);
    saving = false;
    if (!done) return;
    onCreated?.(trimmed);
    onClose();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      void submit();
    }
  }
</script>

<Modal {onClose} width="540px" height="auto" ariaLabel={renaming ? 'Rename code template' : 'New code template'}>
  {#snippet header()}
    <ModalHeader {onClose}>
      {#if renaming}<TextCursorInput size={14} />{:else}<FilePlus2 size={14} />{/if}
      <span class="modal-title">{renaming ? 'Rename template' : 'New template'}</span>
      {#if info}<span class="kind">{info.title}</span>{/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="body" onkeydown={onKeydown}>
    <FormField
      label={kind === 'live' ? 'Abbreviation' : 'Name'}
      hint={renaming
        ? 'The file is renamed in your profile; a project that generates with it follows.'
        : 'Kept in your profile, so every project sees it — and opened in the editor, with a preview beside it.'}
      {error}
    >
      <Input bind:value={name} placeholder={kind === 'live' ? 'logd' : 'team-style'} autofocus />
    </FormField>
    {#if !renaming && picksLanguage}
      <!-- What it writes. It is not decoration: the file is named `<name>.<ext>.jinja`, which is how
           the template is coloured and — for an abbreviation — which files it is offered in. -->
      <FormField
        label="Writes"
        hint={kind === 'live'
          ? 'The language the abbreviation is offered in. Plain text is offered in every file.'
          : 'The language of the file this template writes.'}
      >
        <Select
          value={extension}
          options={languages.map((l) => ({ value: l.extension, label: l.label }))}
          size="sm"
          ariaLabel="Language the template writes"
          onchange={(next) => (language = next)}
        />
      </FormField>
    {/if}
    {#if !renaming && sources.length > 1}
      <FormField label="Start from">
        <Select
          value={source ?? ''}
          options={sources.map((t) => ({ value: t.name, label: t.description ? `${t.name} — ${t.description}` : t.name }))}
          size="sm"
          ariaLabel="Template to copy"
          onchange={(next) => (chosen = next)}
        />
      </FormField>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="ghost" onclick={onClose}>Cancel</Button>
      <Button variant="primary" loading={saving} disabled={!valid} onclick={() => void submit()}>
        {renaming ? 'Rename' : 'Create and open'}
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .body { display: flex; flex-direction: column; gap: 12px; }
  .kind { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>
