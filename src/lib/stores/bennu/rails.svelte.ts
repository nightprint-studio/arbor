/**
 * Bennu's activity rails — the arrangement the user chose, and what it means for a rail
 * that changes shape with the project.
 *
 * ## Why this is not simply "a saved list"
 *
 * Bennu's rails are not a fixed set of buttons. Structure, Maven, Dependencies and Forms are
 * backed by the Java index or the Struts/Spring graph, so a Cargo root does not have them at
 * all; a Spring service with no controllers does not get Endpoints. What is on the rail is
 * derived, per project, every time.
 *
 * So the saved arrangement is a preference about ids, applied to whatever the project offers
 * — and `rail-order.ts` is where the three awkward cases (saved-but-absent, present-but-new,
 * never-hideable) are decided once for both products that have a rail.
 *
 * ## Persistence
 *
 * The shell's `~/.config/arbor/config.toml`, under `activity_bar.products.bennu`. Writing
 * goes through the whole-config setter, so every save **re-reads** first and puts back only
 * its own key: Corvus's bar lives in the same table, and a blind write would flatten it.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { getActivityBarConfig, setActivityBarConfig } from '$lib/ipc/config';
import type { ActivityBarItemConfig, ActivityBarSections } from '$lib/types/config';

/** The four clusters, named by where they are rather than by what Bennu puts there. */
export type RailSection = 'leftTop' | 'leftBottom' | 'rightTop' | 'rightBottom';

/** Which key in the persisted shape each cluster is. */
const CONFIG_KEY: Record<RailSection, keyof ActivityBarSections> = {
  leftTop: 'top_items',
  leftBottom: 'bottom_items',
  rightTop: 'right_top_items',
  rightBottom: 'right_bottom_items',
};

/**
 * The buttons that cannot be hidden.
 *
 * Only the project tree. It is the one rail button that is the way *in* — hide it on a
 * window with no file open and there is nothing on screen that can open one. Everything else
 * has a second route (the title bar, a shortcut, the Command Palette), which is what makes it
 * safe to take off the rail.
 */
export const BENNU_MANDATORY: ReadonlySet<string> = new Set(['project']);

function createBennuRailsStore() {
  let sections = $state<ActivityBarSections>({});
  let loaded = $state(false);

  async function load(): Promise<void> {
    try {
      const cfg = await getActivityBarConfig();
      sections = cfg.products?.bennu ?? {};
    } catch {
      sections = {};
    }
    loaded = true;
  }

  return {
    get loaded() { return loaded; },
    load,

    /** The saved arrangement of one cluster, or `undefined` when it has never been touched. */
    saved(section: RailSection): ActivityBarItemConfig[] | undefined {
      return sections[CONFIG_KEY[section]];
    },

    /**
     * Persist all four clusters at once.
     *
     * Re-reads the whole activity-bar config first: Corvus's four flat lists are in the same
     * table, and this setter replaces the table.
     */
    async save(next: ActivityBarSections): Promise<void> {
      const cfg = await getActivityBarConfig();
      await setActivityBarConfig({
        ...cfg,
        products: { ...(cfg.products ?? {}), bennu: next },
      });
      sections = next;
    },
  };
}

export const bennuRailsStore = createBennuRailsStore();
