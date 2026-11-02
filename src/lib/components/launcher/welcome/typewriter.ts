/** Typing and wiping text a character at a time, for the welcome page's greeting and prompt. */

export interface Typist {
  /** The text currently shown. */
  set: (text: string) => void;
  sleep: (ms: number) => Promise<void>;
  /** `true` once a newer run has taken over: the old one stops at its next step. */
  cancelled: () => boolean;
}

/** Type `text` in, with the small irregularities of a real keyboard. Resolves `false` if cancelled. */
export async function write(t: Typist, text: string, speed = 52): Promise<boolean> {
  for (let i = 1; i <= text.length; i++) {
    if (t.cancelled()) return false;
    t.set(text.slice(0, i));
    await t.sleep(speed + Math.random() * speed * 0.7);
  }
  return true;
}

/** Wipe `text` out from the end, faster than it was typed. Resolves `false` if cancelled. */
export async function wipe(t: Typist, text: string, speed = 22): Promise<boolean> {
  for (let n = text.length - 1; n >= 0; n--) {
    if (t.cancelled()) return false;
    t.set(text.slice(0, n));
    await t.sleep(speed);
  }
  return true;
}
