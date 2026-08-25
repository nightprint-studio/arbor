<script lang="ts">
  /**
   * Scripts panel — the repository on disk, exactly as it is on disk.
   *
   * This is where the product's structural rule is visible: the dialect belongs
   * to the FOLDER. Any directory may declare an engine and a purpose, everything
   * under it inherits that until something overrides it, and the panel shows the
   * real hierarchy rather than a two-level shape invented from folder names —
   * because a repository that keeps its engine five levels down was previously
   * rendered as a flat run of identical rows.
   *
   * The panel owns the framing (whose repository this is, what could not be read,
   * what Picus inferred); the tree itself is `ScriptsTree`.
   *
   * The repository shown is **the active connection's**: Picus is database
   * oriented, so you open a database and its scripts are what you get. A
   * connection with none attached is offered a folder to point at, rather than
   * leaving the panel to look broken.
   */
  import { FolderTree, RefreshCw, FolderOpen, Database, FileCog, FolderCog } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import NoticeList from './NoticeList.svelte';
  import ScriptsTree from './ScriptsTree.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { picusUiStore } from '$lib/stores/picus/ui.svelte';
  import { FOREIGN_ENGINES, folderEngine, isForeignEngine, type ForeignEngine } from '$lib/types/picus';
  import { tooltip } from '$lib/actions/tooltip';

  let query = $state('');

  const connection = $derived(connectionsStore.active);
  const attached = $derived(picusProjectStore.attached);
  const unclassified = $derived(picusProjectStore.unclassifiedFolders);
  /**
   * Folders in an engine Picus does not read. Kept apart from `unclassified` on
   * purpose and rendered as a statement rather than a warning: there is nothing
   * for the user to do about them, and a warning that cannot be acted on is how
   * a panel teaches people to ignore its warnings.
   */
  const unsupported = $derived(picusProjectStore.unsupportedFolders);

  const foreignEngineNames = $derived.by(() => {
    const names = [
      ...new Set(
        unsupported
          .map((e) => folderEngine(e.node))
          .filter(isForeignEngine)
          .map((e) => FOREIGN_ENGINES[e]),
      ),
    ];
    if (names.length <= 1) return names[0] ?? 'another engine';
    return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
  });

  function openFile(path: string) {
    const file = picusProjectStore.fileByPath(path);
    if (!file) return;
    picusTabsStore.openFile(file.path, file.name, picusProjectStore.dialectOfFile(file.path));
  }
</script>

