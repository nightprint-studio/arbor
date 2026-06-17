import { invoke } from '@tauri-apps/api/core';
import { platform } from '$lib/ipc/rpc';
import type { JobInfo } from '$lib/feedback/types/jobs';

export const listJobs = () =>
  platform<JobInfo[]>('list_jobs');

export const getJobOutput = (jobId: string) =>
  platform<string[]>('get_job_output', { job_id: jobId });

// DEFERRED: cancel_job signals a live child process and races with the
// `arbor://job-done` emit, so it stays a shell command for now.
export const cancelJob = (jobId: string) =>
  invoke<void>('cancel_job', { jobId });

export const runningJobCount = () =>
  platform<number>('running_job_count');

export const dismissJob = (jobId: string) =>
  platform<boolean>('dismiss_job', { job_id: jobId });

export const clearFinishedJobs = () =>
  platform<string[]>('clear_finished_jobs');
