<script lang="ts">
  /**
   * "…and every other thing called POS" — the second decision, and visibly a
   * second one.
   *
   * The folder or file the user classified is **already saved** by the time this
   * appears. That is the property that makes it safe to offer at all: cancelling
   * costs them nothing they just did, so the dialog can afford to ask a bigger
   * question than the one they answered.
   *
   * And it is a bigger question. Declaring what one folder is touches one folder;
   * declaring what a name means touches every folder of that name and every one
   * added later. A user who reached the second by pressing a button that named
   * the first would be right to feel misled — hence a dialog rather than a
   * checkbox on the classify form.
   *
   * ## Why this is not a ConfirmModal
   *
   * Because the offer now has a *shape*: a name can be looked for in folder
   * names, in file names, or in both, and file names are opt-in for a reason
   * worth stating on screen rather than assuming. A yes/no confirmation cannot
   * carry that choice, and putting the choice anywhere else would mean deciding
   * the blast radius somewhere other than where it is agreed to.
   *
   * The counts are the honest part: each scope says how many things it reaches
   * before it is picked, and both come from the backend by the rule the alias
   * itself will use — a number worked out here would be a second implementation
   * of the matching rule, and the offer is only safe to accept without looking
   * because the number is true. The paths are listed as well as totalled, so a
   * surprising count can be checked rather than believed.
   */
  import { FolderTree, FileCode2, Layers, Tags } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusUiStore, type AliasOffer } from '$lib/stores/picus/ui.svelte';
  import {
    ALIAS_SCOPE_LABELS,
    FOLDER_ROLE_LABELS,
    engineLabel,
    type AliasScope,
  } from '$lib/types/picus';

  let { offer, onClose }: { offer: AliasOffer; onClose: () => void } = $props();

  /**
   * Which word the rule is about.
   *
   * A folder's name is the whole name and there is nothing to choose. A file's
   * name is a sentence, so the word carrying the meaning was a guess — and a
   * guess the user cannot correct is a guess they have to decline.
   */
  // svelte-ignore state_referenced_locally
  let word = $state(offer.name);

  /**
   * Where the name is looked for. Defaults to the axis the user was actually
   * working on, never wider: classifying a folder proposes folders, classifying
   * a file proposes files. "Both" is always one keystroke away and never assumed.
   */
  // A plain string because that is what `RadioGroup` binds; read back through
  // `scope` below, which is the typed view of it.
  // svelte-ignore state_referenced_locally
  let scopeChoice = $state<string>(offer.kind === 'file' ? 'files' : 'folders');
  const scope = $derived(scopeChoice as AliasScope);

  const wordOptions = $derived(
    [offer.name, ...(offer.alternatives ?? [])].map((w) => ({ value: w, label: w })),
  );

  /**
   * What the word reaches, on each axis.
   *
   * Both answered by the **backend**, by the same rule the alias itself will
   * use, so the number in the offer is the number the rule produces rather than
   * a second implementation of it. That property is the whole reason this offer
   * is safe to accept without looking: `POS` matching `01_POS` but not
   * `POSIZIONI` — and, for files, matching the stem so `.sql` cannot match an
   * alias called `SQL` — are load-bearing rules, and a copy of one drifts.
   *
   * Re-asked when the word changes; the lists fill in behind the dialog and
   * nothing waits on them.
   */
  let folderPaths = $state<string[]>(offer.folderPaths);
  let filePaths = $state<string[]>(offer.filePaths ?? []);

  $effect(() => {
    const asked = word;
    if (asked === offer.name) {
      folderPaths = offer.folderPaths;
      filePaths = offer.filePaths ?? [];
      return;
    }
    let live = true;
    void picusProjectStore.foldersNamed(asked).then((paths) => {
      if (live) folderPaths = paths;
    });
    void picusProjectStore.filesNamed(asked).then((paths) => {
      if (live) filePaths = paths;
    });
    return () => { live = false; };
  });

  const scopeOptions = $derived([
    {
      value: 'folders',
      label: `${ALIAS_SCOPE_LABELS.folders} · ${folderPaths.length}`,
      icon: FolderTree,
      description: 'Only directories whose name contains this word.',
    },
    {
      value: 'files',
      label: `${ALIAS_SCOPE_LABELS.files} · ${filePaths.length}`,
      icon: FileCode2,
      // A role is a fact about a directory, so a rule that carries only a role
      // and is pointed at file names would classify nothing at all — two
      // correct-looking lines that together do nothing. Not offered.
      disabled: !offer.engine,
      description: offer.engine
        ? 'Only scripts whose file name contains it.'
        : 'Not available: a role belongs to a folder, so a file-only rule would say nothing.',
    },
    {
      value: 'both',
      label: ALIAS_SCOPE_LABELS.both,
      icon: Layers,
      description: 'Directories and scripts alike.',
    },
  ]);

  /** What the user just declared, in their words. */
  const said = $derived(
    [
      offer.engine ? engineLabel(offer.engine) : null,
      offer.role ? FOLDER_ROLE_LABELS[offer.role] : null,
    ].filter(Boolean).join(' · '),
  );

  const reached = $derived(
    scope === 'folders' ? folderPaths
      : scope === 'files' ? filePaths
        : [...folderPaths, ...filePaths],
  );

  const question = $derived.by(() => {
    if (offer.kind === 'file') {
      const rest = Math.max(filePaths.length - 1, 0);
      return rest === 1
        ? `One other script has ${word} in its name — should it mean the same thing?`
        : `${rest} other scripts have ${word} in their name — should they all mean the same thing?`;
    }
    const rest = Math.max(folderPaths.length - 1, 0);
    return rest === 1
      ? `One other folder is called ${word} — should it mean the same thing?`
      : `${rest} other folders are called ${word} — should they all mean the same thing?`;
  });

  /** The paths, capped: a list nobody can read is not evidence. */
  const otherPaths = $derived(reached.filter((p) => p !== offer.origin));
  const shown = $derived(otherPaths.slice(0, 8));
  const notShown = $derived(Math.max(otherPaths.length - shown.length, 0));

  let applying = $state(false);

  async function apply() {
    if (applying || !word.trim()) return;
    applying = true;
    // A role is a fact about a directory, so it never travels with a file-only
    // rule — the backend reports an alias that tries as one that classifies
    // nothing, and it is right to.
    const role = scope === 'files' ? null : offer.role;
    const message = await picusProjectStore.setAlias(word.trim(), offer.engine, role, scope);
    applying = false;
    picusUiStore.closeAlias();
    if (message) {
      toastStore.show(`${word} could not be declared — ${message}`, 'error');
      return;
    }
    toastStore.show(
      `${word} now means ${said} in this repository — ${reached.length} `
        + `${scope === 'folders' ? 'folder(s)' : scope === 'files' ? 'file(s)' : 'folder(s) and file(s)'}, `
        + 'and anything added later.',
      'success',
    );
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void apply(); }
  }
