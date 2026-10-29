/**
 * What changing the model or the context size costs a conversation that is already long.
 *
 * Facts this rests on (Anthropic's prompt-caching and context-window docs):
 *  - A cache entry is made of an exact prefix; the docs do not promise one model's entries serve another,
 *    so a **model change** is treated as a cold cache: the whole history is written again at the cache-write
 *    price instead of being read at the cache-read price.
 *  - The **context size** here is Bennu's own ceiling, not a request parameter (1M is the default window and
 *    needs no beta header), so changing it does **not** invalidate the cache. It only matters when the new
 *    size is smaller than what the conversation fills: the next message compacts it first, which costs one
 *    summary request and then a rebuilt prefix.
 */

/** Below this many tokens the conversation is cheap to rebuild and nobody is asked. */
export const CONFIRM_ABOVE_TOKENS = 5_000;
/** A conversation is compacted at this share of the window (mirrors the backend). */
export const COMPACT_AT = 0.9;
/** Writing a prefix to the cache costs this much times the plain input price (5-minute cache). */
export const CACHE_WRITE_FACTOR = 1.25;
/** Reading a warm prefix costs this much times the plain input price. */
export const CACHE_READ_FACTOR = 0.1;

export type CostChange =
  | { kind: 'model'; tokens: number; euros: number; warmEuros: number }
  | { kind: 'context'; tokens: number; euros: number; warmEuros: number };

/** Euros to send `tokens` of input at `inputPricePerM` (dollars per million) times `factor`. */
export function inputEuros(tokens: number, inputPricePerM: number, factor: number, eurPerUsd: number): number {
  return (tokens / 1_000_000) * inputPricePerM * factor * eurPerUsd;
}

export interface ChangeInput {
  /** Tokens the conversation holds now (the context meter's estimate). */
  used: number;
  /** The price of the model the next request would use, dollars per million input tokens. */
  inputPricePerM: number;
  eurPerUsd: number;
  threshold?: number;
}

/** Changing the model of a conversation of `used` tokens: null when it is small enough not to ask. */
export function modelChange(i: ChangeInput): CostChange | null {
  if (i.used < (i.threshold ?? CONFIRM_ABOVE_TOKENS)) return null;
  return {
    kind: 'model',
    tokens: i.used,
    euros: inputEuros(i.used, i.inputPricePerM, CACHE_WRITE_FACTOR, i.eurPerUsd),
    warmEuros: inputEuros(i.used, i.inputPricePerM, CACHE_READ_FACTOR, i.eurPerUsd),
  };
}

/** Changing the context size to `newSize`: only a size the conversation already exceeds forces a compaction. */
export function contextChange(i: ChangeInput & { newSize: number }): CostChange | null {
  if (i.used < (i.threshold ?? CONFIRM_ABOVE_TOKENS)) return null;
  if (i.used < i.newSize * COMPACT_AT) return null;
  return {
    kind: 'context',
    tokens: i.used,
    // The summary request reads the history once; the rebuilt (shorter) prefix is written afterwards, which
    // is not estimated here — it is smaller than what was there.
    euros: inputEuros(i.used, i.inputPricePerM, CACHE_WRITE_FACTOR, i.eurPerUsd),
    warmEuros: inputEuros(i.used, i.inputPricePerM, CACHE_READ_FACTOR, i.eurPerUsd),
  };
}
