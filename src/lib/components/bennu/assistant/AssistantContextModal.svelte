<script lang="ts">
  /**
   * Imber's context — standing notes sent with every conversation (the "custom instructions" of a chat assistant),
   * at two levels: **general** (about you, in the profile's config) and **this project** (about the repository,
   * in `<repo>/.arbor/bennu/config.toml`). One modal, opened from Settings › Imber, the panel's ⋯ menu and the
   * command palette; mounted once, in the window.
   *
   * Each level is a list of short notes rather than one block of text — one rule per item is easier to read, reorder and
   * drop — stored as a Markdown bullet list (`assistant-context.ts`). Edited as drafts and saved on purpose
   * (Ctrl/Cmd+Enter), because the text is paid for on every turn and a half-typed note should not be sent.
   */
  import { untrack } from 'svelte';
  import { BookUser } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import TextListEditor from '$lib/components/shared/ui/TextListEditor.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { bennuAssistantStore } from '$lib/stores/bennu/assistant.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { getProjectContext, setProjectContext } from '$lib/ipc/bennu/assistant';
  import { formatTokens } from './assistant-format';
  import { MAX_GENERAL_CONTEXT } from './assistant-decision';
  import { parseContext, serializeContext } from './assistant-context';

  const store = bennuAssistantStore;

  type Scope = 'general' | 'project';
  let scope = $state<Scope>('general');
  let general = $state<string[]>([]);
  let project = $state<string[]>([]);
  /** What the project's file held when the modal opened — to know whether the draft changed. */
  let savedProject = $state('');
  /** Bumped when the drafts are reloaded, which remounts the lists with their new items. */
  let load = $state(0);

  const GENERAL_IDEAS = ['Answer in Italian', 'Keep code and comments in English', 'Be terse'];
  const PROJECT_IDEAS = ['Never edit anything under /vendor', 'Domain terms stay untranslated'];

  const root = $derived(projectStore.project?.root ?? '');
  const projectName = $derived(projectStore.project?.name ?? '');
  const tabs = $derived([
    { id: 'general', label: 'General' },
    { id: 'project', label: projectName ? `This project · ${projectName}` : 'This project', disabled: !root },
  ]);

  // A fresh draft each time the modal opens, from what is saved.
  $effect(() => {
    if (!store.contextOpen) return;
    general = parseContext(store.generalContext);
    scope = 'general';
    project = [];
    savedProject = '';
    untrack(() => load++);
    const at = root;
    if (at) {
      void getProjectContext(at).then((text) => {
        if (store.contextOpen && root === at) {
          savedProject = serializeContext(parseContext(text));
          project = parseContext(text);
          load++; // in a callback, not tracked
        }
      }).catch(() => undefined);
    }
  });

  const generalText = $derived(serializeContext(general));
  const projectText = $derived(serializeContext(project));
  const draft = $derived(scope === 'general' ? generalText : projectText);
  /** Rough rule of thumb: four characters a token. */
  const approxTokens = $derived(Math.ceil(draft.length / 4));
  const tooLong = $derived(generalText.length > MAX_GENERAL_CONTEXT || projectText.length > MAX_GENERAL_CONTEXT);
  const dirty = $derived(generalText !== store.generalContext || projectText !== savedProject);

  function choose(id: string) {
    scope = id as Scope;
  }

  async function save() {
    if (tooLong) return;
    if (generalText !== store.generalContext) store.setGeneralContext(generalText);
    if (projectText !== savedProject && root) {
      try {
        await setProjectContext(root, projectText);
      } catch (e) {
        toastStore.show(`Could not save the project's context: ${e}`, 'error');
        return;
      }
    }
    store.closeContext();
  }
</script>

{#if store.contextOpen}
  <Modal onClose={() => store.closeContext()} width="640px" height="580px" ariaLabel="Imber context">
    {#snippet header()}
      <ModalHeader onClose={() => store.closeContext()}>
        <BookUser size={14} />
        <span class="title">Imber context</span>
      </ModalHeader>
    {/snippet}

    <div class="body">
      <Tabs items={tabs} value={scope} onSelect={choose} variant="pill" size="sm" />
      {#if scope === 'general'}
        <p class="hint">
          What Imber should always know about <strong>you</strong> — your role, the stack you live in, the tone and
          language you want, rules to follow everywhere. One note per item. Kept in your profile and sent with every
          conversation, in every project.
        </p>
        {#key load}
          <TextListEditor
            bind:items={general}
            suggestions={GENERAL_IDEAS}
            addLabel="Add a note"
            placeholder="e.g. I'm a senior Java developer"
            ariaLabel="General context for Imber"
            onsubmit={save}
          />
        {/key}
      {:else}
        <p class="hint">
          What Imber should know about <strong>this project</strong> — its conventions, what not to touch, the vocabulary
          of its domain. One note per item. Kept in <code>.arbor/bennu/config.toml</code> inside the project and sent only
          with conversations about it. What the whole team should read belongs in the project's CLAUDE.md instead.
        </p>
        {#key load}
          <TextListEditor
            bind:items={project}
            suggestions={PROJECT_IDEAS}
            addLabel="Add a note"
            placeholder="e.g. Legacy Struts 1 on Java 8"
            ariaLabel="Context for Imber about this project"
            onsubmit={save}
          />
        {/key}
      {/if}
      <p class="meta" class:over={tooLong}>
        {#if tooLong}Too long: a level holds at most {MAX_GENERAL_CONTEXT} characters.{:else}About {formatTokens(approxTokens)} tokens added to every request.{/if}
        <span class="keys"><kbd>Enter</kbd> next note · <kbd>Alt</kbd>+<kbd>↑</kbd><kbd>↓</kbd> move · <kbd>Ctrl</kbd>+<kbd>Enter</kbd> save</span>
      </p>
    </div>

    {#snippet footer()}
      <ModalFooter align="end">
        <Button size="sm" onclick={() => store.closeContext()}>Cancel</Button>
        <Button variant="primary" size="sm" disabled={!dirty || tooLong} onclick={save}>Save</Button>
      </ModalFooter>
    {/snippet}
  </Modal>
{/if}

<style>
  .title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .body { display: flex; flex-direction: column; gap: 10px; padding: 14px 16px; }
  .hint { margin: 0; font-size: var(--font-size-xs); color: var(--text-secondary); line-height: 1.5; }
  .meta { margin: 0; display: flex; justify-content: space-between; gap: 12px; font-size: var(--font-size-2xs); color: var(--text-disabled); }
  .meta.over { color: var(--warning); }
  .keys { color: var(--text-muted); }
</style>
