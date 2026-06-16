/**
 * nemus transcription-models store — install status of the on-demand ONNX models
 * (basic-pitch for polyphonic pitch, Demucs for stem separation) plus live
 * per-model download progress.
 *
 * Mirrors {@link packsStore}, but the models ride the **generic** job system
 * (`nemus_download_model` returns a job id; progress on `arbor://job-progress`,
 * completion on `arbor://job-done`) rather than the sample-pack-specific
 * `nemus:pack_progress` stream. Each download is also surfaced in the shared
 * Downloads & Exports overlay (`transfersStore`), keyed by model id.
 *
 * Subscribe once on mount (NemusShell), unlisten on teardown.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  nemusModels, nemusDownloadModel, nemusDeleteModel, type NemusModelStatus,
} from '$lib/ipc/nemus';
import { cancelJob } from '$lib/feedback/ipc/job';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';

interface JobProgress { job_id: string; pct: number; }
interface JobDone { job_id: string; success: boolean; error: string | null; }

function createModelsStore() {
  let models   = $state<NemusModelStatus[]>([]);
  // Keyed by model id; a model is "downloading" while it has a live job id.
  let progress = $state<Record<string, number | null>>({});
  let jobIds   = $state<Record<string, string>>({});
  let deleting = $state<Record<string, boolean>>({});

  function clearJob(id: string) {
    const next = { ...jobIds };
    delete next[id];
    jobIds = next;
  }

  /** The model id that owns `jobId`, if any is still in flight. */
  function modelForJob(jobId: string): string | undefined {
    return Object.keys(jobIds).find((id) => jobIds[id] === jobId);
  }

  async function refreshModels() {
    try { models = await nemusModels(); } catch { /* keep last */ }
  }

  return {
    /** All known models with their install status (display order). */
    get models() { return models; },
    /** Live download percent for a model (`null` = indeterminate / idle). */
    progressOf(id: string) { return progress[id] ?? null; },
    /** Whether a model has a download job in flight. */
    downloadingOf(id: string) { return jobIds[id] != null; },
    /** Whether a model is being deleted. */
    deletingOf(id: string) { return deleting[id] ?? false; },

    /** Re-read every model's install status from disk. */
    refresh() { return refreshModels(); },

    /** Start the download job for `id`. Progress flows via the subscription,
     *  mirrored into the shared Downloads & Exports overlay. */
    async download(id: string) {
      const model = models.find((m) => m.id === id);
      progress = { ...progress, [id]: null };
      transfersStore.start({
        id, kind: 'download', label: model?.name ?? id, sublabel: 'Starting…', progress: null,
        path: model?.path,
        cancel: () => { void this.cancel(id); },
      });
      try {
        const job = await nemusDownloadModel(id);
        jobIds = { ...jobIds, [id]: job };
      } catch (e) {
        // A Tauri rejection is the serialized AppError string — surface verbatim.
        transfersStore.fail(id, e instanceof Error ? e.message : String(e));
      }
    },

    /** Cancel a model's in-flight download. */
    async cancel(id: string) {
      const job = jobIds[id];
      if (!job) return;
      try { await cancelJob(job); } catch { /* already gone */ }
      clearJob(id);
      progress = { ...progress, [id]: null };
      transfersStore.cancelled(id);
    },

    /** Delete an installed model from disk, then re-read the list. Throws on
     *  failure so the caller can surface it. */
    async remove(id: string) {
      deleting = { ...deleting, [id]: true };
      try {
        await nemusDeleteModel(id);
        await refreshModels();
      } finally {
        deleting = { ...deleting, [id]: false };
      }
    },

    async subscribe(): Promise<UnlistenFn> {
      const unProgress = await listen<JobProgress>('arbor://job-progress', (e) => {
        const id = modelForJob(e.payload.job_id);
        if (!id) return;
        const pct = e.payload.pct >= 0 ? e.payload.pct : null;
        progress = { ...progress, [id]: pct };
        transfersStore.update(id, { progress: pct });
      });
      const unDone = await listen<JobDone>('arbor://job-done', (e) => {
        const id = modelForJob(e.payload.job_id);
        if (!id) return;
        clearJob(id);
        progress = { ...progress, [id]: null };
        if (e.payload.success) {
          transfersStore.finish(id);
          void refreshModels();
        } else {
          transfersStore.fail(id, e.payload.error ?? 'Download failed');
        }
      });
      return () => { unProgress(); unDone(); };
    },
  };
}

export const modelsStore = createModelsStore();
