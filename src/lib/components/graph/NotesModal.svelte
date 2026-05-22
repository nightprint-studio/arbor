<script lang="ts">
  import { StickyNote, Plus, Pencil, Trash2, Check, CloudOff, Cloud, CheckCircle, RefreshCw, ChevronDown } from 'lucide-svelte';
  import { notesStore } from '$lib/stores/notes.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { saveCommitNote, deleteCommitNote, pushNoteNamespace } from '$lib/ipc/notes';
  import type { CommitNote } from '$lib/types/git';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    commitOid,
    shortOid,
    onClose,
  }: {
    commitOid: string;
    shortOid: string;
    onClose: () => void;
  } = $props();

  const tab = $derived(tabsStore.activeTab);

  // ── Local state ──────────────────────────────────────────────────────────────
  let notes        = $derived(notesStore.getNotes(commitOid));
  let editingNs    = $state<string | null>(null);
  let editContent  = $state('');
  let addingNew    = $state(false);
  let newNamespace = $state('');
  let newContent   = $state('');
  let saving       = $state(false);
  let checkingRemote = $state<Set<string>>(new Set());
  // Split button dropdown open state: namespace → bool
  let splitOpen    = $state<string | null>(null);  // which namespace's dropdown is open
  let addSplitOpen = $state(false);                // dropdown for the "Add" split button

  const DEFAULT_NS = 'commits';

  // Git ref component rules (git check-ref-format):
  // no whitespace, no ~^:?*[\, no .., cannot start/end with dot, cannot end with .lock
  function validateNamespace(ns: string): string | null {
    if (!ns) return null; // empty handled separately
    if (/[\s~^:?*\[\\]/.test(ns))   return 'Invalid character (no spaces or ~^:?*[\\)';
    if (/\.\./.test(ns))             return 'Cannot contain ".."';
    if (ns.startsWith('.'))          return 'Cannot start with "."';
    if (ns.endsWith('.'))            return 'Cannot end with "."';
    if (ns.endsWith('.lock'))        return 'Cannot end with ".lock"';
    if (ns.startsWith('/') || ns.endsWith('/') || ns.includes('//')) return 'Invalid slash usage';
    return null;
  }

  let nsError = $derived(validateNamespace(newNamespace.trim()));

  // When modal opens, check remote status for all notes whose status is unknown.
  // We track which ones we've already dispatched to avoid repeated calls.
  let checkedNs = new Set<string>();
  $effect(() => {
    if (!tab) return;
    for (const note of notes) {
      if (note.remote_status === 'unknown' && !checkedNs.has(note.namespace)) {
        checkedNs.add(note.namespace);
        const ns = note.namespace;
        const s = new Set(checkingRemote);
        s.add(ns);
        checkingRemote = s;
        notesStore.checkRemoteStatus(tab.id, commitOid, ns).finally(() => {
          const s2 = new Set(checkingRemote);
          s2.delete(ns);
          checkingRemote = s2;
        });
      }
    }
  });

  // ── Edit ─────────────────────────────────────────────────────────────────────

  function startEdit(note: CommitNote) {
    editingNs   = note.namespace;
    editContent = note.content;
    splitOpen   = null;
  }

  function cancelEdit() {
    editingNs   = null;
    editContent = '';
  }

  async function doSave(namespace: string, andPush: boolean) {
    if (!tab) return;
    saving = true;
    splitOpen = null;
    try {
      notesStore.optimisticUpsert(commitOid, namespace, editContent);
      await saveCommitNote(tab.id, commitOid, namespace, editContent);
      if (andPush) {
        await pushNoteNamespace(tab.id, namespace);
        uiStore.showToast(`Note pushed to origin`, 'success');
      }
      await notesStore.reload(tab.id, commitOid);
      // Re-check remote status after save
      notesStore.checkRemoteStatus(tab.id, commitOid, namespace);
      editingNs = null;
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
      await notesStore.reload(tab.id, commitOid);
    } finally {
      saving = false;
    }
  }

  // ── Delete ────────────────────────────────────────────────────────────────────

  async function deleteNote(namespace: string) {
    if (!tab) return;
    saving = true;
    try {
      notesStore.optimisticRemove(commitOid, namespace);
      await deleteCommitNote(tab.id, commitOid, namespace);
      await notesStore.reload(tab.id, commitOid);
      if (editingNs === namespace) editingNs = null;
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
      await notesStore.reload(tab.id, commitOid);
    } finally {
      saving = false;
    }
  }

  // ── Add new ───────────────────────────────────────────────────────────────────

  function startAdd() {
    addingNew    = true;
    newNamespace = DEFAULT_NS;
    newContent   = '';
  }

  function cancelAdd() {
    addingNew    = false;
    newNamespace = '';
    newContent   = '';
    addSplitOpen = false;
  }

  async function confirmAdd(andPush: boolean) {
    if (!tab || !newNamespace.trim() || !newContent.trim()) return;
    if (notes.some(n => n.namespace === newNamespace.trim())) {
      uiStore.showToast(`Namespace "${newNamespace.trim()}" already exists`, 'warning');
      return;
    }
    saving = true;
    addSplitOpen = false;
    try {
      const ns = newNamespace.trim();
      notesStore.optimisticUpsert(commitOid, ns, newContent);
      await saveCommitNote(tab.id, commitOid, ns, newContent);
      if (andPush) {
        await pushNoteNamespace(tab.id, ns);
        uiStore.showToast(`Note pushed to origin`, 'success');
      }
      await notesStore.reload(tab.id, commitOid);
      notesStore.checkRemoteStatus(tab.id, commitOid, ns);
      addingNew = false;
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
      await notesStore.reload(tab.id, commitOid);
    } finally {
      saving = false;
    }
  }

  // ── Remote status ─────────────────────────────────────────────────────────────

  async function refreshRemoteStatus(namespace: string) {
    if (!tab) return;
    const s = new Set(checkingRemote);
    s.add(namespace);
    checkingRemote = s;
    try {
      await notesStore.checkRemoteStatus(tab.id, commitOid, namespace);
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    } finally {
      const s2 = new Set(checkingRemote);
      s2.delete(namespace);
      checkingRemote = s2;
    }
  }

  function remoteIcon(status: string) {
    switch (status) {
      case 'in_sync':     return CheckCircle;
      case 'out_of_sync': return Cloud;
      case 'local_only':  return CloudOff;
      default:            return null;
    }
  }

  function remoteLabel(status: string) {
    switch (status) {
      case 'in_sync':     return 'In sync';
      case 'out_of_sync': return 'Local ahead';
      case 'local_only':  return 'Local only';
      default:            return '';
    }
  }

  function remoteClass(status: string) {
    switch (status) {
      case 'in_sync':     return 'status-synced';
      case 'out_of_sync': return 'status-ahead';
      case 'local_only':  return 'status-local';
      default:            return '';
    }
  }

  function formatDate(ts: number): string {
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = Math.floor((now - d.getTime()) / 1000);
    if (diff < 60)          return 'just now';
    if (diff < 3600)        return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86400)       return `${Math.floor(diff / 3600)}h ago`;
    if (diff < 7 * 86400)   return `${Math.floor(diff / 86400)}d ago`;
    return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
  }

  function isoDate(ts: number): string {
    return new Date(ts * 1000).toLocaleString();
  }

  // Escape closes the modal directly (Modal handles it). Open split-button
  // dropdowns dismiss via outside-click below.
