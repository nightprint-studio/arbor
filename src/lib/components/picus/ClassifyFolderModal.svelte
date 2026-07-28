<script lang="ts">
  /**
   * Say what a folder is — its engine and its purpose — without a mouse.
   *
   * The tree's row menu is the fast path when the folder is already on screen.
   * This is the other one: the folder is found by typing part of its path, the
   * two answers are picked with the arrows, and `Ctrl+Enter` writes them. It is
   * what the command palette opens, which is what makes classifying a verb of
   * the product rather than a gesture on a row.
   *
   * ## Both answers are three-valued
   *
   * "Inherit from above" is a real choice, not the absence of one: a wrong guess
   * has to be **clearable**, and clearing it is different from never having said
   * anything. The two selectors therefore always start on what this folder
   * *declares* — not on what it effectively is — because that is what pressing
   * Apply will write. What it effectively is, and where that comes from, is
   * stated underneath in words.
   *
   * ## And the engine has three answers, not two
   *
   * Oracle and PostgreSQL are what Picus reads; SQL Server, DB2, MySQL, MariaDB
   * and SQLite are engines it can only name. Saying "this is SQL Server" is a
   * real answer with real consequences — the folder is left alone from then on,
   * and never asked about again — so it is in the same picker as the other two
   * rather than hidden behind a second control. A `Select` and not a radio group
   * purely because there are now eight options and a row of eight radios is a
   * wall, not a choice.
   */
  import { Check, FolderCog, Search, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import PicusDialectChip from './PicusDialectChip.svelte';
  import PicusRoleChip from './PicusRoleChip.svelte';
  import {
    CLEAR_ID,
    ENGINE_CHOICES,
    ROLE_CHOICES,
    classifyFolder,
    engineChoiceLabel,
  } from './folder-classify';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import {
    DIALECTS,
    FOLDER_ROLE_LABELS,
    FOREIGN_ENGINES,
    declaredEngine,
    engineIsUnknown,
    folderEngine,
    isDialect,
    isForeignEngine,
    isGenericEngine,
    type FolderEngine,
    type FolderRole,
  } from '$lib/types/picus';

  let { path = '', onClose }: { path?: string; onClose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  const opened = picusProjectStore.entryFor(path);

  let query = $state('');
  // svelte-ignore state_referenced_locally
  let selectedPath = $state(path);
  let engine = $state<string>(opened ? declaredEngine(opened.node) ?? CLEAR_ID : CLEAR_ID);
  let role = $state<string>(opened?.node.role ?? CLEAR_ID);
  let listEl = $state<HTMLDivElement | undefined>();

  const needle = $derived(query.trim().toLowerCase());
  const visible = $derived(
    picusProjectStore.entries.filter((e) => !needle || e.node.path.toLowerCase().includes(needle)),
  );
  const selected = $derived(picusProjectStore.entryFor(selectedPath));

  const engineOptions = $derived([
    ...ENGINE_CHOICES.map((e) => ({ value: e as string, label: engineChoiceLabel(e) })),
    { value: CLEAR_ID, label: 'Inherit from above' },
  ]);
  const roleOptions = $derived([
    ...ROLE_CHOICES.map((r) => ({ value: r as string, label: FOLDER_ROLE_LABELS[r] })),
    { value: CLEAR_ID, label: 'Inherit' },
  ]);

  /** Where this folder currently stands, in words — the sentence Apply changes. */
  const standing = $derived.by(() => {
    const e = selected;
    if (!e) return '';
    const declared = declaredEngine(e.node) !== null;
    const from = declared ? 'declared here' : `inherited from ${e.dialectFrom}`;
    const effective = folderEngine(e.node);
    const engineText = isDialect(effective)
      ? `${DIALECTS[effective].short}, ${from}`
      : isGenericEngine(effective)
        ? `portable, ${from} — it runs on every engine and counts for all of them`
        : isForeignEngine(effective)
          ? `${FOREIGN_ENGINES[effective]} — not supported, ${from}; it is listed and left alone`
          : 'no engine — nothing is generated into it';
    const roleText = e.node.role !== null
      ? `${FOLDER_ROLE_LABELS[e.node.effectiveRole]}, declared here`
      : e.roleFrom
        ? `${FOLDER_ROLE_LABELS[e.node.effectiveRole]}, inherited from ${e.roleFrom}`
        : 'ignored — nobody gave it a purpose';
    return `Today: ${engineText}. Role: ${roleText}.`;
  });

  /**
   * Move to a folder and reset both selectors to what it **declares** — not to
   * what it effectively is. Apply writes the declaration, so the controls have
   * to start on it or the dialog would propose turning an inherited answer into
   * a copy of itself on every folder it touches.
   */
  function pick(folderPath: string) {
    selectedPath = folderPath;
    const entry = picusProjectStore.entryFor(folderPath);
    engine = entry ? declaredEngine(entry.node) ?? CLEAR_ID : CLEAR_ID;
    role = entry?.node.role ?? CLEAR_ID;
  }

  /** Arrows walk the filtered list from wherever focus is — including the search box. */
  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') { e.preventDefault(); void apply(); return; }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    if (!visible.length) return;
    e.preventDefault();
    const i = visible.findIndex((v) => v.node.path === selectedPath);
    const next = i < 0
      ? (e.key === 'ArrowDown' ? 0 : visible.length - 1)
      : (i + (e.key === 'ArrowDown' ? 1 : -1) + visible.length) % visible.length;
    pick(visible[next].node.path);
    listEl?.querySelector(`[data-path="${CSS.escape(visible[next].node.path)}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }

  let applying = $state(false);

  async function apply() {
    const entry = selected;
    if (!entry || applying) return;
    applying = true;
    const ok = await classifyFolder(entry, {
      dialect: engine === CLEAR_ID ? null : (engine as FolderEngine),
      role: role === CLEAR_ID ? null : (role as FolderRole),
    });
    applying = false;
    if (ok) onClose();
  }
</script>

<Modal {onClose} width="680px" height="580px" padBody={false} ariaLabel="Classify a folder">
  {#snippet header()}
    <ModalHeader {onClose}>
      <FolderCog size={14} />
      <span class="modal-title">Classify a folder</span>
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="cf" role="group" onkeydown={onKeydown}>
    <div class="cf-toolbar">
      <SearchBar
        bind:query
        showRegex={false}
        showCounter={false}
        placeholder="Filter folders by path…"
        ariaLabel="Filter folders"
        autofocus
      />
    </div>

    <div class="cf-list" bind:this={listEl}>
      {#if !visible.length}
        <div class="cf-empty">
          <Search size={22} strokeWidth={1.5} />
          <p>{picusProjectStore.folderCount ? `No folder matches “${query.trim()}”.` : 'No repository is open.'}</p>
        </div>
      {/if}

      {#each visible as entry (entry.node.path)}
        {@const folder = entry.node}
        <!-- The warning triangle is for the folders nobody could identify — the
             ones this dialog exists for. Never for an unsupported engine: that
             is answered, and a permanent warning on an answered row is how a
             warning stops meaning anything. -->
        {@const orphaned = folder.files.length > 0 && engineIsUnknown(folder)}
        <button
          class="cf-row"
          class:cf-selected={selectedPath === folder.path}
          data-path={folder.path}
          style:padding-left="{12 + entry.depth * 12}px"
          onclick={() => pick(folder.path)}
          ondblclick={() => void apply()}
        >
          {#if orphaned}
            <span class="cf-warn"><TriangleAlert size={12} /></span>
          {:else if selectedPath === folder.path}
            <span class="cf-tick"><Check size={12} /></span>
          {:else}
            <span class="cf-gap"></span>
          {/if}
          <span class="cf-name">{folder.name}</span>
          <span class="cf-path">{folder.path}</span>
          <span class="cf-spacer"></span>
          <PicusDialectChip
            engine={folderEngine(folder)}
            terse
            inherited={declaredEngine(folder) === null}
            from={entry.dialectFrom ?? ''}
          />
          <PicusRoleChip
            role={folder.effectiveRole}
            terse
            inherited={folder.role === null}
            from={entry.roleFrom ?? ''}
          />
          {#if folder.files.length}
            <Badge variant="count" size="sm" label={String(folder.files.length)} />
          {/if}
        </button>
      {/each}
    </div>

    <div class="cf-editor">
      {#if !selected}
        <StateBlock tone="info" fill={false} label="Pick a folder above — type part of its path, then use ↑ ↓." />
      {:else}
        <p class="cf-target">{selected.node.path}</p>
        <p class="cf-standing">{standing}</p>

        <FormField
          label="Engine"
          hint="Which database this folder's scripts are written for. Picus reads and generates Oracle and PostgreSQL; naming any other engine says these scripts are not its business — they are listed, never parsed, and never asked about again. Inherit takes the answer from the nearest folder above that has one."
        >
          <Select bind:value={engine} options={engineOptions} />
        </FormField>

        <FormField label="Role" hint="What the scripts here are for — it decides how generated SQL is written into them.">
          <RadioGroup bind:value={role} options={roleOptions} size="sm" />
        </FormField>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <span class="cf-foot">
      Applying writes the repository's own configuration, so everyone opening this folder
      reads it the same way. Everything below inherits it until a folder says otherwise.
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
  .modal-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .cf { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .cf-toolbar { flex-shrink: 0; padding: 12px 16px; border-bottom: 1px solid var(--border-subtle); }
  .cf-list { flex: 1; min-height: 0; overflow-y: auto; padding: 6px 0; }

  .cf-row {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 4px 16px 4px 12px;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .cf-row:hover { background: var(--bg-hover); }
  .cf-selected { background: var(--accent-subtle); }

  .cf-tick { display: inline-flex; color: var(--accent); flex-shrink: 0; }
  /* Scripts nobody gave an engine to: the rows this dialog exists for. */
  .cf-warn { display: inline-flex; color: var(--warning); flex-shrink: 0; }
  .cf-gap { width: 12px; flex-shrink: 0; }

  .cf-name { font-size: 12px; white-space: nowrap; }
  .cf-path {
    font-family: var(--font-code);
    font-size: 10px;
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
  .cf-target { font-family: var(--font-code); font-size: 11.5px; color: var(--text-primary); }
  .cf-standing { font-size: 11px; line-height: 1.5; color: var(--text-muted); }

  .cf-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 40px 16px;
    color: var(--text-disabled);
  }
  .cf-empty p { margin: 0; font-size: 12px; }

  .cf-foot {
    flex: 1;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-muted);
    text-align: left;
  }
</style>
