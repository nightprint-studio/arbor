/**
 * The DTO Lab's state — the class being tried out, the payload typed for it, what the JVM answered,
 * and the tests about to be written.
 *
 * In a store rather than in the panel because the lab is opened from outside it — the editor's
 * context menu, the palette, `Alt+Shift+J` — and because closing the panel to read the code must not
 * throw away a payload somebody spent a minute writing.
 */

import {
  dtoLabClass,
  dtoLabCreateFile,
  dtoLabDefaultJson,
  dtoLabGenerate,
  dtoLabNewTemplate,
  dtoLabRead,
  dtoLabSetProjectTemplate,
  dtoLabTemplates,
  dtoLabValidate,
  type DtoLabClassView,
  type DtoLabPreview,
  type DtoLabReadResult,
  type DtoLabTarget,
  type DtoLabTemplates,
  type DtoLabValidateResult,
} from '$lib/ipc/bennu/dtolab';
import { writeFile } from '$lib/ipc/bennu';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import { projectStore } from './project.svelte';
import { bennuUiStore } from './ui.svelte';

export type DtoLabTab = 'payload' | 'tests';

/** What the lab needs from the editor — bound by the window that owns one. */
export interface DtoLabEditorApi {
  caretContext: () => { source: string; offset: number } | null;
  applyEdits: (edits: readonly { start: number; end: number; replacement: string }[]) => void;
}

interface Origin {
  root: string;
  file: string;
  source: string;
  offset: number;
}

