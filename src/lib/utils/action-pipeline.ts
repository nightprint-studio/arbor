/**
 * Tiny sequential action pipeline used by the deep-link dispatcher (and any
 * other multi-step UI flow) to coordinate async work that mixes IPC, store
 * mutations and layout transitions.
 *
 * Why not just `await a(); await b(); await c()`?  Because some of the
 * "wait for layout to settle" / "wait for animation" steps don't have a
 * natural Promise to await, so the linear flow ends up sprinkled with
 * magic `requestAnimationFrame` and `setTimeout` calls that are easy to
 * misread.  Wrapping each beat as a named step makes the sequence explicit:
 *
 * ```ts
 * await runPipeline([
 *   step('open-bottom-panel', () => uiStore.setActiveBottomSection('detail')),
 *   delay(220, 'wait-slide-in'),         // CSS transition duration
 *   step('scroll-to-commit', () => graphStore.scrollToCommit(sha)),
 *   step('load-detail', async () => {
 *     const [detail, files] = await Promise.all([...]);
 *     graphStore.setDetail(detail);
 *     diffStore.setFiles(files);
 *   }),
 * ]);
 * ```
 *
 * Failures abort the pipeline (subsequent steps don't run) and re-throw the
 * original error after a structured `console.error`.
 */

export interface PipelineStep {
  name: string;
  /** Resolved Promise = step is genuinely done.  May be sync (return void). */
  run(): Promise<void> | void;
}

/** Run `steps` in order, awaiting each fully before starting the next. */
export async function runPipeline(steps: PipelineStep[]): Promise<void> {
  for (const s of steps) {
    try {
      await s.run();
    } catch (e) {
      console.error(`[pipeline] step "${s.name}" failed:`, e);
      throw e;
    }
  }
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

/** A named step wrapping an arbitrary sync/async function. */
export function step(name: string, fn: () => Promise<void> | void): PipelineStep {
  return { name, run: fn };
}

/** Wait `ms` milliseconds — the bluntest tool, used when a step depends on
 *  a CSS transition whose `transitionend` event isn't easy to subscribe to
 *  (e.g. Svelte transition). */
export function delay(ms: number, name = `delay-${ms}ms`): PipelineStep {
  return { name, run: () => new Promise<void>(r => setTimeout(r, ms)) };
}

/** Wait `n` animation frames — useful between Svelte state changes and
 *  layout-dependent reads, since rAF callbacks fire after style + layout. */
export function waitFrames(n: number, name = `wait-${n}-frames`): PipelineStep {
  return {
    name,
    async run() {
      for (let i = 0; i < n; i++) {
        await new Promise<void>(r => requestAnimationFrame(() => r()));
      }
    },
  };
}

/** Conditional step — included in the pipeline only when `predicate()` is
 *  true at run time.  Returns a no-op step when the predicate is false so
 *  the array shape stays predictable. */
export function when(predicate: () => boolean, included: PipelineStep): PipelineStep {
  return {
    name: `when[${included.name}]`,
    async run() { if (predicate()) await included.run(); },
  };
}

/**
 * Wait for the next Svelte intro transition on the element matched by
 * `selector` to complete.  Resolves immediately if the element is already
 * mounted (no intro queued).  Uses a safety timeout so a missed event
 * doesn't hang the pipeline forever.
 *
 * Pairs with elements that use `transition:slide` / `transition:fly` /
 * any custom Svelte transition — they all dispatch `introend` once the
 * animation finishes (Svelte transitions emit it on the host element).
 */
export function awaitIntroEnd(
  selector: string,
  timeoutMs = 500,
  name = `await-introend(${selector})`,
): PipelineStep {
  return {
    name,
    run() {
      return new Promise<void>(resolve => {
        // Element may not be in the DOM yet — give Svelte a frame to mount it
        // before binding the listener.  If after one frame it's still missing,
        // there's nothing to wait for.
        requestAnimationFrame(() => {
          const el = document.querySelector(selector);
          if (!el) { resolve(); return; }

          let done = false;
          const finish = () => {
            if (done) return;
            done = true;
            el.removeEventListener('introend', finish);
            clearTimeout(timeoutId);
            resolve();
          };
          el.addEventListener('introend', finish);

          // Safety net — fires when the element was already mounted before
          // we got here (no intro transition will play, so no event).
          // Tuned just above the longest panel transition (~250ms) plus
          // a small buffer to absorb event-loop jitter.
          const timeoutId = window.setTimeout(finish, timeoutMs);
        });
      });
    },
  };
}
