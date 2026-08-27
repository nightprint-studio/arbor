<script lang="ts">
  /**
   * Every breakpoint of the project, in one place.
   *
   * The gutter is where you *set* a breakpoint, and it is the wrong place for two things it
   * cannot express: seeing the twelve you have scattered across nine files without opening
   * nine files, and setting one on a **throw** — which has no line to click at all.
   *
   * Disabling is offered beside deleting, and first, because the two are constantly confused
   * for one another: a breakpoint you disable is one you keep, and the reason to reach for
   * this list is usually "not this one, not right now" rather than "never again".
   *
   * ## Why the condition is edited here rather than in a popup of its own
   *
   * IntelliJ puts a breakpoint's condition behind a per-breakpoint dialog. Here it is a field on
   * the row, for two reasons. It is **reachable by Tab** from a window the keyboard already opens,
   * which a popup hanging off a gutter click is not. And a list where every condition is visible at
   * once answers the question you actually have when a program does not stop where you expected —
   * *which of these has a condition on it* — which a popup can only answer one breakpoint at a
   * time.
   */
  import { tick } from 'svelte';
  import { SvelteMap } from 'svelte/reactivity';
  import { Trash2, Plus, X, CircleDot } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import NumberStepper from '$lib/components/shared/ui/NumberStepper.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { checkBreakpointCondition } from '$lib/ipc/bennu/debug';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { ExceptionBreakpointDto } from '$lib/types/bennu/debug';

  let { onClose }: { onClose: () => void } = $props();

  const root = $derived(projectStore.project?.root ?? '');
  const breakpoints = $derived(root ? bennuDebugStore.breakpointsFor(root) : []);
  const exceptions = $derived(root ? bennuDebugStore.exceptionsFor(root) : []);

  /** Grouped by file, files in path order and lines ascending — the order you would read them
   *  in, rather than the order they happened to be clicked in. */
  const byFile = $derived.by(() => {
    const map = new Map<string, typeof breakpoints>();
    for (const b of breakpoints) {
      const list = map.get(b.file) ?? [];
      list.push(b);
      map.set(b.file, list);
    }
    return [...map.entries()]
      .map(([file, list]) => ({ file, list: [...list].sort((a, b) => a.line - b.line) }))
      .sort((a, b) => a.file.localeCompare(b.file));
  });

  let draft = $state('');

  /** An example rather than a description: the shape of the language is the thing to convey, and
   *  a field labelled "Condition" that shows one is faster to read than a sentence about paths. */
  const CONDITION_HINT = 'Condition — i > 5, order.customer.name == "acme"';

  /**
   * What is wrong with each condition, keyed `file:line` — from the backend's own parser, so what
   * this box accepts and what the debugger accepts cannot drift.
   *
   * Checked on a debounce as you type. A condition is the one debugger setting whose mistakes are
   * invisible at the time you make them: a bad watch shows an error beside the watch, a bad
   * condition just means the program never stops, later, somewhere you are not looking.
   */
  const conditionErrors = new SvelteMap<string, string>();
  const checkTimers = new Map<string, ReturnType<typeof setTimeout>>();

  /** One row's identity. Case-folded and forward-slashed, like the store's own key: the gutter
   *  hands over the editor's path and the list holds the canonical one, and on Windows those two
   *  spellings of the same file are routinely different. */
  function rowKey(file: string, line: number): string {
    return `${file.replace(/\\/g, '/').toLowerCase()}:${line}`;
  }

  function editCondition(file: string, line: number, condition: string) {
    bennuDebugStore.setBreakpointCondition(root, file, line, condition);
    const key = rowKey(file, line);
    clearTimeout(checkTimers.get(key));
    if (!condition.trim()) {
      conditionErrors.delete(key);
      return;
    }
    checkTimers.set(
      key,
      setTimeout(() => {
        void checkBreakpointCondition(file, condition)
          .then((why) => {
            if (why) conditionErrors.set(key, why);
            else conditionErrors.delete(key);
          })
          // A check that could not run says nothing rather than accusing the condition: the
          // backend being busy is not evidence about what was typed.
          .catch(() => conditionErrors.delete(key));
      }, 300),
    );
  }

  /** The one line to show under a row: what it cannot parse, then what the VM said about it. */
  function noteFor(file: string, line: number): { text: string; bad: boolean } | null {
    const typed = conditionErrors.get(rowKey(file, line));
    if (typed) return { text: typed, bad: true };
    const status = bennuDebugStore.statusOf(file, line);
    if (status?.condition_error) return { text: status.condition_error, bad: true };
    if (status?.message) return { text: status.message, bad: false };
    return null;
  }

  /**
   * Focus the row the list was opened on — the gutter's "Condition…" lands with that condition
   * box focused rather than at the top of a list of forty.
   */
  let listEl = $state<HTMLElement | null>(null);
  $effect(() => {
    const focus = bennuUiStore.breakpointsFocus;
    if (!focus || !listEl) return;
    const key = rowKey(focus.file, focus.line);
    void tick().then(() => {
      // The CONDITION field, not the first input in the row — that one is the enable toggle, and
      // landing on it would make "Add condition…" put the caret on the wrong control.
      const field = listEl?.querySelector<HTMLElement>(
        `[data-bp="${CSS.escape(key)}"] .bm-cond input`,
      );
      field?.focus();
      field?.scrollIntoView({ block: 'center' });
    });
  });

  /** `C:/p/src/main/java/com/acme/Order.java` → `Order.java`, with the rest as the tooltip. */
  function fileName(path: string): string {
    return path.split('/').pop() ?? path;
  }

  function open(file: string, line: number) {
    void projectStore.openFile(file).then(() => bennuUiStore.requestGoto(line));
    onClose();
  }

  function patchException(at: number, next: Partial<ExceptionBreakpointDto>) {
    bennuDebugStore.setExceptions(
      root,
      exceptions.map((e, i) => (i === at ? { ...e, ...next } : e)),
    );
  }

  function addException() {
    const name = draft.trim();
    // An empty class means "any throwable", which is a legitimate and useful entry — so the
    // field being blank is not a reason to refuse, only a reason not to add a second one.
    if (exceptions.some((e) => e.class === name)) return;
    draft = '';
    bennuDebugStore.setExceptions(root, [
      ...exceptions,
      { class: name, caught: false, uncaught: true, enabled: true },
    ]);
  }

  function removeException(at: number) {
    bennuDebugStore.setExceptions(root, exceptions.filter((_, i) => i !== at));
  }
