<script lang="ts">
  /**
   * Settings › Code Templates — one kind's templates, and what is done to them: read one, copy it,
   * rename it, make it the project's, delete it.
   *
   * **A list of names is not a list of templates.** What tells `team-builder` from `builder` is what
   * they write, so the page is a list beside the thing itself: pick a row and its body is there,
   * coloured as the language it generates, with everything the row could not say — what the project
   * requires of it, the word that expands it, where its file is.
   *
   * With no `kind` it is the overview instead: the six kinds, what each generates, and how many
   * templates you have of it. The tree has a page per kind under it, and this is the way in.
   */
  import { onMount } from 'svelte';
  import { Copy, Eye, FilePlus2, PenLine, Pin, TextCursorInput, Trash2 } from 'lucide-svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import InlineEdit from '$lib/components/shared/ui/InlineEdit.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { highlightCode } from '$lib/utils/highlight';
  import { jinjaPrismLanguage } from '$lib/utils/jinja-words';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuTemplatesStore as templates } from '$lib/stores/bennu/templates.svelte';
  import { templateText, type KindTemplates, type TemplateInfo, type TemplateKindId } from '$lib/ipc/bennu/templates';
  import { languageOf, TEMPLATE_KIND_ICONS } from './template-kinds';
  import BennuNewTemplateModal from './BennuNewTemplateModal.svelte';

  let {
    kind = null,
    onOpenKind,
  }: {
    kind?: TemplateKindId | null;
    /** The overview's way into a kind's own page — the tree's child entry, from here. */
    onOpenKind?: (kind: TemplateKindId) => void;
  } = $props();

  onMount(() => { void templates.load(); });

  let creating = $state<{ kind: TemplateKindId; from: string | null } | null>(null);
  let renaming = $state<{ kind: TemplateKindId; name: string } | null>(null);
  let deleting = $state<{ kind: TemplateKindId; name: string } | null>(null);
  /** The abbreviation whose trigger word is being typed. */
  let editingWord = $state(false);
  let wordDraft = $state('');
  /** The template being read, by name. */
  let picked = $state<string | null>(null);
  /** Its body, and the name it was read for — so a stale answer never lands under another template. */
  let body = $state<{ name: string; text: string } | null>(null);

  const hasProject = $derived(!!projectStore.project);
  /** The kinds worth showing: a Cargo workspace has no Java class to generate from, so the four kinds
   *  that read one are not listed rather than listed and unusable. */
  const kinds = $derived(templates.all.filter((g) => !(g.java_only && projectStore.isCargo)));
  const group = $derived<KindTemplates | null>(kind ? kinds.find((g) => g.kind === kind) ?? null : null);
  const list = $derived(group?.templates ?? []);

  // The selection follows the list: the first template when nothing is picked, and back to the first
  // when the one being read has just been deleted or renamed out from under it.
  $effect(() => {
    if (!list.length) { picked = null; return; }
    if (!picked || !list.some((t) => t.name === picked)) picked = list[0].name;
  });

  const current = $derived<TemplateInfo | null>(list.find((t) => t.name === picked) ?? null);
  const language = $derived(languageOf(current?.extension ?? ''));
  /** The grammar its body is coloured with — the template's tags over the language it writes. */
  const grammar = $derived(jinjaPrismLanguage(`x.${current?.extension ?? ''}.jinja`));

  // Read the body of whatever is selected. Keyed on the kind and the name, so switching pages or
  // picking another row re-reads, and a rename (which changes the name) does too.
  $effect(() => {
    const name = picked;
    const of = kind;
    if (!of || !name) { body = null; return; }
    void templateText(of, name)
      .then((text) => { if (picked === name && kind === of) body = { name, text }; })
      .catch(() => { if (picked === name && kind === of) body = { name, text: '' }; });
  });

  const shown = $derived(body && body.name === picked ? body.text : '');

  /** The word that expands an abbreviation: its own, or its file name when it has not been given
   *  one. Abbreviations only — for every other kind the name is a label, not a trigger. */
  const wordOf = (t: TemplateInfo) => t.abbrev ?? t.name;

  async function commitWord(t: TemplateInfo, next: string) {
    editingWord = false;
    // Back to the file name is said by clearing it, so the two never drift into disagreeing.
    await templates.setAbbrev(t.name, next === t.name ? '' : next);
  }
  function validWord(v: string): string | null {
    return /^\w+$/.test(v) ? null : 'Letters, digits and “_”.';
  }

  /** Arrow keys walk the list, as they do in every other list in Bennu. */
  function onListKeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const at = list.findIndex((t) => t.name === picked);
    const next = e.key === 'ArrowDown' ? at + 1 : at - 1;
    if (next >= 0 && next < list.length) picked = list[next].name;
  }
