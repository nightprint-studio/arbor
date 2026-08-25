<script lang="ts">
  /**
   * The folder names this repository has given a meaning — reviewed and edited
   * in one place.
   *
   * A rule that only ever gets *added* in the moment, from a confirmation
   * attached to something else, is a rule nobody can audit later. This is where
   * "why is POS reading as PostgreSQL?" is answered, and where an answer that
   * turned out to be wrong is changed or removed.
   *
   * It lives under **Project** rather than anywhere else because that is what it
   * is: a fact about the repository, saved with the repository, inherited by
   * whoever opens it next — not a preference of the person looking at it.
   *
   * Every row shows how many folders it currently reaches, because a rule whose
   * blast radius is invisible is a rule people are afraid to touch.
   */
  import { Ban, Database, Plus, Trash2 } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { CLEAR_ID, ENGINE_CHOICES, ROLE_CHOICES, engineChoiceLabel } from '../engine-choices';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    ALIAS_SCOPE_CHOICES,
    ALIAS_SCOPE_LABELS,
    FOLDER_ROLE_LABELS,
    aliasScope,
    isDialect,
    scopeCoversFolders,
    type AliasScope,
    type FolderAlias,
    type FolderEngine,
    type FolderRole,
  } from '$lib/types/picus';

  const NONE = CLEAR_ID;

  const engineOptions = [
    { value: NONE, label: '— says nothing about the engine —' },
    ...ENGINE_CHOICES.map((e) => ({ value: e as string, label: engineChoiceLabel(e) })),
  ];
  const roleOptions = [
    { value: NONE, label: '— says nothing about the role —' },
    ...ROLE_CHOICES.map((r) => ({ value: r as string, label: FOLDER_ROLE_LABELS[r] })),
  ];
  /**
   * Where the name is looked for. Third control rather than a checkbox because
   * "files only" is a real answer: a repository whose engine markers live purely
   * in file names has no folders of that name to classify.
   */
  const scopeOptions = ALIAS_SCOPE_CHOICES.map((s) => ({ value: s as string, label: ALIAS_SCOPE_LABELS[s] }));

  const aliases = $derived(picusProjectStore.aliases);
  const attached = $derived(picusProjectStore.attached);

  /**
   * How many folders each name currently reaches.
   *
   * Asked of the backend, by the same rule the alias itself uses. Re-deriving it
   * here from folder names would mean a second implementation of "whole word,
   * case-insensitively" — and the moment the two disagree, this screen is quietly
   * lying about what the rule does, which is the one thing a review screen must
   * never do. A handful of names means a handful of cheap calls; the numbers fill
   * in behind the rows and nothing waits on them.
   */
  let reach = $state<Record<string, number>>({});

  $effect(() => {
    const names = aliases.map((a) => a.name);
    if (!names.length) { reach = {}; return; }
    let live = true;
    void Promise.all(names.map((name) => picusProjectStore.foldersNamed(name))).then((results) => {
      if (!live) return;
      reach = Object.fromEntries(names.map((name, i) => [name, results[i].length]));
    });
    return () => { live = false; };
  });

  let busy = $state('');
  let newName = $state('');
  let newEngine = $state<string>(NONE);
  let newRole = $state<string>(NONE);
  /** A plain string because that is what `Select` binds; cast where it is used. */
  let newScope = $state<string>('folders');

  function engineOf(alias: FolderAlias): string { return alias.engine ?? NONE; }
  function roleOf(alias: FolderAlias): string { return alias.role ?? NONE; }

  /**
   * Every field of an alias is **replaced**, never merged — so each of the three
   * controls sends the other two as they stand. Letting one of them default
   * instead is how editing an engine would quietly move a file-matching rule
   * back to folders only.
   */
  async function write(
    name: string,
    engine: string,
    role: string,
    scope: AliasScope,
    what: string,
  ): Promise<boolean> {
    busy = name;
    const message = await picusProjectStore.setAlias(
      name,
      engine === NONE ? null : (engine as FolderEngine),
      role === NONE ? null : (role as FolderRole),
      scope,
    );
    busy = '';
    if (message) {
      toastStore.show(`${name} — ${message}`, 'error');
      return false;
    }
    toastStore.show(what, 'success');
    return true;
  }

  async function save(alias: FolderAlias, engine: string, role: string, scope: AliasScope) {
    const both = [
      engine === NONE ? null : engineChoiceLabel(engine as FolderEngine),
      role === NONE ? null : FOLDER_ROLE_LABELS[role as FolderRole],
    ].filter(Boolean);
    if (!both.length) { await remove(alias); return; }
    const where = scope === 'files' ? 'file' : scope === 'both' ? 'folder and file' : 'folder';
    await write(
      alias.name,
      engine,
      role,
      scope,
      `Every ${where} named ${alias.name} → ${both.join(' · ')}.`,
    );
  }

  async function remove(alias: FolderAlias) {
    busy = alias.name;
    const message = await picusProjectStore.removeAlias(alias.name);
    busy = '';
    if (message) { toastStore.show(`${alias.name} — ${message}`, 'error'); return; }
    toastStore.show(`${alias.name} no longer means anything in this project.`, 'success');
  }

  async function add() {
    const name = newName.trim();
    if (!name || (newEngine === NONE && newRole === NONE)) return;
    const ok = await write(
      name,
      newEngine,
      newRole,
      newScope as AliasScope,
      `Everything named ${name} is now classified.`,
    );
    if (!ok) return;
    newName = '';
    newEngine = NONE;
    newRole = NONE;
    newScope = 'folders';
  }

  const canAdd = $derived(!!newName.trim() && (newEngine !== NONE || newRole !== NONE));
