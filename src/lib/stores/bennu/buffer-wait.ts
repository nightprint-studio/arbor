/**
 * Wait until the editor shows a file, and read its buffer.
 *
 * Opening a file is asynchronous and the editor takes the new document a tick or two later, so an
 * edit sent straight after `openFile` would land in the file that was showing before. Polled because
 * nothing announces that the buffer is in; forty short waits cap it at a second.
 *
 * `expected` is the text the caller computed its edit against: the wait ends early when the buffer
 * matches it, and otherwise returns the last buffer seen so the caller can tell the two apart.
 */

import { projectStore } from './project.svelte';

export async function bufferOf(
  file: string,
  expected: string | null,
  read: () => string | null,
): Promise<string | null> {
  let seen: string | null = null;
  for (let i = 0; i < 40; i++) {
    if (projectStore.activeFilePath === file) {
      seen = read();
      if (seen !== null && (expected === null || seen === expected)) return seen;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  return seen;
}
