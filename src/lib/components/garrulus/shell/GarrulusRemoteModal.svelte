<script lang="ts">
  /**
   * Where this vault syncs to — the destination, and nothing else.
   *
   * Two kinds ship, and the second is not filler (`docs/garrulus-design.md` §4.1):
   * a **git** remote, and a **folder** the vault is mirrored into — a USB stick, a
   * network share, or a directory that Drive/OneDrive/Dropbox already syncs. They
   * are different enough that the form asks different questions, so the kind is
   * picked first and the fields follow it.
   *
   * Four verbs, deliberately separate:
   *
   *   • **Test** builds the destination and probes it, and persists nothing. "Does
   *     this work?" has to be answerable without adopting the answer.
   *   • **Save** persists first and probes second, so a destination that did not
   *     answer is still the one the user configured — it comes back as offline
   *     rather than as "not configured".
   *   • **Create a private repository** goes through the shell's git provider,
   *     points `origin` at what it made, and adopts it. It is stated in the UI
   *     that the repository is private, because that is a product guarantee with
   *     no opt-out at any layer — not a default someone might flip later.
   *   • **Stop syncing** drops the destination and touches no file. Behind a
   *     confirmation, because "which button made my vault local-only" is not a
   *     question anyone should have to answer from memory.
   *
   * **The form is not drawn until the configured destination has been read.**
   * Seeding the fields from a snapshot taken while the module body runs is the one
   * bug this dialog cannot afford: the read has not landed yet, every box is
   * empty, and a Save in that window writes nulls over a destination that was
   * working. So the read *is* the gate — `onMount` asks `garrulus_remote_config`
   * and until it answers there is a loading block and no Save to press.
   *
   * **Writes go through the IPC wrappers rather than through `garrulusSyncStore`.**
   * The store's `act()` turns a failure into a toast and returns nothing, and it
   * skips silently while another action is in flight; a form needs the failure
   * inline and needs to stay open on it. What the store *does* own is the state
   * the title bar reads, so every successful write here is followed by
   * `garrulusSyncStore.refresh()`, and the store's `busy` flag is honoured so a
   * Save cannot race a sync started from the title bar.
   *
   * Nothing on this page writes a note or moves a byte of the vault: it configures
   * a destination. Everything that changes content still happens because the user
   * pressed the sync button (§4.2).
   */
  import { onMount } from 'svelte';
  import {
    CheckCircle2, Cloud, FolderOpen, FolderSync, GitBranch, Lock, Plus, Unplug,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import RadioGroup from '$lib/components/shared/ui/RadioGroup.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import StateBlock from '$lib/components/shared/ui/StateBlock.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { garrulusSyncStore } from '$lib/stores/garrulus/sync.svelte';
  import {
    clearRemote,
    createRemoteRepo,
    remoteConfig,
    setRemote,
    syncStateCount,
    syncStateTag,
    testRemote,
    type RemoteConfig,
    type RemoteKind,
    type RemoteStatus,
    type SyncState,
  } from '$lib/ipc/garrulus';

  interface Props {
    onClose: () => void;
    /** The vault's name, for the suggested repository name and the heading. */
    vaultName?: string | null;
    /** A destination was adopted or dropped — the host re-titles whatever names
     *  it. The sync store is refreshed here, so the host does not have to. */
    onChanged?: (config: RemoteConfig | null) => void;
  }

  let { onClose, vaultName = null, onChanged }: Props = $props();

  let kind = $state<RemoteKind>('git');
  let gitRemote = $state('');
  let branch = $state('');
  let folder = $state('');

  /** What is on disk right now, so the footer can offer to stop syncing only when
   *  there is something to stop. */
  let configured = $state<RemoteConfig | null>(null);
  /** The gate. Nothing is editable and nothing is savable before this. */
  let loaded = $state(false);

  let testing = $state(false);
  let saving = $state(false);
  let creating = $state(false);
  let clearing = $state(false);
  let status = $state<RemoteStatus | null>(null);
  let error = $state<string | null>(null);
  /** Set only by "create a private repository", so the confirmation of privacy
   *  appears where the action happened and not on every save. */
  let createdRepo = $state(false);

  let picking = $state(false);
  let confirmClear = $state(false);

  let firstField = $state<HTMLInputElement | undefined>();
  /** Plain `let`, not `$state`: it gates the effect below and nothing renders it,
   *  so making it reactive would put the effect's own write in its dependencies. */
  let autoFocused = false;

  // No `autofocus` attribute (a11y). One shot only — the first field appears when
  // the read lands, and re-focusing on every later change of `firstField` would
  // yank the caret out from under someone switching kind from the keyboard.
  $effect(() => {
    if (autoFocused || !firstField) return;
    autoFocused = true;
    firstField.focus();
  });

  onMount(() => {
    void remoteConfig()
      .then((cfg) => {
        configured = cfg;
        if (!cfg) return;
        kind = cfg.kind;
        gitRemote = cfg.gitRemote ?? '';
        branch = cfg.branch ?? '';
        folder = cfg.folder ?? '';
      })
      // A destination that failed to build at open time is still the configured
      // one; a read that fails is a different thing and says so.
      .catch((e) => { error = String(e); })
      .finally(() => { loaded = true; });
  });

  const KINDS = [
    {
      value: 'git',
      label: 'Git repository',
      description: 'Full history per note, and the conflict handling the design is built on.',
      icon: GitBranch,
    },
    {
      value: 'folder',
      label: 'Mirror folder',
      description: 'A stick, a share, or a folder Drive already syncs. No history.',
      icon: FolderSync,
    },
  ];

  const valid = $derived(kind === 'git' || folder.trim() !== '');
  /**
   * A write is in flight — this dialog's own, or one the title bar started.
   *
   * Two writers on one vault's destination is the race worth closing, and it is
   * the only reason the fields go inert. A `Test` deliberately does *not* freeze
   * them: it can take a network round trip, disabling the focused field would
   * drop focus to the body, and editing during a test is already handled — every
   * control clears `status` on input, so the answer cannot outlive its question.
   */
  const writing = $derived(saving || creating || clearing || garrulusSyncStore.busy);
  const busy = $derived(writing || testing);
  /** Nothing is actionable before the configured destination has been read. */
  const ready = $derived(loaded && !busy);

  function toConfig(): RemoteConfig {
    return kind === 'git'
      ? {
          kind: 'git',
          // Absent means `origin` on the backend, so an empty box is not an
          // error — it is the default, spelled by not spelling it.
          gitRemote: gitRemote.trim() || null,
          branch: branch.trim() || null,
          folder: null,
        }
      : { kind: 'folder', folder: folder.trim(), gitRemote: null, branch: null };
  }

  /** Tell the rest of the window that the destination moved. The store first —
   *  it is what the title bar's control reads — then the host's own hook. */
  async function announce(config: RemoteConfig | null) {
    await garrulusSyncStore.refresh();
    onChanged?.(config);
  }

  async function test() {
    if (!valid || !ready) return;
    testing = true;
    error = null;
    status = null;
    try {
      status = await testRemote(toConfig());
    } catch (e) {
      error = String(e);
    } finally {
      testing = false;
    }
  }

  async function save() {
    if (!valid || !ready) return;
    saving = true;
    error = null;
    try {
      const config = toConfig();
      status = await setRemote(config);
      configured = config;
      createdRepo = false;
      // Hand the status to the store: `refresh()` re-reads the state and the
      // config but not the descriptor, and verbs gated on the remote's
      // capabilities (note history) read it from there.
      garrulusSyncStore.adoptStatus(status, config);
      toastStore.show(`Syncing to ${status.descriptor.display}.`, 'success');
      await announce(config);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Create a private repository ─────────────────────────────────────────────
  let repoName = $state('');
  const suggestedName = $derived(
    (vaultName ?? '').trim().toLowerCase().replace(/\s+/g, '-') || 'notes',
  );

  async function createRepo() {
    if (!ready) return;
    const name = repoName.trim() || suggestedName;
    creating = true;
    error = null;
    status = null;
    let created: RemoteConfig | null = null;
    try {
      // The backend creates it, points `origin` at it and adopts it in one step —
      // there is no half-configured state to clean up if the user closes now.
      created = await createRemoteRepo(name);
      configured = created;
      kind = created.kind;
      gitRemote = created.gitRemote ?? '';
      branch = created.branch ?? '';
      createdRepo = true;
      toastStore.show(`Private repository ${name} created and adopted.`, 'success');
      await announce(created);
    } catch (e) {
      error = String(e);
    } finally {
      creating = false;
    }
    if (!created) return;

    // Read-only follow-up, purely so the status block below can name the
    // destination and its standing without a second dialog. Outside the try on
    // purpose: a probe that does not answer says nothing about the repository
    // that was just created and adopted, and reporting it as "that did not work"
    // next to a success alert is the one reading that would be wrong.
    try {
      status = await testRemote(created);
      // Same reason as in `save()`: the descriptor only reaches the store from
      // here, and the capability-gated verbs read it there.
      garrulusSyncStore.adoptStatus(status, created);
    } catch { /* the success alert already carries the outcome that matters */ }
  }

  async function stopSyncing() {
    confirmClear = false;
    clearing = true;
    error = null;
    try {
      await clearRemote();
      configured = null;
      status = null;
      createdRepo = false;
      toastStore.show('This vault is local-only again.', 'info');
      await announce(null);
      onClose();
    } catch (e) {
      error = String(e);
    } finally {
      clearing = false;
    }
  }

  /**
   * One `SyncState` as a sentence.
   *
   * Local on purpose and only for the block below: the title bar's sync button is
   * the one place sync state is *displayed* (§4.3), and its own state table
   * phrases the same states as button labels ("3 notes to send") rather than as
   * report lines. Two tables, two jobs — the day a third caller needs one of them,
   * it is the button's that moves into `stores/garrulus/`.
   */
  function describe(state: SyncState): string {
    const n = syncStateCount(state);
    switch (syncStateTag(state)) {
      case 'synced':      return 'Everything here is there, and the other way round.';
      case 'has-changes': return `${n} note${n === 1 ? '' : 's'} waiting to be sent.`;
      case 'ahead':       return `${n} change${n === 1 ? '' : 's'} waiting to be sent.`;
      case 'behind':      return `${n} note${n === 1 ? '' : 's'} waiting to come in.`;
      case 'diverged':    return 'Both sides have moved since they last agreed.';
      case 'conflict':    return `${n} conflict${n === 1 ? '' : 's'} to resolve.`;
      case 'offline':     return 'Configured, but not reachable right now.';
      case 'no-remote':   return 'No destination configured.';
      default:            return 'Reachable.';
    }
  }

  function onKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      void save();
    }
  }
</script>

<Modal {onClose} width="640px" height="580px" padBody={false} ariaLabel="Sync destination">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Cloud size={14} />
      <span class="modal-title">
        Sync destination{vaultName ? ` — ${vaultName}` : ''}
      </span>
      {#if configured}
        <Badge variant="tone" tone="neutral" size="sm" label={configured.kind} />
      {/if}
    </ModalHeader>
  {/snippet}

  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div class="rs" role="group" aria-label="Sync destination" onkeydown={onKeyDown}>
    {#if !loaded}
      <!-- The gate, not a courtesy: see the header comment. An empty form shown
           over a configured vault is a form that saves nulls. -->
      <StateBlock tone="loading">
        {#snippet spinner()}<Spinner size={14} />{/snippet}
        <span>Reading the destination this vault already has…</span>
      </StateBlock>
    {:else}
      <section class="rs-section">
        <FormField label="Kind">
          <RadioGroup
            value={kind}
            options={KINDS}
            appearance="card"
            direction="vertical"
            block
            disabled={writing}
            onchange={(v) => { kind = v as RemoteKind; status = null; error = null; }}
          />
        </FormField>
      </section>

      {#if kind === 'git'}
        <section class="rs-section">
          <div class="rs-grid">
            <FormField label="Remote" hint="Leave empty for origin.">
              <Input
                bind:value={gitRemote}
                bind:element={firstField}
                placeholder="origin"
                ariaLabel="Git remote name"
                disabled={writing}
                oninput={() => (status = null)}
              />
            </FormField>
            <FormField
              label="Branch"
              optionalText="(optional)"
              hint="Empty tracks whatever is checked out — what a vault nobody branches always wants."
            >
              <Input
                bind:value={branch}
                placeholder="main"
                ariaLabel="Branch to track"
                disabled={writing}
                oninput={() => (status = null)}
              />
            </FormField>
          </div>
        </section>

        <section class="rs-section">
          <FormField
            label="No repository yet?"
            hint="Creates it through the account Arbor is already signed in to, points this vault's origin at it, and starts syncing."
          >
            <div class="rs-row">
              <Input
                bind:value={repoName}
                placeholder={suggestedName}
                ariaLabel="Repository name"
                disabled={writing}
              />
              <Button
                variant="secondary"
                size="sm"
                loading={creating}
                disabled={!ready}
                tooltip={{ content: 'Create it private and adopt it' }}
                onclick={() => void createRepo()}
              >
                {#snippet iconStart()}<Plus size={13} />{/snippet}
                Create private repository
              </Button>
            </div>
          </FormField>

          <!-- Stated, not assumed: privacy here is a guarantee with no opt-out,
               and a guarantee nobody is told about is indistinguishable from a
               default that might change. -->
          <Alert variant="info" compact>
            <span class="rs-inline">
              <Lock size={12} />
              <span>
                The repository is created <b>private</b>. There is no public option at any
                layer of this flow — a personal note vault has no business being public, and
                once its contents are indexed the click cannot be taken back. Making it
                public is something to do deliberately, on the provider's own site.
              </span>
            </span>
          </Alert>
        </section>
      {:else}
        <section class="rs-section">
          <FormField
            label="Mirror folder"
            required
            hint="An absolute path on this machine. It stays on this machine: the other PC has its own, and the path never travels with the vault."
          >
            <div class="rs-row">
              <Input
                bind:value={folder}
                bind:element={firstField}
                placeholder="/Volumes/stick/notes-mirror"
                ariaLabel="Mirror folder"
                disabled={writing}
                oninput={() => (status = null)}
              />
              <Button
                variant="secondary"
                size="sm"
                disabled={writing}
                ariaLabel="Choose the mirror folder"
                onclick={() => (picking = true)}
              >
                {#snippet iconStart()}<FolderOpen size={13} />{/snippet}
                Choose…
              </Button>
            </div>
          </FormField>

          <Alert variant="warning" compact>
            A mirror folder keeps no history, so per-note version history and
            “restore this version” stay hidden for this vault. Conflicts are still
            detected and still resolved the same way.
          </Alert>
        </section>
      {/if}

      {#if createdRepo}
        <section class="rs-section">
          <Alert variant="success" compact>
            <span class="rs-inline">
              <Lock size={12} />
              <span>Repository created <b>private</b> and adopted as this vault's destination.</span>
            </span>
          </Alert>
        </section>
      {/if}

      {#if status}
        <section class="rs-section">
          <div class="rs-status">
            <CheckCircle2 size={14} />
            <div class="rs-status-text">
              <span class="rs-status-title">{status.descriptor.display}</span>
              <span class="rs-status-line">{describe(status.state)}</span>
            </div>
            <span class="rs-caps">
              <Badge
                variant="tone"
                tone={status.descriptor.capabilities.history ? 'success' : 'neutral'}
                size="sm"
                label={status.descriptor.capabilities.history ? 'history' : 'no history'}
              />
              <Badge
                variant="tone"
                tone={status.descriptor.capabilities.conflicts ? 'success' : 'warning'}
                size="sm"
                label={status.descriptor.capabilities.conflicts ? 'detects conflicts' : 'last writer wins'}
              />
            </span>
          </div>
        </section>
      {/if}

      {#if error}
        <section class="rs-section">
          <Alert variant="error" title="That did not work" text={error} />
        </section>
      {/if}
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      {#if configured}
        <Button
          variant="ghost"
          size="sm"
          disabled={!ready}
          tooltip={{ content: 'Stop syncing this vault. No file is touched.' }}
          onclick={() => (confirmClear = true)}
        >
          {#snippet iconStart()}<Unplug size={13} />{/snippet}
          Stop syncing
        </Button>
      {:else}
        <span></span>
      {/if}
      <div class="rs-actions">
        <Button
          variant="secondary"
          size="sm"
          loading={testing}
          disabled={!valid || !ready}
          tooltip={{ content: 'Try it without adopting it — nothing is saved' }}
          onclick={() => void test()}
        >
          Test
        </Button>
        <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          size="sm"
          loading={saving}
          disabled={!valid || !ready}
          tooltip={{ content: 'Save this destination and probe it', shortcut: 'Ctrl+Enter' }}
          onclick={() => void save()}
        >
          Save
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if picking}
  <!-- Arbor's own explorer in folder mode, stacked over this dialog so the
       half-filled form survives the choice. -->
  <FileExplorerModal
    mode="folder"
    title="Choose the mirror folder"
    initialPath={folder || undefined}
    onConfirm={(path) => { folder = path; status = null; picking = false; }}
    onCancel={() => (picking = false)}
    onClose={() => (picking = false)}
  />
{/if}

{#if confirmClear}
  <ConfirmModal
    title="Stop syncing this vault?"
    message="The vault becomes local-only."
    detail="Nothing on disk changes: a git vault keeps its .git, a mirrored vault keeps its mirror. Point it at a destination again whenever you like."
    variant="warning"
    confirmLabel="Stop syncing"
    zIndex="var(--z-modal-picker)"
    onConfirm={() => void stopSyncing()}
    onCancel={() => (confirmClear = false)}
  />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }

  .rs {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    overflow-y: auto;
    padding: 16px;
  }

  /* `flex-shrink: 0`: the body scrolls, so a section keeps its height instead of
     squashing when the status block and an error alert are both showing. */
  .rs-section { display: flex; flex-direction: column; gap: 12px; flex-shrink: 0; }

  .rs-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
  }

  /* Typed value first, picker second: the field stays the source of truth. */
  .rs-row { display: flex; align-items: center; gap: 8px; }
  .rs-row > :global(:first-child) { flex: 1; min-width: 0; }

  .rs-inline { display: inline-flex; align-items: flex-start; gap: 6px; line-height: 1.5; }
  .rs-inline :global(svg) { margin-top: 2px; flex-shrink: 0; }

  .rs-status {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 11px;
    background: var(--success-subtle);
    border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
    border-radius: var(--radius-md);
  }
  .rs-status :global(svg) { color: var(--success); flex-shrink: 0; }
  .rs-status-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .rs-status-title { font-size: var(--font-size-sm); color: var(--text-primary); font-weight: 600; }
  .rs-status-line { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .rs-caps { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }

  .rs-actions { display: flex; align-items: center; gap: 8px; }
</style>
