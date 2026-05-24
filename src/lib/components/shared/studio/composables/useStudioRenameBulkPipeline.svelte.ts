/**
 * useStudioRenameBulkPipeline — owns the F12 "Rename across project…"
 * + F13 "Bulk edit by query" modal lifecycles shared by every Studio
 * modal. State is trivial; the dedup payoff is the post-apply flow
 * (success/failure notifications, index refresh, active-doc reload).
 *
 * Single-doc vs multi-tab divergence (Ron has tabs, others don't) is
 * abstracted via `reloadAfterDiskWrite(writtenPaths)`. The composable
 * uniformly calls it once per applied rename / bulk-edit; the wrapper
 * decides whether that means re-opening one doc or several tabs.
 */

import { studioStore } from '$lib/stores/studio.svelte';
import { notificationsStore } from '$lib/stores/notifications.svelte';
import type { StudioFileKind } from '$lib/ipc/studio';
import type {
  BulkEditOpenDoc, BulkEditResult,
  RenameOpenDoc, RenameResult,
} from '$lib/types/studio-format';

export interface RenameBulkConfig<TNode> {
  /** e.g. 'yaml' — used by `studioStore.loadCrossRefsForKind`. */
  formatId: StudioFileKind;
  /** Display label for notifications — e.g. 'YAML'. Unused for now,
   *  but kept so consumers don't need a second prop later. */
  formatLabel: string;
  getDocId:        () => string | null;
  getSourcePath:   () => string | null;
  getDirty:        () => boolean;
  getActiveTabId:  () => string | null;
  /** Pull the renameable value (without quotes) from a tree node.
   *  Wrappers typically wire this to `crossRefs.unquotedString(n.preview)`. */
  extractRenameValue: (node: TNode) => string | null;
  /** Re-read the active document(s) from disk after the rename / bulk
   *  edit wrote them. Single-doc formats re-open the same path + reload
   *  the tree; RON's multi-tab impl re-opens each touched tab. */
  reloadAfterDiskWrite: (writtenPaths: string[]) => Promise<void>;
  /** Apply the server-returned mutate state in-place — used by bulk-edit
   *  when the host already produced the new active-doc text and we want
   *  to skip the disk reload. */
  applyExternalActiveDocState: (state: NonNullable<BulkEditResult['active_doc_state']>) => Promise<void>;
  /** Optional override producing the open-docs snapshot used by both the
   *  rename and bulk-edit modals. Default = a single entry built from
   *  `getDocId / getSourcePath / getDirty`. RON overrides this to walk
   *  the workspace tab list. */
  buildOpenDocs?: () => RenameOpenDoc[];
}

export interface RenameBulkPipeline<TNode> {
  // Rename modal.
  readonly renameModalState: { oldValue: string } | null;
  /** Open the rename modal for a tree node — extracts the value via
   *  `config.extractRenameValue` and silently no-ops on empty. */
  openRenameModalForNode(node: TNode): void;
  closeRenameModal(): void;
  buildRenameOpenDocs(): RenameOpenDoc[];
  onRenameApplied(result: RenameResult): Promise<void>;

  // Bulk edit modal.
  readonly bulkEditModalState: { query: string } | null;
  openBulkEditModal(query: string): void;
  closeBulkEditModal(): void;
  buildBulkEditOpenDocs(): BulkEditOpenDoc[];
  onBulkEditApplied(result: BulkEditResult): Promise<void>;
}

function pathTouched(active: string | null, written: string[]): boolean {
  if (!active) return false;
  const norm = active.replace(/\\/g, '/').toLowerCase();
  return written.some(p => p.replace(/\\/g, '/').toLowerCase() === norm);
}

async function refreshCrossRefIndex(tabId: string | null, formatId: StudioFileKind): Promise<void> {
  if (!tabId) return;
  try { await studioStore.loadCrossRefsForKind(tabId, formatId, true); } catch { /* soft */ }
  try { await studioStore.refreshIndex(tabId); }                         catch { /* soft */ }
}