</script>

<div class="section-header">
  <h2>Folder names</h2>
  <p>
    What a <b>name</b> means in this repository. Picus knows the names that mean the same
    thing everywhere — <code>ORACLE</code>, <code>ORA</code>, <code>POSTGRES</code>,
    <code>MSSQL</code> — and deliberately not the ones that do not: <code>POS</code> is
    PostgreSQL in your repository and <code>POSIZIONI</code> in somebody else's.
  </p>
  <p>
    Each name is looked for in folder names by default. <b>File names</b> are opt-in per
    name, for the repositories whose engine markers live there instead —
    <code>4_12_ORA.sql</code> beside <code>4_12_POS.sql</code> in one directory. They are
    not the default because a file name is a sentence, and there are hundreds of them to a
    dozen folder names. A <b>role</b> always stays a fact about a folder.
  </p>
</div>

{#if !attached}
  <StateBlock
    tone="info"
    fill={false}
    label="No repository is attached. Folder names belong to a script repository — attach one to a connection and its vocabulary lives here."
  />
{:else}
  <div class="card">
    {#if !aliases.length}
      <p class="pa-empty">
        This repository declares no names of its own yet. Classify a folder whose name repeats and
        Picus offers to make it a rule — or add one below.
      </p>
    {/if}

    {#each aliases as alias (alias.name)}
      {@const count = reach[alias.name]}
      {@const scope = aliasScope(alias)}
      <div class="pa-row">
        <span class="pa-name" use:tooltip={'Matched as a whole word, case-insensitively'}>{alias.name}</span>
        <span class="pa-icon">
          {#if alias.engine && isDialect(alias.engine as FolderEngine)}
            <Database size={12} />
          {:else if alias.engine}
            <Ban size={12} />
          {/if}
        </span>
        <Select
          value={engineOf(alias)}
          options={engineOptions}
          disabled={busy === alias.name}
          onchange={(v) => void save(alias, v, roleOf(alias), scope)}
        />
        <Select
          value={roleOf(alias)}
          options={roleOptions}
          disabled={busy === alias.name}
          onchange={(v) => void save(alias, engineOf(alias), v, scope)}
        />
        <Select
          value={scope}
          options={scopeOptions}
          disabled={busy === alias.name}
          onchange={(v) => void save(alias, engineOf(alias), roleOf(alias), v as AliasScope)}
        />
        <!-- A rule whose blast radius is invisible is a rule people are afraid
             to touch. Blank until the count arrives, never a placeholder zero —
             and absent entirely for a file-only rule, because the number that
             would go here counts folders and would read as "this does nothing". -->
        <span class="pa-reach">
          {#if count !== undefined && scopeCoversFolders(scope)}
            <Badge
              variant="tone"
              tone={count === 0 ? 'warning' : 'neutral'}
              size="sm"
              label={count === 1 ? '1 folder' : `${count} folders`}
            />
          {/if}
        </span>
        <Button
          variant="icon"
          size="xs"
          tooltip="Forget this name — the folders go back to being read by their own names"
          ariaLabel={`Remove the alias ${alias.name}`}
          disabled={busy === alias.name}
          onclick={() => void remove(alias)}
        >
          {#snippet iconStart()}<Trash2 size={12} />{/snippet}
        </Button>
      </div>
    {/each}

    <div class="pa-row pa-new">
      <Input
        value={newName}
        placeholder="POS"
        ariaLabel="Folder name"
        oninput={(v) => (newName = v)}
        onkeydown={(e) => { if (e.key === 'Enter' && canAdd) void add(); }}
      />
      <span class="pa-icon"></span>
      <Select bind:value={newEngine} options={engineOptions} />
      <Select bind:value={newRole} options={roleOptions} />
      <Select bind:value={newScope} options={scopeOptions} />
      <span></span>
      <Button variant="secondary" size="xs" disabled={!canAdd} onclick={() => void add()}>
        {#snippet iconStart()}<Plus size={12} />{/snippet}
        Add
      </Button>
    </div>
  </div>

  <Alert
    variant="info"
    compact
    text="A folder — or a script — that declares its own engine keeps it: a specific answer beats the rule. Naming an engine Picus does not support (SQL Server, DB2, …) says those scripts are not its business: they are listed, never parsed, and never asked about. The count beside a rule is the folders it reaches today; a rule pointed only at file names shows none, because that number would be counting the wrong thing."
  />
{/if}

<style>
  .pa-empty {
    margin: 0;
    padding: 4px 2px 10px;
    font-size: var(--font-size-xs);
    line-height: 1.55;
    color: var(--text-muted);
  }

  /* name · icon · engine · role · where · reach · remove — one grid so every
     row's controls line up whatever the name's length. */
  .pa-row {
    display: grid;
    grid-template-columns:
      minmax(80px, 1fr) 14px minmax(140px, 1.3fr) minmax(120px, 1.1fr)
      minmax(120px, 1.1fr) auto auto;
    align-items: center;
    gap: 8px;
    padding: 7px 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .pa-row:last-child { border-bottom: none; }

  .pa-name {
    font-family: var(--font-code);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pa-icon { display: inline-flex; color: var(--text-disabled); }
  /* Reserved so the row does not jump when the count lands. */
  .pa-reach { display: inline-flex; min-width: 66px; justify-content: flex-end; }

  /* The add row reads as a draft of the rows above it, not as a second form. */
  .pa-new { padding-top: 12px; }
</style>
