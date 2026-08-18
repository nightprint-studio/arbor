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
  return projectStore.isCargo
    ? (bennuCargoTestStore as unknown as TestTreeStore)
    : (bennuTestStore as unknown as TestTreeStore);
}

/** Whether the open project's tests are run by cargo — for the few places that must know (the
 *  ignored-tests toggle exists only there, and the Java catalogue's columns say `class`). */
export function testsAreCargo(): boolean {
  return projectStore.isCargo;
}