</script>

<Modal {onClose} width="640px" height="560px" ariaLabel="Breakpoints">
  {#snippet header()}
    <ModalHeader {onClose}>
      <CircleDot size={14} />
      <span class="modal-title">Breakpoints</span>
      {#if projectStore.project}<span class="bm-project">{projectStore.project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  <div class="bm">
    <section class="bm-section">
      <header class="bm-head">
        <h3>Line breakpoints</h3>
        <span class="bm-count">{breakpoints.length}</span>
        <button
          class="bm-clear"
          type="button"
          disabled={!breakpoints.length}
          onclick={() => bennuDebugStore.clearBreakpoints(root)}
        >
          Remove all
        </button>
      </header>

      {#if !byFile.length}
        <EmptyState message="None yet. Click the left margin of a Java file to set one." />
      {:else}
        <div bind:this={listEl}>
          {#each byFile as group (group.file)}
            <div class="bm-file" use:tooltip={group.file}>{fileName(group.file)}</div>
            {#each group.list as bp (bp.line)}
              {@const hits = bennuDebugStore.statusOf(bp.file, bp.line)?.hits ?? 0}
              {@const note = noteFor(bp.file, bp.line)}
              <div class="bm-bp" class:off={!bp.enabled} data-bp={rowKey(bp.file, bp.line)}>
                <div class="bm-row">
                  <Toggle
                    checked={bp.enabled}
                    onchange={(v) => bennuDebugStore.setBreakpointEnabled(root, bp.file, bp.line, v)}
                  />
                  <button class="bm-where" type="button" onclick={() => open(bp.file, bp.line)}>
                    {fileName(bp.file)}<span class="bm-line">:{bp.line}</span>
                  </button>
                  <!-- What it has actually done this session. Only once it has done something:
                       a `0×` on every row would be forty rows of noise before a launch. -->
                  {#if hits > 0}
                    <span class="bm-hits" use:tooltip={'Times it has stopped the program this session'}>
                      {hits}×
                    </span>
                  {/if}
                  <span class="bm-spacer"></span>
                  <button
                    class="bm-x"
                    type="button"
                    use:tooltip={'Remove'}
                    aria-label="Remove this breakpoint"
                    onclick={() => bennuDebugStore.removeBreakpoint(root, bp.file, bp.line)}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
                <div class="bm-cond">
                  <Input
                    value={bp.condition}
                    size="sm"
                    placeholder={CONDITION_HINT}
                    ariaLabel="Condition for the breakpoint at {fileName(bp.file)}:{bp.line}"
                    error={note?.bad ? note.text : null}
                    oninput={(v) => editCondition(bp.file, bp.line, v)}
                  />
                  <span
                    class="bm-every"
                    use:tooltip={'Stop on every Nth hit — 1 is every one. Counted after the condition.'}
                  >
                    every
                    <NumberStepper
                      value={bp.hit_count || 1}
                      min={1}
                      size="sm"
                      ariaLabel="Stop on every Nth hit at {fileName(bp.file)}:{bp.line}"
                      onchange={(v) =>
                        bennuDebugStore.setBreakpointHitCount(root, bp.file, bp.line, v)}
                    />
                  </span>
                </div>
                <!-- The bind note, when there is nothing wrong with the condition — the error
                     takes the field's own red border instead, which is where you are looking. -->
                {#if note && !note.bad}<span class="bm-note">{note.text}</span>{/if}
              </div>
            {/each}
          {/each}
        </div>
      {/if}
    </section>

    <section class="bm-section">
      <header class="bm-head">
        <h3>Exception breakpoints</h3>
        <span class="bm-count">{exceptions.length}</span>
      </header>
      <p class="bm-hint">
        Stop where a throwable is <em>thrown</em>, not where it is caught — which is the only way
        to see the state that produced it. Leave the class empty for any exception.
      </p>

      <div class="bm-add">
        <Input
          bind:value={draft}
          placeholder="java.lang.IllegalStateException (empty = any)"
          size="sm"
          onkeydown={(e: KeyboardEvent) => { if (e.key === 'Enter') addException(); }}
        />
        <Button size="sm" variant="secondary" onclick={addException}>
          {#snippet iconStart()}<Plus size={13} />{/snippet}
          Add
        </Button>
      </div>

      {#each exceptions as exc, i (exc.class)}
        <div class="bm-row" class:off={!exc.enabled}>
          <Toggle checked={exc.enabled} onchange={(v) => patchException(i, { enabled: v })} />
          <span class="bm-exc">{exc.class || 'Any exception'}</span>
          <!-- Two questions, not one. An uncaught throw is a crash and is worth stopping on
               always; a caught one is ordinary control flow in any framework that uses
               exceptions for flow, and asking for those under Spring stops thousands of times
               before `main`. -->
          <label class="bm-check">
            <input
              type="checkbox"
              checked={exc.uncaught}
              onchange={(e) => patchException(i, { uncaught: e.currentTarget.checked })}
            />
            Uncaught
          </label>
          <label class="bm-check">
            <input
              type="checkbox"
              checked={exc.caught}
              onchange={(e) => patchException(i, { caught: e.currentTarget.checked })}
            />
            Caught
          </label>
          <button
            class="bm-x"
            type="button"
            use:tooltip={'Remove'}
            aria-label="Remove this exception breakpoint"
            onclick={() => removeException(i)}
          >
            <X size={12} />
          </button>
        </div>
      {/each}
    </section>
  </div>

  {#snippet footer()}
    <ModalFooter>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .bm { flex: 1; min-height: 0; overflow: auto; padding: 12px 16px; }
  .bm-project {
    padding: 0 6px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted); font-size: var(--font-size-2xs);
  }
  .bm-section + .bm-section { margin-top: 20px; }

  .bm-head { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  .bm-head h3 {
    margin: 0; font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary);
  }
  .bm-count {
    padding: 0 6px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-muted); font-size: var(--font-size-2xs);
  }
  .bm-clear {
    margin-left: auto; padding: 2px 6px;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-muted); font-size: var(--font-size-xs); cursor: pointer;
  }
  .bm-clear:hover:not(:disabled) { background: var(--bg-hover); color: var(--error); }
  .bm-clear:disabled { color: var(--text-disabled); cursor: default; }

  .bm-hint {
    margin: 0 0 8px; font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.6;
  }

  .bm-file {
    margin: 8px 0 2px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  .bm-row {
    display: flex; align-items: center; gap: 8px;
    padding: 3px 4px; border-radius: var(--radius-sm);
    font-size: var(--font-size-xs);
  }
  .bm-row:hover { background: var(--bg-hover); }
  /* Disabled reads as disabled, so the list never has to be read twice to find what is armed. */
  .bm-row.off .bm-where, .bm-row.off .bm-exc,
  .bm-bp.off .bm-where { color: var(--text-disabled); }

  /* One breakpoint: the location row, then its condition. Separated by a hairline rather than by
     space, so a list of forty stays scannable as forty things rather than eighty. */
  .bm-bp { padding-bottom: 4px; border-bottom: 1px solid var(--border-subtle); }
  .bm-bp:last-child { border-bottom: 0; }
  .bm-bp.off { opacity: 0.6; }
  .bm-spacer { flex: 1; }
  /* What it has actually done. Accent-tinted because it is the one number on the row that changed
     while you were not looking. */
  .bm-hits {
    font-size: var(--font-size-2xs);
    color: var(--accent);
    background: var(--accent-subtle);
    border-radius: var(--radius-sm);
    padding: 0 4px;
  }
  .bm-cond {
    display: flex; align-items: center; gap: 8px;
    padding: 0 4px 2px 30px; /* aligned under the location, clear of the toggle */
  }
  .bm-cond :global(.input-wrap) { flex: 1; min-width: 0; }
  .bm-cond :global(input) { font-family: var(--font-code); }
  .bm-every {
    display: inline-flex; align-items: center; gap: 4px;
    flex: 0 0 auto;
    font-size: var(--font-size-2xs); color: var(--text-muted);
  }

  .bm-where {
    padding: 0; border: 0; background: none; cursor: pointer;
    font-family: var(--font-code); color: var(--text-primary);
  }
  .bm-where:hover { color: var(--accent); text-decoration: underline; }
  .bm-line { color: var(--text-muted); }
  .bm-exc { font-family: var(--font-code); color: var(--text-primary); }
  .bm-note {
    display: block;
    padding-left: 30px;
    color: var(--text-muted); font-size: var(--font-size-2xs); font-style: italic;
  }

  .bm-check {
    display: flex; align-items: center; gap: 4px;
    color: var(--text-secondary); font-size: var(--font-size-2xs); cursor: pointer;
    white-space: nowrap;
  }
  .bm-x {
    margin-left: auto; flex: 0 0 auto;
    display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; padding: 0;
    border: 0; border-radius: var(--radius-sm); background: none;
    color: var(--text-muted); cursor: pointer;
  }
  .bm-x:hover { background: var(--bg-hover); color: var(--error); }
  .bm-check + .bm-x { margin-left: 0; }

  .bm-add { display: flex; align-items: center; gap: 6px; margin-bottom: 6px; }
  .bm-add :global(.input-wrap) { flex: 1; min-width: 0; }
</style>
