/**
 * Which panel of Imber's tray (the bar over the composer) is open: the plan, the sub-agents, or the edits waiting
 * for review. One at a time; the palette opens them too, which is why the choice lives here and not in the bar.
 */
export type TrayPanel = 'plan' | 'agents' | 'edits';

function createAssistantTrayStore() {
  let open = $state<TrayPanel | null>(null);
  return {
    get open() { return open; },
    /** Open `panel`, or close it when it is the one open. */
    toggle(panel: TrayPanel) { open = open === panel ? null : panel; },
    show(panel: TrayPanel) { open = panel; },
    close() { open = null; },
  };
}

export const bennuAssistantTray = createAssistantTrayStore();
