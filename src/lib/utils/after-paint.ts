/**
 * Run `fn` once the current change has been painted — and not at all if the caller's effect
 * re-runs first.
 *
 * Meant to be **returned from a `$effect`**:
 *
 * ```ts
 * $effect(() => {
 *   const root = projectStore.project?.root;
 *   if (!root) return;
 *   return afterPaint(() => void store.load(root), 300);
 * });
 * ```
 *
 * Why: switching project re-fires a dozen effects, each starting its own backend walk (framework
 * overview, test discovery, diagnostics, run configs…). Started in the same tick they compete with
 * the render of the very screen the user is waiting for, and flipping between two projects with
 * the keyboard starts the whole set again for a project that is gone a moment later. Waiting two
 * frames lets the swap paint first; the delay, and the cleanup that cancels a pending run, make a
 * quick A → B → A cost nothing at all.
 *
 * @param delayMs extra wait after the second frame — stagger heavy walks so they do not start
 *   together.
 * @returns the cleanup an effect expects.
 */
export function afterPaint(fn: () => void, delayMs = 0): () => void {
  let cancelled = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let raf2 = 0;
  const raf1 = requestAnimationFrame(() => {
    raf2 = requestAnimationFrame(() => {
      timer = setTimeout(() => {
        if (!cancelled) fn();
      }, delayMs);
    });
  });
  return () => {
    cancelled = true;
    cancelAnimationFrame(raf1);
    cancelAnimationFrame(raf2);
    if (timer !== undefined) clearTimeout(timer);
  };
}
