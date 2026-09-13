<script lang="ts">
  /**
   * One custom dictionary — the words the spell-checker is told to accept — as an editable list.
   *
   * Written once and used at both scopes: `global` in Settings (words that are correct in every
   * project of yours) and `project` in Project Configuration (a domain vocabulary that belongs to
   * one codebase and should not follow you to the next). Which one it is, is a prop.
   *
   * ## Why the list exists at all
   *
   * "Add to dictionary" has been the only door for a long time, and it is one-way: a word added by
   * mistake — a real typo, accepted with the wrong quick-fix — stays accepted forever, and the only
   * way back was to find a text file on disk. A list you can read is also the only way to answer
   * "why is *that* spelling not being flagged".
   *
   * Every change writes straight through: this is a list of words, not a form with an Apply, and
   * the backend reloads the engine so the next keystroke reflects it.
   */
  import { BookMarked, Plus, Trash2 } from 'lucide-svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import { dictWords, setDictWords } from '$lib/ipc/bennu/spell';
  import { bennuSpellStore } from '$lib/stores/bennu/spell.svelte';

  interface Props {
    /** Which dictionary: your own, or this project's. */
    scope: 'project' | 'global';
    /** The project root. Required for `project`, ignored for `global`. */
    root?: string | null;
    /** The card's title, since the two scopes mean different things. */
    title?: string;
    /** A line under the title saying what belongs in this one. */
    hint?: string;
  }

  const { scope, root = null, title = 'Accepted words', hint = '' }: Props = $props();

  let words = $state<string[]>([]);
  let draft = $state('');
  /** So the empty state does not flash "no words" before the first read answers. */
  let read = $state(false);

  async function load() {
    if (scope === 'project' && !root) return;
    try {
      words = await dictWords(scope, root ?? '');
    } catch {
      words = [];
    }
    read = true;
  }

  async function write(next: string[]) {
    if (scope === 'project' && !root) return;
    words = next;
    try {
      await setDictWords(scope, next, root ?? '');
      // The editor's spell effect watches this: a word removed has to start being flagged again
      // on the next debounce, not on the next time the file is opened.
      bennuSpellStore.bumpRevision();
    } catch {
      // Put back what is actually on disk rather than leaving the screen showing a write that
      // did not happen.
      void load();
    }
  }

  $effect(() => {
    void scope;
    void root;
    void load();
  });

  function add() {
    const word = draft.trim();
    if (!word) return;
    // Case-insensitively, the way the backend de-duplicates — so adding a word twice in two
    // spellings does not produce two rows that mean one thing.
    if (!words.some((w) => w.toLowerCase() === word.toLowerCase())) {
      void write([...words, word]);
    }
    draft = '';
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      add();
    }
  }
</script>

<div class="card">
  <div class="card-section-title"><BookMarked size={12} /> {title}</div>
  {#if hint}<p class="set-hint">{hint}</p>{/if}
  {#if scope === 'project' && !root}
    <p class="set-empty">Open a project to give it a vocabulary of its own.</p>
  {:else}
    <div class="set-list">
      {#each words as word (word)}
        <div class="set-list-row">
          <span class="word">{word}</span>
          <button
            class="set-list-del"
            type="button"
            aria-label={`Remove ${word}`}
            onclick={() => void write(words.filter((w) => w !== word))}
          >
            <Trash2 size={13} />
          </button>
        </div>
      {/each}
      {#if read && words.length === 0}
        <p class="set-empty">No words yet — “Add to dictionary” from a spelling hint puts them here.</p>
      {/if}
      <div class="set-list-add">
        <Input
          value={draft}
          placeholder="a word to accept"
          ariaLabel="Word to accept"
          oninput={(v) => (draft = v)}
          onkeydown={onKeydown}
        />
        <Button variant="secondary" size="sm" onclick={add} disabled={!draft.trim()}>
          {#snippet iconStart()}<Plus size={13} />{/snippet}
          Add
        </Button>
      </div>
    </div>
  {/if}
</div>

<style>
  /* Left-to-right, unlike the path rows the shared list class is written for: a word is read from
     its start, and the `direction: rtl` that keeps the tail of a long path visible would put the
     punctuation of a word in the wrong place. */
  .word {
    flex: 1; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-xs); color: var(--text-primary);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
</style>
