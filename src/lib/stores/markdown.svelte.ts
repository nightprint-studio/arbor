/**
 * Markdown editor store — drives `MarkdownEditorModal`.
 *
 * The modal is a singleton: opening a second file replaces whatever was
 * previously shown. State persisted here is metadata only (path, filename
 * for the header, optional tab id for future repo-aware actions). The
 * actual content + dirty tracking live inside the modal component so the
 * store stays trivial.
 */

interface OpenOptions {
  /** Absolute filesystem path of the file to edit. */
  path:     string;
  /** Display name shown in the modal header. Defaults to the trailing
   *  segment of `path` when omitted. */
  filename?: string;
  /** Repo id this file belongs to. Optional — kept for future actions
   *  (blame on selection, Save & commit, etc.) that need a tab context. */
  tabId?:   string | null;
}

function createMarkdownStore() {
  let open     = $state(false);
  let path     = $state<string | null>(null);
  let filename = $state<string | null>(null);
  let tabId    = $state<string | null>(null);

  return {
    get open()     { return open; },
    get path()     { return path; },
    get filename() { return filename; },
    get tabId()    { return tabId; },

    openFile(opts: OpenOptions) {
      path     = opts.path;
      filename = opts.filename
        ?? opts.path.replace(/\\/g, '/').split('/').filter(Boolean).pop()
        ?? opts.path;
      tabId    = opts.tabId ?? null;
      open     = true;
    },
    close() {
      open     = false;
      path     = null;
      filename = null;
      tabId    = null;
    },
  };
}

export const markdownStore = createMarkdownStore();