</script>

<Modal {onClose} width="560px" ariaLabel={`Declare what ${word} means`}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Tags size={14} />
      <span class="modal-title">Say it once, for the name</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="ao" role="group" onkeydown={onKeydown}>
    <p class="ao-message">
      <code>{offer.origin}</code> is {said}. {question}
    </p>

    {#if offer.alternatives?.length}
      <FormField
        label="Which word carries the meaning"
        hint="A file name is a sentence, so Picus proposes the word that recurs across the most scripts — correct it if it guessed the wrong one. Numbers are never offered."
      >
        <Select bind:value={word} options={wordOptions} />
      </FormField>
    {/if}

    <FormField
      label="Where the name applies"
      hint="Matched as a whole word and case-insensitively, so POS matches POS, 01_POS and POS_2024 — and never POSIZIONI. File names are the riskier half: there are far more of them and they read as sentences, which is why they are never assumed."
    >
      <RadioGroup bind:value={scopeChoice} options={scopeOptions} appearance="card" size="sm" block />
    </FormField>

    {#if shown.length}
      <div class="ao-reach">
        <p class="ao-reach-head">What it reaches today</p>
        <pre class="ao-paths">{shown.join('\n')}{notShown ? `\n…and ${notShown} more` : ''}</pre>
      </div>
    {/if}

    <p class="ao-note">
      Anything of this name added later is classified the same way, without touching the
      configuration again. A folder or file that declares its own engine keeps it — a
      specific answer still beats the rule.
      {#if picusProjectStore.configPath}
        Saved with the repository, in {picusProjectStore.configPath}.
      {/if}
    </p>
  </div>

  {#snippet footer()}
    <span class="ao-foot">
      {offer.kind === 'file' ? 'The script' : 'The folder'} you just classified stays as you set it
      either way.
    </span>
    <Button variant="ghost" size="sm" onclick={onClose}>
      Just this {offer.kind === 'file' ? 'script' : 'folder'}
    </Button>
    <Button
      variant="primary"
      size="sm"
      disabled={applying || !reached.length}
      loading={applying}
      tooltip={{ content: 'Declare this name with the repository', shortcut: 'Ctrl+Enter' }}
      onclick={() => void apply()}
    >
      Apply to {reached.length}
    </Button>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .ao { display: flex; flex-direction: column; gap: 12px; }

  .ao-message { margin: 0; font-size: var(--font-size-sm); line-height: 1.55; color: var(--text-primary); }
  .ao-message code { font-family: var(--font-code); font-size: 11.5px; color: var(--text-secondary); }

  .ao-reach { display: flex; flex-direction: column; gap: 4px; }
  .ao-reach-head { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); }
  .ao-paths {
    margin: 0;
    max-height: 132px;
    overflow-y: auto;
    padding: 7px 9px;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-input);
    font-family: var(--font-code);
    font-size: 10.5px;
    line-height: 1.6;
    color: var(--text-secondary);
    white-space: pre;
  }

  .ao-note { margin: 0; font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }

  .ao-foot { flex: 1; font-size: 11px; line-height: 1.45; color: var(--text-muted); text-align: left; }
</style>
