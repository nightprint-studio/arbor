/**
 * Open a connection's session, and say so when it fails.
 *
 * One function because opening a connection is reachable from three places — the
 * plug on the sidebar row, its context menu, and the toolbar of a query tab bound
 * to a closed connection — and all three mean exactly the same thing, including
 * how the failure is reported. A copy per call site is a copy that eventually
 * swallows the error in one of them.
 *
 * The toast lives here rather than in `connectionsStore` because a store has no
 * business deciding how a product tells the user something; the store answers with
 * the message, and this is the one place that turns it into a toast.
 *
 * ## Opening one also selects it
 *
 * Not a convenience — it is what makes the connection **usable**. The catalogue is
 * read for the *active* connection and for no other, so a connection that is open
 * but not selected has no tables, no views and therefore no completion, no
 * abbreviation expansion and no live validation. Nothing on screen explains that;
 * the editor simply stops knowing anything.
 *
 * Before this, the only thing that ever selected a connection was clicking its row
 * in the sidebar — which happens to be how you expand it. So the catalogue arrived
 * as a **side effect of expanding the tree**, and every path that opened a session
 * without touching that row (the row's own plug button, the context menu, a query
 * tab's toolbar) left the editor blind.
 *
 * Only on success. A connection that refused is not what you are working with, and
 * moving the sidebar off a working connection onto a broken one would be taking
 * something away as well as failing.
 */
import { connectionsStore } from '$lib/stores/picus/connections.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

/** `true` when the session is open afterwards. */
export async function openConnection(id: string): Promise<boolean> {
  const message = await connectionsStore.connect(id);
  if (message) {
    toastStore.show(message, 'error');
    return false;
  }
  connectionsStore.setActive(id);
  return true;
}
