/**
 * One build at a time **per project**, queued — not one build at a time per window, and not
 * dropped.
 *
 * The run store used a single `building` flag. While project A compiled, a launch of project B
 * returned silently (the click was discarded, not deferred), so B could only start after A had
 * got as far as running — which is the wait it looked like. Two different projects share no
 * `target/` and no index, so nothing needs them to take turns; two builds of the SAME project do
 * (two `mvn` on one tree thrash it, and the backend refuses the second), so those line up.
 *
 * Pure and framework-free so the ordering rules are testable: the store hands it a callback to
 * learn when the set of busy projects changed.
 */

/**
 * The key a project root is filed under: separators unified, trailing separator dropped,
 * case-folded (paths on Windows are case-insensitive, and a root typed two ways must be one lock).
 */
export function rootKey(root: string): string {
  return root.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

export interface RootLocks {
  /** Run `task` once every earlier task of the same key has finished. Tasks of other keys do not wait. */
  run<T>(key: string, task: () => Promise<T>): Promise<T>;
  /** Whether a task of `key` is running or waiting. */
  isBusy(key: string): boolean;
  /** How many tasks of `key` are waiting behind the running one. */
  waiting(key: string): number;
  /** Every key with a task running or waiting. */
  busyKeys(): string[];
}

export function createRootLocks(onChange: () => void = () => {}): RootLocks {
  /** Per key: the promise the NEXT task waits on, and how many tasks are in the system. */
  const lanes = new Map<string, { tail: Promise<void>; count: number }>();

  return {
    async run<T>(key: string, task: () => Promise<T>): Promise<T> {
      const lane = lanes.get(key) ?? { tail: Promise.resolve(), count: 0 };
      lanes.set(key, lane);
      lane.count += 1;
      onChange();

      const before = lane.tail;
      let release!: () => void;
      lane.tail = new Promise<void>((resolve) => {
        release = resolve;
      });
      // `before` never rejects: every task releases in `finally`, whatever it did.
      await before;
      try {
        return await task();
      } finally {
        lane.count -= 1;
        if (lane.count === 0) lanes.delete(key);
        release();
        onChange();
      }
    },

    isBusy: (key) => (lanes.get(key)?.count ?? 0) > 0,
    waiting: (key) => Math.max(0, (lanes.get(key)?.count ?? 0) - 1),
    busyKeys: () => [...lanes.keys()],
  };
}