</script>

<Modal {onClose} width="480px" ariaLabel="Commit notes">
  {#snippet header()}
    <ModalHeader {onClose}>
      <StickyNote size={14} />
      <span class="modal-title">Notes</span>
      <code class="commit-ref">{shortOid}</code>
    </ModalHeader>
  {/snippet}

  <div class="notes-body">
      {#if notes.length === 0 && !addingNew}
        <div class="empty-state">
          <StickyNote size={22} class="empty-icon" />
          <p>No notes on this commit.</p>
          <button class="btn-primary" onclick={startAdd}>
            <Plus size={13} />
            Add note
          </button>
        </div>
      {:else}
        {#each notes as note (note.namespace)}
          <div class="note-item">
            <!-- Top row: namespace pill + date + remote status + actions -->
            <div class="note-header">
              <div class="note-meta">
                <span class="note-ns">{note.namespace}</span>
                <span class="note-date" use:tooltip={isoDate(note.created_at)}>
                  {formatDate(note.created_at)}
                </span>
                <!-- Remote status — hidden while unknown/checking -->
                {#if checkingRemote.has(note.namespace)}
                  <span class="remote-status status-checking" use:tooltip={'Checking remote…'}>
                    <RefreshCw size={10} class="spin" />
                  </span>
                {:else if note.remote_status !== 'unknown'}
                  {@const RemoteIcon = remoteIcon(note.remote_status)}
                  <span
                    class="remote-status {remoteClass(note.remote_status)}"
                    use:tooltip={remoteLabel(note.remote_status)}
                  >
                    <RemoteIcon size={10} />
                    <span class="status-label">{remoteLabel(note.remote_status)}</span>
                  </span>
                {/if}
              </div>

              <div class="note-actions">
                <button
                  class="icon-btn"
                  use:tooltip={'Refresh remote status'}
                  onclick={() => refreshRemoteStatus(note.namespace)}
                  disabled={checkingRemote.has(note.namespace)}
                >
                  <RefreshCw size={11} />
                </button>
                {#if editingNs !== note.namespace}
                  <button class="icon-btn" use:tooltip={'Edit'} onclick={() => startEdit(note)}>
                    <Pencil size={11} />
                  </button>
                {/if}
                <button class="icon-btn danger" use:tooltip={'Delete'} onclick={() => deleteNote(note.namespace)} disabled={saving}>
                  <Trash2 size={11} />
                </button>
              </div>
            </div>

            <!-- Content: view or edit -->
            {#if editingNs === note.namespace}
              <textarea
                class="note-textarea"
                bind:value={editContent}
                rows={5}
                onkeydown={(e) => { if (e.key === 'Escape') cancelEdit(); }}
              ></textarea>

              <div class="edit-actions">
                <button class="btn-secondary" onclick={cancelEdit} disabled={saving}>Cancel</button>

                <!-- Split button: Save / Save and push -->
                <div class="split-btn-wrap" class:open={splitOpen === note.namespace}>
                  <button
                    class="btn-primary split-main"
                    onclick={() => doSave(note.namespace, false)}
                    disabled={saving || !editContent.trim()}
                  >
                    <Check size={13} />
                    Save
                  </button>
                  <button
                    class="btn-primary split-chevron"
                    onclick={(e) => { e.stopPropagation(); splitOpen = splitOpen === note.namespace ? null : note.namespace; }}
                    disabled={saving}
                    use:tooltip={'More save options'}
                  >
                    <ChevronDown size={12} />
                  </button>
                  {#if splitOpen === note.namespace}
                    <div class="split-dropdown" role="menu" tabindex="-1">
                      <button class="split-option" role="menuitem" onclick={(e) => { e.stopPropagation(); doSave(note.namespace, true); }} disabled={saving}>
                        Save and push to origin
                      </button>
                    </div>
                  {/if}
                </div>
              </div>
            {:else}
              <div class="note-content-wrap">
                <pre class="note-content">{note.content}</pre>
              </div>
            {/if}
          </div>
        {/each}

        <!-- Inline add-new form -->
        {#if addingNew}
          <div class="note-item note-item-new">
            <div class="note-header">
              <span class="note-ns-label">Namespace</span>
            </div>
            <input
              class="ns-input"
              class:ns-input-error={nsError !== null && newNamespace.trim() !== ''}
              type="text"
              bind:value={newNamespace}
              placeholder="e.g. commits, review, jira"
              spellcheck="false"
            />
            {#if nsError && newNamespace.trim() !== ''}
              <span class="ns-error">{nsError}</span>
            {/if}
            <textarea
              class="note-textarea"
              bind:value={newContent}
              rows={4}
              placeholder="Note content…"
            ></textarea>

            <div class="edit-actions">
              <button class="btn-secondary" onclick={cancelAdd} disabled={saving}>Cancel</button>

              <!-- Split button: Add / Add and push -->
              <div class="split-btn-wrap" class:open={addSplitOpen}>
                <button
                  class="btn-primary split-main"
                  onclick={() => confirmAdd(false)}
                  disabled={saving || !newNamespace.trim() || !newContent.trim() || nsError !== null}
                >
                  <Plus size={13} />
                  Add
                </button>
                <button
                  class="btn-primary split-chevron"
                  onclick={(e) => { e.stopPropagation(); addSplitOpen = !addSplitOpen; }}
                  disabled={saving || !newNamespace.trim() || !newContent.trim() || nsError !== null}
                  use:tooltip={'More options'}
                >
                  <ChevronDown size={12} />
                </button>
                {#if addSplitOpen}
                  <div class="split-dropdown" role="menu" tabindex="-1">
                    <button class="split-option" role="menuitem" onclick={(e) => { e.stopPropagation(); confirmAdd(true); }} disabled={saving}>
                      Add and push to origin
                    </button>
                  </div>
                {/if}
              </div>
            </div>
          </div>
        {:else if notes.length > 0}
          <button class="add-note-btn" onclick={startAdd}>
            <Plus size={13} />
            Add note
          </button>
        {/if}
      {/if}
  </div>
</Modal>

<!-- Close split dropdowns on outside click -->
<svelte:window onclick={() => { splitOpen = null; addSplitOpen = false; }} />

<style>
  .commit-ref {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }

  /* ── Body ── */
  .notes-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── Empty state ── */
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 28px 0 20px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  :global(.empty-icon) { color: var(--text-disabled); }

  /* ── Note card ── */
  .note-item {
    background: var(--bg-overlay);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    transition: border-color var(--transition-fast);
  }
  .note-item:hover { border-color: var(--border); }
  .note-item-new { border-style: dashed; border-color: var(--border); }

  .note-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  /* Left: namespace + date + remote status */
  .note-meta {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
    flex-wrap: wrap;
    row-gap: 2px;
  }
  .note-ns {
    font-family: var(--font-code);
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    background: var(--accent-subtle);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .note-date {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--text-disabled);
    white-space: nowrap;
    cursor: default;
  }
  .note-ns-label {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    color: var(--text-disabled);
  }

  /* Remote status badge */
  .remote-status {
    display: flex;
    align-items: center;
    gap: 3px;
    font-family: var(--font-ui-sans);
    font-size: 10px;
    white-space: nowrap;
  }
  .status-label { opacity: 0.85; }
  .status-synced  { color: var(--success, #57a64a); }
  .status-ahead   { color: var(--warning, #e2a335); }
  .status-local   { color: var(--text-muted); }
  .status-checking { color: var(--text-disabled); }

  .note-actions {
    display: flex;
    align-items: center;
    gap: 2px;
    flex-shrink: 0;
    opacity: 0.5;
    transition: opacity var(--transition-fast);
  }
  .note-item:hover .note-actions { opacity: 1; }
  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px; height: 22px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .icon-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .icon-btn.danger:hover { color: var(--error, #e05252); }
  .icon-btn:disabled { opacity: 0.35; cursor: default; }

  /* Content */
  .note-content-wrap {
    border-left: 2px solid var(--accent-subtle);
    padding-left: 9px;
    margin-left: 1px;
  }
  .note-content {
    font-family: var(--font-code);
    font-size: 12px;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    line-height: 1.6;
  }

  .note-textarea {
    width: 100%;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    line-height: 1.55;
    padding: 7px 9px;
    resize: vertical;
    box-sizing: border-box;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .note-textarea:focus { border-color: var(--accent); }

  .ns-input {
    width: 100%;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 12px;
    padding: 5px 9px;
    box-sizing: border-box;
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .ns-input:focus { border-color: var(--accent); }
  .ns-input-error { border-color: var(--error, #e05252) !important; }
  .ns-error {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    color: var(--error, #e05252);
    margin-top: -4px;
  }

  .edit-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 7px;
  }

  /* ── Split button ── */
  .split-btn-wrap {
    position: relative;
    display: flex;
  }
  .split-main {
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    border-right: 1px solid rgba(255,255,255,0.15);
  }
  .split-chevron {
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    padding: 5px 7px;
  }
  .split-dropdown {
    position: absolute;
    bottom: calc(100% + 4px);
    right: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: 0 6px 20px rgba(0,0,0,0.35);
    min-width: 180px;
    z-index: var(--z-above);
    overflow: hidden;
    animation: fadeIn var(--anim-dur-fast) ease;
  }
  .split-option {
    display: block;
    width: 100%;
    padding: 8px 12px;
    text-align: left;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .split-option:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }
  .split-option:disabled { opacity: 0.4; cursor: default; }

  .add-note-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    padding: 5px 10px;
    background: transparent;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    transition: border-color var(--transition-fast), color var(--transition-fast), background var(--transition-fast);
  }
  .add-note-btn:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-subtle); }

  /* ── Shared buttons ── */
  .btn-primary {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    background: var(--accent);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-on-accent);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.45; cursor: default; }

  .btn-secondary {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    background: var(--bg-hover);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-xs);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  .btn-secondary:hover:not(:disabled) { background: var(--bg-overlay); }
  .btn-secondary:disabled { opacity: 0.45; cursor: default; }

  /* ── Animations ── */
  @keyframes fadeIn  { from { opacity: 0 } to { opacity: 1 } }
</style>
