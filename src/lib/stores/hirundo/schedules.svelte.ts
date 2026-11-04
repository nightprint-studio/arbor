/**
 * Hirundo schedules — the list, and the tick that runs what is due.
 *
 * The schedules, their recurrence and the question "is this due?" belong to `hirundo-be` (which answers with
 * `arbor-scheduler`'s calendar rules, the same ones the Imber schedules use). This store asks every half
 * minute while the window is open, runs what it is handed with the runner it already has, and reports back.
 * Nothing fires while the window is closed.
 */
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { scheduleDelete, scheduleRan, scheduleSave, schedulesDue, schedulesList } from '$lib/ipc/hirundo';
import type { Schedule, ScheduleView } from '$lib/types/hirundo';
import { hirundoRunner } from './runner.svelte';
import { hirundoStore } from './workspace.svelte';

/** How often the window asks what is due. The calendar's grace is two minutes, so this is well inside it. */
const TICK_MS = 30_000;

function createHirundoSchedules() {
  let items = $state<ScheduleView[]>([]);
  let timer: ReturnType<typeof setInterval> | null = null;

  async function load() {
    const project = hirundoStore.project;
    items = project ? await schedulesList(project).catch(() => []) : [];
  }

  async function save(schedule: Schedule) {
    const project = hirundoStore.project;
    if (!project) return;
    await scheduleSave(project, schedule);
    await load();
  }

  async function remove(id: string) {
    const project = hirundoStore.project;
    if (!project) return;
    await scheduleDelete(project, id);
    await load();
  }

  async function toggle(s: ScheduleView) {
    await save({ ...s, enabled: !s.enabled });
  }

  /** Run one schedule now, whatever its time says. */
  async function runNow(s: Schedule) {
    await execute(s);
  }

  async function execute(s: Schedule) {
    const project = hirundoStore.project;
    if (!project) return;
    const result = await hirundoRunner.run(s.target, { confirmed: false, envId: s.env_id || undefined, source: `Schedule · ${s.name}` });
    if (!result) return;
    const ok = result.failed === 0;
    await scheduleRan(project, s.id, ok, `${result.passed} ok, ${result.failed} ko`).catch(() => {});
    if (!ok) toastStore.show(`Schedule “${s.name}”: ${result.failed} failed.`, 'warning');
    await load();
  }

  async function tick() {
    const project = hirundoStore.project;
    if (!project || hirundoRunner.running) return;
    try {
      for (const s of await schedulesDue(project)) {
        if (hirundoRunner.running) break;
        await execute(s);
      }
    } catch { /* the backend is not up yet: the next tick asks again */ }
  }

  return {
    get items() { return items; },
    load, save, remove, toggle, runNow,
    start() {
      if (timer) return;
      timer = setInterval(() => void tick(), TICK_MS);
      void tick();
    },
    stop() {
      if (timer) clearInterval(timer);
      timer = null;
    },
  };
}

export const hirundoSchedules = createHirundoSchedules();
