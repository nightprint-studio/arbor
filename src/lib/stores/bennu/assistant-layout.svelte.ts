/**
 * Whether the assistant panel is docked or expanded over the editor area.
 *
 * Purely the window's current arrangement, so it lives in memory and is not a setting: the docked width
 * is never touched (the panel is lifted over the editor without being resized), which is how it comes
 * back exactly as it was.
 */
function createAssistantLayoutStore() {
  let expanded = $state(false);

  return {
    get expanded() { return expanded; },
    expand() { expanded = true; },
    restore() { expanded = false; },
    toggle() { expanded = !expanded; },
  };
}

export const assistantLayoutStore = createAssistantLayoutStore();
