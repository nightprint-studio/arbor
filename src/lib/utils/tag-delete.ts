import { deleteTag } from '$lib/ipc/branch';
import { pushBranch } from '$lib/ipc/remote';
import { localTagTracker } from '$lib/stores/local-tags.svelte';
import { graphStore } from '$lib/stores/graph.svelte';
import { uiStore } from '$lib/stores/ui.svelte';

export type TagDeleteScope = 'local' | 'remote';

export async function executeTagDelete(
  tabId: string,
  name: string,
  scope: TagDeleteScope,
): Promise<void> {
  if (scope === 'local') {
    try {
      await deleteTag(tabId, name);
      await localTagTracker.markPushed(tabId, name).catch(() => {});
      uiStore.showToast(`Tag "${name}" eliminato in locale`, 'success');
      graphStore.refresh();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
    return;
  }

  try {
    await pushBranch(tabId, 'origin', `:refs/tags/${name}`);
  } catch (err) {
    uiStore.showToast(`Delete su origin fallito: ${err}`, 'error');
    return;
  }
  try {
    await deleteTag(tabId, name);
    await localTagTracker.markPushed(tabId, name).catch(() => {});
    uiStore.showToast(`Tag "${name}" eliminato in locale e su origin`, 'success');
    graphStore.refresh();
  } catch (err) {
    uiStore.showToast(`Origin pulito ma delete locale fallito: ${err}`, 'warning');
  }
}
