/**
 * The question asked when Corvus is sent to a repository it does not know: "add it to a workspace?".
 *
 * `ask` resolves with the user's choice, or `null` when they cancelled. One question at a time — a second
 * request while one is open replaces nothing: it waits for the answer and then asks its own.
 */

import type { AddChoice } from '$lib/utils/repo-target';

export interface OpenRepoRequest { path: string; name: string }

function createOpenRepoPromptStore() {
  let request = $state<OpenRepoRequest | null>(null);
  let settle: ((c: AddChoice | null) => void) | null = null;
  let queue: Promise<unknown> = Promise.resolve();

  function present(req: OpenRepoRequest): Promise<AddChoice | null> {
    return new Promise((resolve) => {
      request = req;
      settle = resolve;
    });
  }

  return {
    get request() { return request; },

    ask(req: OpenRepoRequest): Promise<AddChoice | null> {
      const turn = queue.then(() => present(req));
      queue = turn.catch(() => {});
      return turn;
    },

    answer(choice: AddChoice | null) {
      const done = settle;
      settle = null;
      request = null;
      done?.(choice);
    },
  };
}

export const openRepoPrompt = createOpenRepoPromptStore();
