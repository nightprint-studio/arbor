/**
 * The credit meter's state, shared by every title bar that shows it (Bennu, Corvus).
 *
 * Read in the background — when a meter first appears, every few minutes while one is on screen, and after each
 * answer (the assistant stores call `refresh()`) — never on the path of a request to the model. A failure keeps the
 * last figures and says why, rather than blanking the meter.
 */
import { getImberCredits, type CreditSummary } from '$lib/ipc/imber-credits';
import { periodStarts } from '$lib/components/bennu/assistant/assistant-periods';

/** How often an on-screen meter reads again by itself. */
const EVERY_MS = 5 * 60 * 1000;

function createImberCreditsStore() {
  let summary = $state<CreditSummary | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let users = 0;
  let timer: ReturnType<typeof setInterval> | null = null;

  async function refresh(force = false) {
    if (loading) return;
    loading = true;
    try {
      summary = await getImberCredits(periodStarts(), force);
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  return {
    get summary() { return summary; },
    get loading() { return loading; },
    get error() { return error; },
    refresh,
    /** A meter appeared: read now, and every few minutes while any is shown. Returns the matching release. */
    attach(): () => void {
      users++;
      if (users === 1) {
        void refresh();
        timer = setInterval(() => void refresh(), EVERY_MS);
      }
      return () => {
        users = Math.max(0, users - 1);
        if (users === 0 && timer) {
          clearInterval(timer);
          timer = null;
        }
      };
    },
  };
}

export const imberCreditsStore = createImberCreditsStore();