export function useStudioRenameBulkPipeline<TNode>(config: RenameBulkConfig<TNode>): RenameBulkPipeline<TNode> {
  let renameModalState   = $state<{ oldValue: string } | null>(null);
  let bulkEditModalState = $state<{ query: string }    | null>(null);

  function openRenameModalForNode(node: TNode): void {
    if (!config.getActiveTabId()) {
      notificationsStore.add(
        'Rename across project',
        `No active project — open this ${config.formatLabel} file from a project tab to rename across files.`,
        'warning',
      );
      return;
    }
    const value = config.extractRenameValue(node);
    if (!value) return;
    renameModalState = { oldValue: value };
  }
  function closeRenameModal(): void { renameModalState = null; }

  function buildRenameOpenDocs(): RenameOpenDoc[] {
    if (config.buildOpenDocs) return config.buildOpenDocs();
    const docId = config.getDocId();
    if (!docId) return [];
    return [{
      doc_id:      docId,
      source_path: config.getSourcePath(),
      dirty:       config.getDirty(),
    }];
  }

  async function onRenameApplied(result: RenameResult): Promise<void> {
    closeRenameModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    // Multi-tab wrappers (e.g. RON) set `buildOpenDocs` and own the
    // decision of which tabs to reload inside the callback. Single-tab
    // wrappers only need the reload when the active doc was touched.
    const shouldReload = config.buildOpenDocs
      ? written.length > 0
      : pathTouched(config.getSourcePath(), written);
    if (shouldReload) {
      try { await config.reloadAfterDiskWrite(written); }
      catch (e) { console.warn('rename: active doc reload failed', e); }
    }
    await refreshCrossRefIndex(config.getActiveTabId(), config.formatId);

    if (failed.length === 0) {
      notificationsStore.add(
        'Rename across project',
        `Renamed in ${written.length} ${written.length === 1 ? 'file' : 'files'}.`,
        'success',
      );
    } else {
      const lines = failed.map(f => `· ${f.absolute_path}: ${f.message}`).join('\n');
      notificationsStore.add(
        'Rename across project',
        `Renamed in ${written.length} ${written.length === 1 ? 'file' : 'files'}, `
          + `but ${failed.length} ${failed.length === 1 ? 'file' : 'files'} could not be written:\n${lines}`,
        'warning',
      );
    }
  }

  function openBulkEditModal(query: string): void {
    if (!config.getActiveTabId()) {
      notificationsStore.add(
        'Bulk edit by query',
        `No active project — open this ${config.formatLabel} file from a project tab to run a bulk edit.`,
        'warning',
      );
      return;
    }
    if (!config.getDocId()) return;
    if (!query) return;
    bulkEditModalState = { query };
  }
  function closeBulkEditModal(): void { bulkEditModalState = null; }

  function buildBulkEditOpenDocs(): BulkEditOpenDoc[] {
    return buildRenameOpenDocs();
  }

  async function onBulkEditApplied(result: BulkEditResult): Promise<void> {
    closeBulkEditModal();
    const written = result.written_files ?? [];
    const failed  = result.failed_files  ?? [];

    if (result.active_doc_state) {
      try { await config.applyExternalActiveDocState(result.active_doc_state); }
      catch (e) { console.warn('bulk edit: active-doc sync failed', e); }
    } else {
      const shouldReload = config.buildOpenDocs
        ? written.length > 0
        : pathTouched(config.getSourcePath(), written);
      if (shouldReload) {
        try { await config.reloadAfterDiskWrite(written); }
        catch (e) { console.warn('bulk edit: active doc reload failed', e); }
      }
      await refreshCrossRefIndex(config.getActiveTabId(), config.formatId);
    }

    const appliedTxt = `${result.applied_sites} ${result.applied_sites === 1 ? 'site' : 'sites'}`;
    const skippedTxt = result.skipped_sites > 0
      ? ` (${result.skipped_sites} skipped)`
      : '';
    if (failed.length === 0) {
      notificationsStore.add(
        'Bulk edit',
        result.active_doc_state
          ? `Applied to ${appliedTxt}${skippedTxt} in this doc.`
          : `Applied to ${appliedTxt}${skippedTxt} across ${written.length} ${written.length === 1 ? 'file' : 'files'}.`,
        'success',
      );
    } else {
      const lines = failed.map(f => `· ${f.absolute_path}: ${f.message}`).join('\n');
      notificationsStore.add(
        'Bulk edit',
        `Applied to ${appliedTxt}${skippedTxt} across ${written.length} ${written.length === 1 ? 'file' : 'files'}, `
          + `but ${failed.length} ${failed.length === 1 ? 'file' : 'files'} could not be written:\n${lines}`,
        'warning',
      );
    }
  }

  return {
    get renameModalState() { return renameModalState; },
    openRenameModalForNode,
    closeRenameModal,
    buildRenameOpenDocs,
    onRenameApplied,

    get bulkEditModalState() { return bulkEditModalState; },
    openBulkEditModal,
    closeBulkEditModal,
    buildBulkEditOpenDocs,
    onBulkEditApplied,
  };
}
