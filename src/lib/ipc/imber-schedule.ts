/**
 * Imber schedules over the rpc seam. Corvus and Bennu serve the same five methods with the same shapes,
 * so the wrappers are written once and bound per product (`corvus/imber-schedule.ts`, `bennu/imber-schedule.ts`).
 */

import { rpc } from './rpc';
import type { ImberJob, ImberJobInput } from '$lib/types/imber-schedule';

export type ImberScheduleProduct = 'corvus' | 'bennu';

/** The Tauri topic a product's schedule events (`started`, `failed`) arrive on. */
export const imberScheduleTopic = (product: ImberScheduleProduct) => `arbor://${product}/imber-schedule`;

export function imberScheduleApi(product: ImberScheduleProduct) {
  return {
    /** Every schedule, soonest first. The first call also starts the loop that fires them. */
    list: () => rpc<ImberJob[]>(product, 'imber_schedule_list'),
    save: (job: ImberJobInput) => rpc<ImberJob>(product, 'imber_schedule_save', { job }),
    remove: (id: string) => rpc<void>(product, 'imber_schedule_delete', { id }),
    setEnabled: (id: string, enabled: boolean) => rpc<ImberJob>(product, 'imber_schedule_set_enabled', { id, enabled }),
    /** Run now, outside its times; its next run is unchanged. */
    runNow: (id: string) => rpc<void>(product, 'imber_schedule_run_now', { id }),
  };
}
