<script lang="ts">
  /**
   * Every overlay this window can put in front, and the catalogue that reaches
   * them.
   *
   * Split out of the shell because they answer to a different owner: the shell
   * owns a layout and a keyboard, these own "what can be done and where it
   * opens". Keeping them here means the shell stays readable as a layout while
   * the list of verbs grows, which it does every release.
   *
   * Each dialog is rendered from the store flag rather than from the control
   * that opens it: the vault picker is reached from the start pane, the palette,
   * `Ctrl+Shift+O` and the title bar, and a dialog must not depend on which of
   * the four was hit.
   *
   * **Nothing here writes on its own.** The palette's context is read from the
   * stores; every action in it runs from a keystroke or a click.
   */
  import CommandPaletteShell, {
    type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import GarrulusDocsPanel from './GarrulusDocsPanel.svelte';
  import OpenVaultModal from './OpenVaultModal.svelte';
  import GarrulusCommitModal from './shell/GarrulusCommitModal.svelte';
  import GarrulusRemoteModal from './shell/GarrulusRemoteModal.svelte';
  import {
    buildGarrulusPalette,
    garrulusPaletteIcon,
    type GarrulusPaletteActions,
    type GarrulusPaletteContext,
  } from './garrulus-palette';
  import { garrulusUiStore, type DockPanel } from '$lib/stores/garrulus/ui.svelte';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';
  import { garrulusVaultStore } from '$lib/stores/garrulus/vault.svelte';
  import type { VaultSummary } from '$lib/ipc/garrulus';

  /** The palette's query. Session-shaped and window-local, so it lives here. */
  let paletteQuery = $state('');

  /**
   * Close the palette, then act.
   *
   * The microtask matters: an action that opens another dialog would otherwise
   * mount it while the palette is still up, and the two would fight over focus.
   */
  function run(fn: () => void) {
    garrulusUiStore.closePalette();
    queueMicrotask(fn);
  }

  /**
   * What this window can actually do today.
   *
   * The verbs whose surface has not landed — every note verb, the dock, the
   * graph, the trash, settings — are **not passed**, and `buildGarrulusPalette`
   * filters their entries out. That is the honest form of "not yet": a palette
   * that lists a command which does nothing is worse than one that lists fewer.
   * Adding the surface is what makes its verb appear; nothing here needs editing
   * twice.
   */
  const actions: GarrulusPaletteActions = {
    run,

    // Sync. Every one is a write, and every one is behind this click.
    syncNow: () => void garrulusSyncStore.syncNow(),
    pull: () => void garrulusSyncStore.pull(),
    push: () => void garrulusSyncStore.push(),
    commitWithMessage: () => garrulusUiStore.openCommitMessage(),
    configureRemote: () => garrulusUiStore.openRemoteConfig(),

    // Vault.
    openVault: () => garrulusUiStore.openVaultPicker('open'),
    createVault: () => garrulusUiStore.openVaultPicker('create'),
    switchVault: (id) => void garrulusVaultStore.openById(id),
    closeVault: () => void garrulusVaultStore.close(),
    rebuildIndex: () => void garrulusVaultStore.rebuild(),

    // View and help.
    showSection: (id) => garrulusUiStore.selectSection(id),
    toggleSidebar: () => garrulusUiStore.toggleSidebar(),
    // The dock. Only the conflicts panel exists so far, but the ids are the
    // store's, so the palette entries land where they say they will as the other
    // two arrive — and until they do, the dock opens on conflicts rather than on
    // an empty frame.
    showDock: (id) => garrulusUiStore.showDock(id as DockPanel),
    toggleDock: () => garrulusUiStore.toggleDock(),
    openDocs: (section) => garrulusUiStore.openDocs(section),
    // The shortcut reference is a page of the docs rather than a dialog of its
    // own: one panel to open, one place to keep the list right.
    openShortcuts: () => garrulusUiStore.openDocs('shortcuts'),
  };

  const context = $derived<GarrulusPaletteContext>({
    vaultOpen: garrulusVaultStore.isOpen,
    // No editor in this window yet. It stays null rather than guessing, and the
    // note verbs are absent from `actions` for the same reason.
    notePath: null,
    syncTag: garrulusSyncStore.tag,
    // The count is on the state itself — `conflict` is the only tag carrying it.
    conflicts: garrulusSyncStore.tag === 'conflict' ? garrulusSyncStore.count : 0,
    history: garrulusSyncStore.descriptor?.capabilities.history ?? false,
    types: garrulusVaultStore.types.map((t) => ({ id: t.id, name: t.name })),
    vaults: garrulusVaultStore.entries.map((v) => ({
      id: v.id,
      displayName: v.display_name,
      path: v.path,
    })),
  });

  // Annotated at the call site on purpose: `buildGarrulusPalette` returns its
  // sections structurally so a `.ts` module need not import a type out of a
  // `.svelte` one, and this annotation is what turns any drift into a type error.
  const paletteSections = $derived<PaletteSection[]>(
    buildGarrulusPalette(paletteQuery, context, actions),
  );

  /**
   * The picker opened a vault. It performs the call itself — it needs the
   * failure inline, beside the folder that produced it — so what arrives here is
   * the summary. `adopt` is the single funnel: it loads the note types and
   * re-reads the sync state, which is per-vault.
   */
  function onVaultOpened(summary: VaultSummary) {
    garrulusVaultStore.adopt(summary);
  }
</script>

{#if garrulusUiStore.vaultPickerOpen}
  <OpenVaultModal
    initialPage={garrulusUiStore.vaultPickerPage}
    currentRoot={garrulusVaultStore.root}
    onOpened={onVaultOpened}
    onClose={() => garrulusUiStore.closeVaultPicker()}
  />
{/if}

{#if garrulusUiStore.remoteConfigOpen}
  <!-- `vaultName` is not decoration: it seeds the name proposed when creating the
       private repository, and without it every vault is offered "notes". -->
  <GarrulusRemoteModal
    vaultName={garrulusVaultStore.name}
    onClose={() => garrulusUiStore.closeRemoteConfig()}
  />
{/if}

{#if garrulusUiStore.commitMessageOpen}
  <GarrulusCommitModal onClose={() => garrulusUiStore.closeCommitMessage()} />
{/if}

{#if garrulusUiStore.paletteOpen}
  <CommandPaletteShell
    onClose={() => garrulusUiStore.closePalette()}
    iconResolver={garrulusPaletteIcon}
    sections={paletteSections}
    bind:query={paletteQuery}
    placeholder="Search a command, a vault or a note type…"
  />
{/if}

{#if garrulusUiStore.docsOpen}
  <GarrulusDocsPanel
    initialSection={garrulusUiStore.docsSection || 'getting-started'}
    onClose={() => garrulusUiStore.closeDocs()}
  />
{/if}
