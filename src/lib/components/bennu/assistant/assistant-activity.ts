/**
 * What the assistant is doing, in words, for the line that shows it is working.
 *
 * The backend reports a phase (thinking, writing, using a tool, compacting) and, for a tool, its name. This
 * turns that into the present-tense phrase the thread shows — "Reading", "Searching", "Planning a
 * refactoring" — and the elapsed time into a short figure. Pure, so it is tested.
 */

export type ActivityPhase = 'thinking' | 'writing' | 'tool' | 'compacting';

export interface Activity {
  phase: ActivityPhase;
  /** The tool's name, in the `tool` phase. */
  tool?: string;
  /** Which request of the run this is (1 for the first). */
  iteration: number;
}

const TOOL_WORDS: [RegExp, string][] = [
  [/^ask_user$/, 'Waiting for your answer'],
  [/^update_plan$/, 'Updating the plan'],
  [/^set_title$/, 'Naming the conversation'],
  [/^load_skill$/, 'Loading a skill'],
  [/^brp_(mutate|insert|remove|spawn|despawn|set_resource|reparent|call|key|type|mouse|gamepad|action|set_state)$/, 'Changing the running game'],
  [/^brp_/, 'Looking into the running game'],
  [/^(screenshot_page|page_diagnostics)$/, 'Looking at the page'],
  [/^(edit_file|create_file)$/, 'Editing'],
  [/^delete_file$/, 'Deleting'],
  [/^merula_render$/, 'Rendering audio'],
  [/^(rename_symbol|add_parameter)$/, 'Planning a refactoring'],
  [/^(search_text|find_files)$/, 'Searching'],
  [/(problems|check|validate|compile)/, 'Checking'],
  [/test/, 'Running tests'],
  [/(usages|references|callers|implement)/, 'Finding usages'],
  [/(read|contents|declarations|structure)/, 'Reading'],
];

/** The phrase for an activity, without a trailing ellipsis. */
export function activityLabel(a: Activity): string {
  switch (a.phase) {
    case 'thinking': return 'Thinking';
    case 'writing': return 'Writing';
    case 'compacting': return 'Compacting the conversation';
    case 'tool': {
      const name = a.tool ?? '';
      return TOOL_WORDS.find(([re]) => re.test(name))?.[1] ?? 'Looking into the project';
    }
  }
}

/** `0:07`, `1:32` — the time a run has been going. */
export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** From how long a step is worth timing on its own: shorter, and the run's clock beside it says enough. */
const STEP_CLOCK_FROM_MS = 10_000;

/**
 * What the step in progress has to show besides its phrase: what the tool says about itself (`Compiling voriz`,
 * `12 passed, 1 failed`, with `3/40` when it knows a count) and, once the step has taken a while, how long it has
 * taken — so a test run of minutes reads as working, not as stuck. Empty when there is nothing to add.
 */
export function progressLine(progress: { message: string; done?: number; total?: number } | null, stepMs: number): string {
  const parts: string[] = [];
  if (progress?.message) {
    const count = progress.total ? ` ${progress.done ?? 0}/${progress.total}` : '';
    parts.push(`${progress.message}${count}`);
  }
  if (stepMs >= STEP_CLOCK_FROM_MS) parts.push(`${formatElapsed(stepMs)} in this step`);
  return parts.join(' · ');
}
