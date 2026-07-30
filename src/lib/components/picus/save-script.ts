/**
 * Write the open script back — the one implementation, for both callers.
 *
 * Save is reachable from the toolbar button and from Ctrl+S, and those two have to
 * mean exactly the same thing: the same buffer, the same refusal, the same words
 * afterwards. Two copies of "read the editor, call the store, show a toast" is two
 * places for the message to drift and one place for the keyboard to quietly do
 * something the button does not.
 */
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { picusEditorStore } from '$lib/stores/picus/editor.svelte';
import { picusProjectStore } from '$lib/stores/picus/project.svelte';
import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';

/**
 * Save `path` from whatever the editor currently holds.
 *
 * Returns the failure's own words, or an empty string. The buffer is never
 * touched on failure: text that could not be written is the last thing to throw
 * away, and the usual refusal here is the useful one — a character the file's
 * declared encoding cannot represent, which is exactly what this product exists
 * to stop happening silently in other people's editors.
 */
export async function saveOpenScript(path: string): Promise<string> {
  const text = picusEditorStore.active?.getValue();
  if (!path || text === undefined) return '';

  const failure = await picusProjectStore.saveText(path, text);
  if (failure) {
    toastStore.show(`${path} was not written — ${failure}`, 'error');
    return failure;
  }
  // The tab is no longer unsaved, and it has to stop saying so. Every tab open on
  // this file, not just the active one: the same script can be open twice, and a
  // dot left behind on the other one is a lie about the disk.
  for (const t of picusTabsStore.tabs) {
    if (t.file === path && t.dirty) picusTabsStore.markDirty(t.id, false);
  }
  toastStore.show(`${path} saved — re-checking the repository.`, 'success');
  return '';
}
