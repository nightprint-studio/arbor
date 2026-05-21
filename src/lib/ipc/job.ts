import { invoke } from '@tauri-apps/api/core';
import type { JobInfo } from '$lib/types/jobs';

export const listJobs = () =>
  invoke<JobInfo[]>('list_jobs');

export const getJobOutput = (jobId: string) =>
  invoke<string[]>('get_job_output', { jobId });

export const cancelJob = (jobId: string) =>
  invoke<void>('cancel_job', { jobId });

export const runningJobCount = () =>
  invoke<number>('running_job_count');

export const dismissJob = (jobId: string) =>
  invoke<boolean>('dismiss_job', { jobId });

export const clearFinishedJobs = () =>
  invoke<string[]>('clear_finished_jobs');