function createBennuDtoLabStore() {
  let editorApi: DtoLabEditorApi | null = null;

  let origin = $state<Origin | null>(null);
  let view = $state<DtoLabClassView | null>(null);
  let tab = $state<DtoLabTab>('payload');
  let opening = $state(false);
  let openError = $state<string | null>(null);

  let payload = $state('');
  let locale = $state('');
  let running = $state(false);
  let skeletonLoading = $state(false);
  let readResult = $state<DtoLabReadResult | null>(null);
  let readError = $state<string | null>(null);
  let validateResult = $state<DtoLabValidateResult | null>(null);
  let validateError = $state<string | null>(null);

  let templates = $state<DtoLabTemplates | null>(null);
  /** This generation's template; `null` means the project's. */
  let template = $state<string | null>(null);
  /** The fields to generate for; `null` means every constrained one. */
  let fields = $state<string[] | null>(null);
  /** An existing test class to add to; `null` means the class's own test file. */
  let target = $state<DtoLabTarget | null>(null);
  let generating = $state(false);
  let generateError = $state<string | null>(null);
  let preview = $state<DtoLabPreview | null>(null);
  let applying = $state(false);

  async function open(next: Origin, openTab?: DtoLabTab) {
    origin = next;
    if (openTab) tab = openTab;
    opening = true;
    openError = null;
    readResult = readError = validateResult = validateError = null;
    preview = generateError = null;
    template = null;
    fields = null;
    target = null;
    try {
      const found = await dtoLabClass(next.root, next.file, next.source, next.offset);
      if (!found) {
        view = null;
        openError = 'There is no class at the caret.';
        return;
      }
      view = found;
      payload = JSON.stringify(found.skeleton, null, 2);
      void loadTemplates(next.root);
    } catch (e) {
      view = null;
      openError = String(e);
    } finally {
      opening = false;
    }
  }

  async function loadTemplates(root: string) {
    try {
      templates = await dtoLabTemplates(root);
    } catch {
      templates = null;
    }
  }

  /** Wait until the editor shows `file`, and return its buffer — or the last buffer seen. */
  async function bufferOf(file: string, expected: string | null): Promise<string | null> {
    let seen: string | null = null;
    for (let i = 0; i < 40; i++) {
      if (projectStore.activeFilePath === file) {
        seen = editorApi?.caretContext()?.source ?? null;
        if (seen !== null && (expected === null || seen === expected)) return seen;
      }
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    return seen;
  }

  return {
    get origin() { return origin; },
    get view() { return view; },
    get tab() { return tab; },
    get opening() { return opening; },
    get openError() { return openError; },
    get payload() { return payload; },
    get locale() { return locale; },
    set locale(next: string) { locale = next; },
    get running() { return running; },
    get skeletonLoading() { return skeletonLoading; },
    get readResult() { return readResult; },
    get readError() { return readError; },
    get validateResult() { return validateResult; },
    get validateError() { return validateError; },
    get templates() { return templates; },
    get template() { return template; },
    get fields() { return fields; },
    get target() { return target; },
    get generating() { return generating; },
    get generateError() { return generateError; },
    get preview() { return preview; },
    get applying() { return applying; },

    bindEditor(api: DtoLabEditorApi) { editorApi = api; },
    setTab(next: DtoLabTab) { tab = next; },
    setPayload(next: string) { payload = next; },
    setLocale(next: string) { locale = next; },
    setTemplate(next: string | null) { template = next; },
    setFields(next: string[] | null) { fields = next; },
    setTarget(next: DtoLabTarget | null) { target = next; },
    closePreview() { preview = null; },

    /** Open the lab on the class at the editor's caret. */
    openAtCaret(openTab: DtoLabTab = 'payload') {
      const root = projectStore.project?.root;
      const file = projectStore.activeFilePath;
      const ctx = editorApi?.caretContext() ?? null;
      if (!root || !file || !file.toLowerCase().endsWith('.java') || !ctx) {
        toastStore.show('Put the caret in a Java class first', 'info');
        return;
      }
      bennuUiStore.showBottom('dtolab');
      void open({ root, file, source: ctx.source, offset: ctx.offset }, openTab);
    },

    /** Read the class again — from the editor when its file is the one showing. */
    reload() {
      const current = origin;
      if (!current) return;
      const ctx = projectStore.activeFilePath === current.file ? editorApi?.caretContext() : null;
      void open(ctx ? { ...current, source: ctx.source } : current);
    },

    /** Replace the payload with what the project's own `ObjectMapper` writes for a new instance. */
    async useProjectJson() {
      const current = origin;
      const cls = view?.class.binary;
      if (!current || !cls || skeletonLoading) return;
      skeletonLoading = true;
      readError = null;
      try {
        const reply = await dtoLabDefaultJson(current.root, cls);
        if (reply.json) payload = reply.json;
        else readError = 'The project has no Jackson, so there is no JSON it would write for this class.';
      } catch (e) {
        readError = String(e);
      } finally {
        skeletonLoading = false;
      }
    },

    /** Bind the payload, write it back, and validate it — each answered separately, so a project with
     *  Jackson and no Bean Validation still gets its round trip. */
    async run() {
      const current = origin;
      const lab = view;
      if (!current || !lab || running) return;
      running = true;
      readError = validateError = null;
      const json = payload;
      try {
        try {
          readResult = await dtoLabRead(current.root, lab.class.binary, json);
        } catch (e) {
          readResult = null;
          readError = String(e);
        }
        try {
          validateResult = await dtoLabValidate(current.root, lab.class.binary, json, locale.trim() || null, lab.validation || null);
        } catch (e) {
          validateResult = null;
          validateError = String(e);
        }
      } finally {
        running = false;
      }
    },

    async generate() {
      const current = origin;
      if (!current || !view || generating) return;
      generating = true;
      generateError = null;
      try {
        preview = await dtoLabGenerate({
          root: current.root,
          file: current.file,
          source: current.source,
          offset: current.offset,
          template,
          target,
          fields,
        });
      } catch (e) {
        generateError = String(e);
      } finally {
        generating = false;
      }
    },

    /**
     * Write the previewed tests.
     *
     * A new file is created. An existing one that is not open is rewritten on disk. One that IS open
     * gets the insertion in its buffer, as one undo step — and only if the buffer is still the text the
     * preview was computed against; otherwise the unsaved edits would be quietly shifted around.
     */
    async apply() {
      const current = origin;
      const result = preview;
      if (!current || !result || applying) return;
      applying = true;
      const count = `${result.cases} test case${result.cases === 1 ? '' : 's'}`;
      try {
        if (!result.exists) {
          await dtoLabCreateFile(current.root, result.file, result.text);
          await projectStore.openFile(result.file);
          preview = null;
          toastStore.show(`Wrote ${count}`, 'success');
          return;
        }
        if (!projectStore.openFilePaths.includes(result.file)) {
          await writeFile(current.root, result.file, result.text);
          await projectStore.openFile(result.file);
          preview = null;
          toastStore.show(`Added ${count}`, 'success');
          return;
        }
        await projectStore.openFile(result.file);
        const buffer = await bufferOf(result.file, result.base);
        if (buffer === null || result.base === null || result.offset === null || buffer !== result.base) {
          toastStore.show('The test file has changes the preview did not see — save it and generate again', 'info');
          return;
        }
        editorApi?.applyEdits([{ start: result.offset, end: result.offset, replacement: result.inserted }]);
        preview = null;
        toastStore.show(`Added ${count} — save the file to keep them`, 'success');
      } catch (e) {
        toastStore.show(`Couldn't write the tests: ${e}`, 'error');
      } finally {
        applying = false;
      }
    },

    async setProjectTemplate(name: string) {
      const current = origin;
      if (!current) return;
      try {
        await dtoLabSetProjectTemplate(current.root, name);
        await loadTemplates(current.root);
      } catch (e) {
        toastStore.show(`Couldn't save the project's template: ${e}`, 'error');
      }
    },

    /** Create a template from `from` and open it for editing. */
    async newTemplate(name: string, from: string | null): Promise<boolean> {
      try {
        const path = await dtoLabNewTemplate(name, from);
        if (origin) await loadTemplates(origin.root);
        template = name;
        await projectStore.openFile(path);
        return true;
      } catch (e) {
        toastStore.show(`${e}`, 'error');
        return false;
      }
    },

    async editTemplate(name: string) {
      const info = templates?.templates.find((t) => t.name === name);
      if (info?.path) await projectStore.openFile(info.path);
    },

    async refreshTemplates() {
      if (origin) await loadTemplates(origin.root);
    },
  };
}

export const bennuDtoLabStore = createBennuDtoLabStore();
