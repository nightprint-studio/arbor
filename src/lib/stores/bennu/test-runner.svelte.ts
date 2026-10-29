/**
 * Which test runner the panel is looking at — the one place that answers it.
 *
 * There are two runners because Maven and cargo model a test differently (see
 * {@link ./test-tree} for how differently), and exactly one of them applies to any open project.
 * So rather than every consumer asking `projectStore.isCargo ? … : …` — which is the shape that
 * drifts, because the day a third build system arrives there are eleven of them to find — the
 * panel components import `activeTestStore` and never learn which they got.
 *
 * The choice is by **project kind**, not by which store has results: a Cargo project with nothing
 * run yet must still show the cargo tree, and a Maven store holding last project's results must
 * not be what the panel draws.
 *
 * Both stores are attached at window mount regardless, because attaching is subscribing to events
 * and the events are cheap; each recognises its own runs by the run id's prefix.
 */

import { projectStore } from './project.svelte';
import { moduleFocusStore } from './module-focus.svelte';
import { bennuTestStore } from './tests.svelte';
import { bennuCargoTestStore } from './cargo-tests.svelte';
import type { TestTreeStore } from './test-tree';

/**
 * The runner for the open project.
 *
 * Typed as the shared interface on purpose: reaching for a Maven-only or cargo-only method through
 * this is a type error, which is what keeps the panel honest. When something genuinely needs one
 * ecosystem's verb — the project tree's "run the tests in this folder", which takes a Java class
 * name or a Rust case ref — it imports that store directly and guards on the kind itself.
 */
export function activeTestStore(): TestTreeStore {
  return testsAreCargo()
    ? (bennuCargoTestStore as unknown as TestTreeStore)
    : (bennuTestStore as unknown as TestTreeStore);
}

/** Whether the open project's tests are run by cargo — for the few places that must know (the
 *  ignored-tests toggle exists only there, and the Java catalogue's columns say `class`). */
export function testsAreCargo(): boolean {
  if (projectStore.isCargo) return true;
  // A project that is neither Maven nor Cargo at its root (a Tauri app is Node-first) can still carry
  // one in a subfolder; then the module the tests resolve to decides. Maven and Cargo roots keep
  // deciding by their own kind, exactly as before modules existed.
  return projectStore.isPlain && moduleFocusStore.targetFor('Maven', 'Cargo')?.kind === 'Cargo';
}

/**
 * The folder the tests run in: the project root, except for a project whose build tool lives in a
 * subfolder (see {@link testsAreCargo}) — then that module's folder. `null` with nothing to run.
 */
export function activeTestRoot(): string | null {
  if (!projectStore.isPlain) return projectStore.project?.root ?? null;
  return moduleFocusStore.targetFor('Maven', 'Cargo')?.toolRoot ?? null;
}
