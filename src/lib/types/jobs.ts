export type JobStatusType =
  | { type: 'running' }
  | { type: 'completed'; exit_code: number }
  | { type: 'failed';    error: string }
  | { type: 'cancelled' };

export interface JobInfo {
  id: string;
  name: string;
  plugin_name: string;
  command: string;
  started_at: number; // Unix seconds
  status: JobStatusType;
  category?: string;
  /** When true the UI hides the cancel/stop button. */
  non_cancellable?: boolean;
  /** When true the job is hidden from the default Jobs panel listing
   *  and the status-bar running-count badge. Revealed by the
   *  "Show hidden" toggle on the Jobs panels. */
  hidden?: boolean;
}

export function isRunning(j: JobInfo): boolean {
  return j.status.type === 'running';
}

export function isSuccess(j: JobInfo): boolean {
  return j.status.type === 'completed' && j.status.exit_code === 0;
}

export function statusLabel(j: JobInfo): string {
  switch (j.status.type) {
    case 'running':   return 'Running';
    case 'completed': return `Done (${j.status.exit_code})`;
    case 'failed':    return 'Failed';
    case 'cancelled': return 'Cancelled';
  }
}