</script>

{#if !kind}
  <!-- ── The overview ───────────────────────────────────────────────────────── -->
  <div class="section-header">
    <h2>Code Templates</h2>
    <p>
      Everything Bennu writes for you comes out of a template, and every one of them can be copied and
      changed. Each kind has a page of its own.
    </p>
  </div>
  {#if kinds.length === 0}
    <EmptyState compact message="Reading the templates…" />
  {/if}
  <div class="kinds">
    {#each kinds as g (g.kind)}
      {@const KindIcon = TEMPLATE_KIND_ICONS[g.kind]}
      {@const mine = g.templates.filter((t) => t.origin === 'global').length}
      <button class="kind" type="button" onclick={() => onOpenKind?.(g.kind)}>
        <span class="kind-head">
          <KindIcon size={14} />
          <span class="kind-title">{g.title}</span>
          <span class="kind-count">{mine ? `${mine} of yours` : 'built-in only'}</span>
        </span>
        <span class="kind-desc">{g.description}</span>
        {#if hasProject && g.project}
          <span class="kind-project">This project generates with <code>{g.project}</code></span>
        {/if}
      </button>
    {/each}
  </div>
{:else if !group}
  <EmptyState compact message="Reading the templates…" />
{:else}
  <!-- ── One kind ───────────────────────────────────────────────────────────── -->
  <div class="section-header">
    <h2>{group.title}</h2>
    <p>{group.description}</p>
  </div>

  <div class="card tpl">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <ul class="tpl-list" onkeydown={onListKeydown}>
      {#each list as t (t.name)}
        {@const tone = languageOf(t.extension)}
        <li>
          <button class="tpl-row" class:picked={t.name === picked} type="button" onclick={() => (picked = t.name)}>
            <span class="tpl-row-main">
              <span class="tpl-name">{t.name}</span>
              {#if t.description}<span class="tpl-row-desc">{t.description}</span>{/if}
            </span>
            <span class="pill pill-{tone.tone}">{tone.label}</span>
            {#if hasProject && group.project === t.name}
              <span class="dot" use:tooltip={'This project generates with it'} aria-label="This project"></span>
            {/if}
          </button>
        </li>
      {/each}
      <li class="tpl-new">
        <Button variant="ghost" size="xs" onclick={() => (creating = { kind: group.kind, from: picked })}>
          {#snippet iconStart()}<FilePlus2 size={12} />{/snippet}
          New template…
        </Button>
      </li>
    </ul>

    <div class="tpl-detail">
      {#if !current}
        <EmptyState compact message="No templates of this kind." />
      {:else}
        <!-- Bound once: the handlers below are closures, and a narrowed `current` does not survive
             into one — inside them it is `TemplateInfo | null` again. -->
        {@const t = current}
        <div class="det-head">
          <span class="det-name">{t.name}<span class="det-ext">.{t.extension || 'txt'}</span></span>
          <span class="pill pill-{language.tone}">{language.label}</span>
          {#if t.origin === 'builtin'}
            <span
              use:tooltip={t.starter
                ? 'A starting point to copy — Bennu never generates with it'
                : 'Ships with Bennu: read it, copy it, but it cannot be changed'}
            >
              <Badge variant="tone" tone="neutral" label={t.starter ? 'Starter' : 'Built-in'} />
            </span>
          {/if}
          {#if hasProject && group.project === t.name}
            <Badge variant="tone" tone="accent" label="This project" />
          {/if}
          {#if t.unmet}
            <!-- Its `bennu.requires`, unmet here: the reason it is missing from this project's lists. -->
            <Badge variant="tone" tone="warning" label={t.unmet} />
          {/if}
        </div>

        {#if t.description}<p class="det-desc">{t.description}</p>{/if}

        {#if group.kind === 'live'}
          <!-- What you actually type. Its own thing rather than the file name, because a name that has
               to be a trigger word can be neither: `logd` says nothing in a list, and
               `logger-for-this-class` is not a word anybody types. -->
          <div class="det-row">
            <span class="det-k">Expands on</span>
            {#if editingWord}
              <span class="det-word-edit">
                <InlineEdit
                  bind:value={wordDraft}
                  placeholder="logd"
                  validate={validWord}
                  onconfirm={(v) => void commitWord(t, v)}
                  oncancel={() => (editingWord = false)}
                />
              </span>
            {:else}
              <button
                class="det-word"
                type="button"
                disabled={t.origin !== 'global'}
                onclick={() => { wordDraft = wordOf(t); editingWord = true; }}
                use:tooltip={t.origin === 'global'
                  ? 'The word that expands it — click to change it'
                  : 'The word that expands it'}
              >{wordOf(t)}</button>
            {/if}
          </div>
        {/if}

        {#if t.requires.length}
          <div class="det-row">
            <span class="det-k">Needs</span>
            <span class="det-requires">{t.requires.join(', ')}</span>
          </div>
        {/if}

        <div class="det-actions">
          {#if hasProject && !t.starter && group.project !== t.name}
            <Button variant="ghost" size="sm" onclick={() => void templates.setProject(group.kind, t.name)}>
              {#snippet iconStart()}<Pin size={12} />{/snippet}
              Use for this project
            </Button>
          {/if}
          <Button variant="ghost" size="sm" onclick={() => void templates.edit(group.kind, t.name)}>
            {#snippet iconStart()}{#if t.path}<PenLine size={12} />{:else}<Eye size={12} />{/if}{/snippet}
            {t.path ? 'Open in the editor' : 'Open read-only'}
          </Button>
          <Button variant="ghost" size="sm" onclick={() => (creating = { kind: group.kind, from: t.name })}>
            {#snippet iconStart()}<Copy size={12} />{/snippet}
            Copy
          </Button>
          {#if t.origin === 'global'}
            <span class="det-spacer"></span>
            <IconButton tooltip="Rename" size={24} onclick={() => (renaming = { kind: group.kind, name: t.name })}>
              <TextCursorInput size={12} />
            </IconButton>
            <IconButton tooltip="Delete" variant="danger" size={24} onclick={() => (deleting = { kind: group.kind, name: t.name })}>
              <Trash2 size={12} />
            </IconButton>
          {/if}
        </div>

        <!-- What it writes. The whole reason this pane exists. -->
        <pre class="det-body"><code>{@html highlightCode(shown, grammar)}</code></pre>

        <p class="det-path">
          {#if t.path}{t.path}{:else}Built into Bennu — copy it to change it.{/if}
        </p>
      {/if}
    </div>
  </div>

  <p class="tpl-dir">Yours are kept in <code>{group.directory}</code></p>
{/if}

{#if creating}
  <BennuNewTemplateModal
    kind={creating.kind}
    from={creating.from}
    onClose={() => (creating = null)}
    onCreated={(name) => (picked = name)}
  />
{/if}

{#if renaming}
  <BennuNewTemplateModal
    kind={renaming.kind}
    rename={renaming.name}
    onClose={() => (renaming = null)}
    onCreated={(name) => (picked = name)}
  />
{/if}

{#if deleting}
  <ConfirmModal
    title="Delete template"
    message={`Delete “${deleting.name}”?`}
    detail="The file is removed from your profile. A project that generated with it goes back to the kind's first template."
    variant="danger"
    confirmLabel="Delete"
    onConfirm={() => {
      const target = deleting;
      deleting = null;
      if (target) void templates.remove(target.kind, target.name);
    }}
    onCancel={() => (deleting = null)}
  />
{/if}

<style>
  /* ── The overview ───────────────────────────────────────────────────────── */
  .kinds { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 8px; }
  .kind {
    display: flex; flex-direction: column; gap: 5px; text-align: left;
    padding: 11px 13px; cursor: pointer;
    background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-md);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .kind:hover { border-color: var(--accent); background: var(--bg-hover); }
  .kind-head { display: flex; align-items: center; gap: 7px; color: var(--accent); }
  .kind-title { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); flex: 1; }
  .kind-count { font-size: var(--font-size-2xs); color: var(--text-muted); }
  .kind-desc { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.5; }
  .kind-project { font-size: var(--font-size-2xs); color: var(--text-secondary); }
  .kind-project code { font-family: var(--font-code); color: var(--accent); }

  /* ── One kind: the list beside the template ─────────────────────────────── */
  .tpl { display: grid; grid-template-columns: minmax(180px, 240px) 1fr; min-height: 340px; }
  .tpl-list {
    display: flex; flex-direction: column; gap: 1px; margin: 0; padding: 6px;
    list-style: none; overflow-y: auto;
    background: var(--bg-base); border-right: 1px solid var(--border);
  }
  .tpl-row {
    display: flex; align-items: center; gap: 6px; width: 100%;
    padding: 6px 8px; text-align: left; cursor: pointer;
    background: transparent; border: none; border-radius: var(--radius-sm);
    transition: background var(--transition-fast);
  }
  .tpl-row:hover:not(.picked) { background: var(--bg-hover); }
  .tpl-row.picked { background: var(--accent-subtle); }
  .tpl-row-main { display: flex; flex-direction: column; min-width: 0; flex: 1; }
  .tpl-name { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary); }
  .tpl-row.picked .tpl-name { color: var(--accent); font-weight: 600; }
  .tpl-row-desc {
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  /* The project's own, as a dot: a badge here would be wider than the name it belongs to. */
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); flex-shrink: 0; }
  .tpl-new { margin-top: 4px; padding: 2px; }

  /* The language, in three families and a neutral — see `template-kinds.ts`. */
  .pill {
    flex-shrink: 0; padding: 1px 6px; border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs); font-weight: 600; letter-spacing: 0.01em;
  }
  .pill-warning { color: var(--warning); background: color-mix(in srgb, var(--warning) 15%, transparent); }
  .pill-danger  { color: var(--error);   background: color-mix(in srgb, var(--error) 15%, transparent); }
  .pill-info    { color: var(--info);    background: color-mix(in srgb, var(--info) 15%, transparent); }
  .pill-success { color: var(--success); background: color-mix(in srgb, var(--success) 15%, transparent); }
  .pill-neutral { color: var(--text-muted); background: var(--bg-overlay); }

  /* ── The template itself ────────────────────────────────────────────────── */
  .tpl-detail { display: flex; flex-direction: column; gap: 9px; padding: 12px 14px; min-width: 0; }
  .det-head { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }
  .det-name { font-family: var(--font-code); font-size: var(--font-size-sm); color: var(--text-primary); }
  .det-ext { color: var(--text-muted); }
  .det-desc { margin: 0; font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.5; }
  .det-row { display: flex; align-items: center; gap: 8px; font-size: var(--font-size-xs); }
  .det-k { color: var(--text-muted); }
  .det-requires { color: var(--text-secondary); font-family: var(--font-code); font-size: var(--font-size-2xs); }
  .det-word {
    padding: 1px 7px; cursor: pointer;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--accent);
    background: var(--accent-subtle); border: 1px solid transparent; border-radius: var(--radius-sm);
    transition: border-color var(--transition-fast);
  }
  .det-word:hover:not(:disabled) { border-color: var(--accent); }
  .det-word:disabled { color: var(--text-muted); background: var(--bg-overlay); cursor: default; }
  .det-word-edit { width: 150px; }
  .det-actions { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
  .det-spacer { flex: 1; }
  .det-body {
    flex: 1; min-height: 120px; max-height: 320px; margin: 0; padding: 9px 11px;
    overflow: auto; white-space: pre;
    background: var(--bg-base); border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
    font-family: var(--font-code); font-size: var(--font-size-2xs); line-height: 1.55;
    color: var(--text-primary); user-select: text;
  }
  .det-path {
    margin: 0; font-size: var(--font-size-2xs); color: var(--text-disabled);
    overflow-wrap: anywhere; font-family: var(--font-code);
  }

  .tpl-dir { margin: 0; font-size: var(--font-size-xs); color: var(--text-muted); overflow-wrap: anywhere; }
  .tpl-dir code { font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-secondary); }
</style>
