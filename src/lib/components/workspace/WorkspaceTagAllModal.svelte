<script lang="ts">
  import { onMount, tick } from 'svelte';
  import {
    Tag, Upload, AlertTriangle, ArrowDown, CircleDot, FolderX,
  } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/workspaces.svelte';
  import { workspaceHealthScan, workspaceTagAll } from '$lib/ipc/workspace';
  import type { RepoHealth, WorkspaceDef } from '$lib/types/workspace';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import StatusList, { type StatusItem } from '$lib/components/shared/ui/StatusList.svelte';

  interface Props {
    workspace: WorkspaceDef;
    onClose:   () => void;
    /** Fired right after the backend returns a job_id, BEFORE the modal
     *  closes — lets the parent set up its OpState so progress events
     *  arriving immediately after have a target to mutate. */
    onStarted?: (info: { jobId: string; total: number; tagName: string; push: boolean }) => void;
  }
  let { workspace, onClose, onStarted }: Props = $props();

  let tagName    = $state('');
  let message    = $state('');
  let nameInput: HTMLInputElement | undefined;
  let scanning   = $state(true);
  let submitting = $state(false);
  let healths    = $state<RepoHealth[]>([]);

  onMount(async () => {
    await tick();
    nameInput?.focus();
    try {
      healths = await workspaceHealthScan(workspace.id);
    } catch (e) {
      uiStore.showToast(`Health scan failed: ${e}`, 'error');
    } finally {
      scanning = false;
    }
  });

  function nameOf(repoId: string): string {
    return workspacesStore.registryById.get(repoId)?.display_name ?? repoId.slice(0, 8);
  }

  // Map RepoHealth → StatusList items. The widget itself doesn't know about
  // git: every flag is just a severity-coded chip.
  const items = $derived<StatusItem[]>(healths.map(h => ({
    id:    h.repo_id,
    label: nameOf(h.repo_id),
    chips: [
      ...(h.missing  ? [{ severity: 'block' as const, icon: FolderX,        text: 'path missing' }] : []),
      ...(h.detached ? [{ severity: 'block' as const, icon: CircleDot,      text: 'detached HEAD — will be skipped' }] : []),
      ...(h.error    ? [{ severity: 'block' as const, icon: AlertTriangle,  text: h.error }] : []),
      ...(h.behind > 0 ? [{ severity: 'warn' as const, icon: ArrowDown,     text: `${h.behind} commit${h.behind === 1 ? '' : 's'} behind upstream` }] : []),
      ...(h.dirty      ? [{ severity: 'warn' as const, icon: AlertTriangle, text: 'local modifications' }] : []),
      ...(h.conflicted ? [{ severity: 'warn' as const, icon: AlertTriangle, text: 'merge in progress' }] : []),
    ],
  })));

  const tagValid = $derived(/^[A-Za-z0-9._\-/]+$/.test(tagName.trim()));

  async function submit(push: boolean) {
    const trimmed = tagName.trim();
    if (!trimmed) { nameInput?.focus(); return; }
    if (!tagValid) {
      uiStore.showToast('Tag name contains invalid characters', 'error');
      nameInput?.focus();
      return;
    }
    submitting = true;
    try {
      const res = await workspaceTagAll(workspace.id, trimmed, message.trim() || null, push);
      onStarted?.({ jobId: res.job_id, total: res.total, tagName: trimmed, push });
      uiStore.showToast(
        push
          ? `Tagging "${trimmed}" + push started for ${workspace.name}`
          : `Tagging "${trimmed}" started for ${workspace.name}`,
        'info',
      );
      onClose();
    } catch (e) {
      uiStore.showToast(`Tag failed: ${e}`, 'error');
    } finally {
      submitting = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void submit(false);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<Modal {onClose} width="560px" ariaLabel={`Tag workspace ${workspace.name}`}>
  {#snippet header()}
    <ModalHeader {onClose}>
      <Tag size={16} />
      <span class="modal-title">Tag workspace</span>
      <span class="ws-pill">{workspace.name}</span>
    </ModalHeader>
  {/snippet}

  <div class="body">
    <div class="field">
      <label for="tag-name">Tag name</label>
      <input
        id="tag-name"
        bind:this={nameInput}
        bind:value={tagName}
        placeholder="e.g. v1.4.0 or release-2026.04"
        autocomplete="off"
        spellcheck="false"
        maxlength="120"
      />
      {#if tagName.trim() && !tagValid}
        <div class="field-hint err">
          Use letters, digits, <code>.</code> <code>_</code> <code>-</code> <code>/</code>.
        </div>
      {/if}
    </div>

    <div class="field">
      <label for="tag-message">
        Message <span class="optional">(optional — annotated tag when present)</span>
      </label>
      <textarea
        id="tag-message"
        bind:value={message}
        placeholder="Release notes, context, ticket reference…"
        rows="3"
      ></textarea>
    </div>

    <StatusList
      items={items}
      totalCount={workspace.repo_ids.length}
      scanning={scanning}
      scanningLabel="Scanning repositories…"
      noun={{ singular: 'repository', plural: 'repositories' }}
      footnote="Blocked repos are skipped. Warning repos are still tagged at HEAD — review the list before continuing."
    />
  </div>

  {#snippet footer()}
    <Button variant="ghost" onclick={onClose}>Cancel</Button>
    <SplitButton
      label={submitting ? 'Starting…' : 'Create tag'}
      variant="primary"
      direction="up"
      disabled={!tagName.trim() || !tagValid || submitting || scanning}
      options={[
        { id: 'tag',       label: 'Create tag',         icon: Tag    },
        { id: 'tag-push',  label: 'Create tag & push',  icon: Upload },
      ]}
      onclick={() => submit(false)}
      onselect={(id) => {
        if (id === 'tag')      void submit(false);
        else if (id === 'tag-push') void submit(true);
      }}
    />
  {/snippet}
</Modal>

<style>
  .ws-pill {
    font-size: 11px;
    font-weight: 500;
    color: var(--accent);
    background: var(--accent-subtle);
    padding: 2px 8px;
    border-radius: var(--radius-md);
  }

  .body {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 11px 14px;
  }
  .field label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    font-weight: 600;
  }
  .field label .optional {
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
    color: var(--text-disabled);
    margin-left: 4px;
  }

  input, textarea {
    padding: 7px 10px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    resize: vertical;
  }
  input:focus, textarea:focus { outline: none; border-color: var(--accent); }
  textarea { min-height: 60px; font-family: var(--font-code); font-size: 12px; }

  .field-hint.err {
    font-size: 10.5px;
    color: var(--error);
  }
  .field-hint code {
    background: var(--bg-overlay);
    padding: 0 4px;
    border-radius: var(--radius-sm);
    font-size: 10px;
  }
</style>
