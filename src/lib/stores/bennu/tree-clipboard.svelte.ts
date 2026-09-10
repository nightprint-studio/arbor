/**
 * What the project tree has on its clipboard.
 *
 * ## Why not the system clipboard
 *
 * Because what is copied here is a **file**, and the system clipboard carries text. Writing a path
 * into it would make Ctrl+V in the editor paste a path — which is not what anybody pressing Ctrl+C
 * on a class means, and is the sort of surprise that makes people stop trusting a key. So the tree
 * keeps its own, and the system clipboard stays what it was; `Copy path` is still there for the
 * times you actually want the text.
 *
 * A module-level store rather than state in the panel: the sidebar is destroyed and rebuilt when
 * the left panel changes, and a clipboard that empties because you glanced at the Structure view is
 * a clipboard nobody relies on.
 */
function createTreeClipboardStore() {
  let paths = $state<string[]>([]);

  return {
    /** The copied files, absolute. Empty when there is nothing to paste. */
    get paths() {
      return paths;
    },
    get isEmpty() {
      return paths.length === 0;
    },
    copy(next: readonly string[]) {
      paths = [...next];
    },
    clear() {
      paths = [];
    },
  };
}

export const treeClipboardStore = createTreeClipboardStore();