<PanelShell title="Scripts on disk" count={picusProjectStore.fileCount}>
  {#snippet icon()}<FolderTree size={13} />{/snippet}

  {#snippet actions()}
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Re-read the repository from disk', shortcut: 'F5' }}
      ariaLabel="Re-read the repository from disk"
      disabled={!attached || picusProjectStore.loading}
      onclick={() => void picusProjectStore.refresh()}
    >
      {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={{ content: 'Say what a folder is — its engine and its purpose', shortcut: 'Ctrl+Shift+F' }}
      ariaLabel="Classify a folder"
      disabled={!attached || !picusProjectStore.folderCount}
      onclick={() => picusUiStore.openFolderClassify()}
    >
      {#snippet iconStart()}<FolderCog size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={{
        content: 'Say what one script is — for a folder holding two engines at once',
        shortcut: 'F6',
      }}
      ariaLabel="Classify a script"
      disabled={!attached || !picusProjectStore.fileCount}
      onclick={() => picusUiStore.openFileClassify()}
    >
      {#snippet iconStart()}<FileCog size={13} />{/snippet}
    </Button>
    <Button
      variant="icon"
      size="xs"
      tooltip={attached
        ? 'Point this connection at another folder of scripts'
        : 'Attach the folder of scripts this database is installed from'}
      ariaLabel="Attach a script repository"
      disabled={!connection}
      onclick={() => connection && picusUiStore.openScriptRootPicker(connection.id)}
    >
      {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
    </Button>
  {/snippet}

  {#snippet toolbar()}
    <SearchBar bind:query showRegex={false} placeholder="Filter folders and files" ariaLabel="Filter folders and files" />
  {/snippet}

  {#if !connection}
    <StateBlock
      tone="info"
      fill={false}
      label="No connection selected. A repository of scripts belongs to the database it installs — pick one under Connections."
    />
  {:else if !attached}
    <div class="sp-attach">
      <StateBlock tone="info" fill={false}>
        <div class="sp-attach-text">
          <strong>{connection.name} has no scripts attached.</strong>
          <span>
            Point it at the folder this database is installed from. Picus reads the tree as
            it is, works out which directories hold which engine, indexes the objects and
            checks the engines against each other.
          </span>
        </div>
      </StateBlock>
      <Button
        variant="primary"
        size="sm"
        onclick={() => picusUiStore.openScriptRootPicker(connection.id)}
      >
        {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
        Attach a folder…
      </Button>
    </div>
  {:else if picusProjectStore.loading && !picusProjectStore.folderCount}
    <StateBlock tone="loading">
      {#snippet spinner()}<Spinner size={14} />{/snippet}
      <span>Reading {picusProjectStore.root}…</span>
    </StateBlock>
  {:else if picusProjectStore.error}
    <div class="sp-error">
      <Alert variant="error" compact title="This folder could not be read" text={picusProjectStore.error} />
      <div class="sp-error-actions">
        <Button variant="secondary" size="xs" onclick={() => void picusProjectStore.refresh()}>Try again</Button>
        <Button
          variant="ghost"
          size="xs"
          onclick={() => picusUiStore.openScriptRootPicker(connection.id)}
        >
          Choose another folder…
        </Button>
      </div>
    </div>
  {:else if !picusProjectStore.folderCount}
    <StateBlock
      tone="info"
      fill={false}
      label="There is no folder of SQL scripts under this path."
    />
  {:else}
    <!-- The reader's questions come before the tree: a folder it could not
         classify changes what every row below it means. -->
    <NoticeList notes={picusProjectStore.problems} label="Needs an answer" onOpen={openFile} />

    {#if picusProjectStore.isNew}
      <div class="sp-inferred">
        <Alert
          variant="info"
          compact
          text="This reading was inferred from the folder names — nothing has been written into the repository. Setting a folder's engine or role saves it, and only then."
        />
      </div>
    {/if}

    {#if unclassified.length}
      <!-- The one thing that stops the repository working at all: scripts in a
           folder no engine covers. Nothing is generated into them, and nothing
           about them is compared, until somebody says what they are. -->
      <div class="sp-inferred">
        <Alert
          variant="warning"
          compact
          title={`${unclassified.length} folder${unclassified.length === 1 ? '' : 's'} of scripts with no engine`}
          text="Right-click one in the tree — or press Ctrl+Shift+F — to say which database it is written for. If every folder of that name means the same thing, Picus offers to say it once for the whole project. Until then nothing is generated into them."
        />
      </div>
    {/if}

    {#if unsupported.length}
      <!-- Stated, never warned about: these folders have an answer. The point of
           the line is that their absence from every count below is explained,
           not that the user should do something. -->
      <p class="sp-foreign">
        {unsupported.length} folder{unsupported.length === 1 ? '' : 's'} in
        {foreignEngineNames} — listed, never parsed, and left alone.
      </p>
    {/if}

    <ScriptsTree filter={query} />

    <NoticeList notes={picusProjectStore.notes} label="What Picus inferred" onOpen={openFile} />

    <p class="sp-hint">
      A folder declares its engine and its purpose; everything under it inherits them until
      something says otherwise. A quiet chip is inherited — the solid one is where it is set.
      A script carries a chip of its own only when it says something its folder does not.
    </p>
    <p class="sp-root" use:tooltip={picusProjectStore.root}>
      <Database size={11} />
      {connection.name} · {picusProjectStore.root}
    </p>
  {/if}
</PanelShell>

<style>
  /* Somebody else's engines: a fact, in the same register as the root path
     below it — never the amber of something that needs attention. */
  .sp-foreign {
    padding: 2px 12px 6px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }

  .sp-hint {
    padding: 10px 12px;
    font-size: var(--font-size-xs);
    line-height: 1.5;
    color: var(--text-muted);
  }

  /* Which database's repository this is — the panel's whole framing in one line. */
  .sp-root {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 12px 10px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sp-attach {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 10px;
    padding: 4px 12px 12px;
  }
  .sp-attach-text { display: flex; flex-direction: column; gap: 4px; text-align: left; }
  .sp-attach-text strong { font-size: var(--font-size-sm); }
  .sp-attach-text span { font-size: var(--font-size-xs); line-height: 1.5; color: var(--text-muted); }

  .sp-error { display: flex; flex-direction: column; gap: 8px; padding: 8px 12px; }
  .sp-error-actions { display: flex; gap: 6px; }

  .sp-inferred { padding: 4px 12px 8px; }
</style>
