<script lang="ts">
  /**
   * Run — the console of the program you launched.
   *
   * Its own tool window, not a section of Build, because they answer different questions and
   * have different lifetimes: a build log is finished the moment you read it, a program's
   * output is a live thing you watch, type into and stop. They shared one buffer until now,
   * which meant launching an app appended the JVM's output under Maven's and the next build
   * threw both away.
   *
   * What makes it a console rather than a log view:
   *   • the **command that actually ran** is its first line — the resolved `java`, the VM
   *     args, the class — so it can be pasted into a terminal when the run misbehaves;
   *   • **you can type back**. stdin is a pipe, so a program that asks a question can be
   *     answered here instead of appearing to hang — from a strip at the bottom that is
   *     collapsed until you want it, since most programs never read a line;
   *   • **Stop really stops** — the backend kills the process tree, not the handle;
   *   • it ends with an **exit code and a duration**, which is the whole answer to "how did
   *     that go" and is otherwise something you reconstruct from the last line.
   *
   * Presentation only: the buffer, the lifecycle and the process live in
   * {@link bennuRunStore}.
   */
  import {
    Play, Square, Trash2, RotateCw, SlidersHorizontal, CornerDownLeft, ChevronDown,
  } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore, formatMs } from '$lib/stores/bennu/run.svelte';
  import { ansiSpans } from './ansi';

  const lines = $derived(bennuRunStore.runLines);
  /** Whether ANY run is live — what the Stop button and the input strip key off. */
  const running = $derived(bennuRunStore.running);
  /** Whether the run you are LOOKING at is the live one. An old tab must read as finished
   *  even while a newer run is going. */
  const isLive = $derived(bennuRunStore.activeIsLive);
  const stopping = $derived(bennuRunStore.stopping);
  const exitCode = $derived(bennuRunStore.runExitCode);
  const duration = $derived(bennuRunStore.runDurationMs);
  const label = $derived(bennuRunStore.runLabel);
  const command = $derived(bennuRunStore.runCommand);

  /** The tab strip: newest last. The live one carries a ▷ so a run still going is visible
   *  from whichever tab you happen to be reading. */
  const tabItems = $derived<TabItem[]>(
    bennuRunStore.tabs.map((t) => ({
      id: t.id,
      label: t.label,
      title: t.command || t.mainClass,
      closable: true,
      icon: t.live ? Play : undefined,
      iconSize: 11,
    })),
  );

  /** A finished run's verdict. `null` while nothing has finished. Keyed off "did it finish"
   *  and not off the exit code, because a killed process has no code of its own. */
  const verdict = $derived.by(() => {
    const tab = bennuRunStore.activeTab;
    if (isLive || !tab?.finished) return null;
    // A tab that never got a start time never became a process — the compile failed, or the
    // spawn did. "Stopped" would be a lie about something that never ran.
    if (!tab.startedAt) return { ok: false, text: 'Did not start' };
    const label =
      exitCode === null ? 'Stopped' : exitCode === 0 ? 'Finished' : `Exited with code ${exitCode}`;
    return {
      ok: exitCode === 0,
      text: label + (duration === null ? '' : ` · ${formatMs(duration)}`),
    };
  });

  // ── the log, and staying at the bottom without fighting the reader ────────────
  let logEl = $state<HTMLDivElement | null>(null);
  /** Follow new output only while the reader is AT the bottom. Scrolling up to read
   *  something is a statement that you want to stay there; a console that yanks you back
   *  down on the next line is unusable on a chatty program. */
  let stick = $state(true);

  function onScroll() {
    const el = logEl;
    if (!el) return;
    stick = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
  }

  $effect(() => {
    void lines.length;
    const el = logEl;
    if (el && stick) queueMicrotask(() => { el.scrollTop = el.scrollHeight; });
  });

  // ── stdin ────────────────────────────────────────────────────────────────────
  /**
   * Collapsed by default, because most programs never read a line: an input box open on
   * every run is permanent chrome for a rare case, and the strip it collapses to is still
   * the thing that tells you the option exists at all.
   *
   * It cannot open itself when the program needs it — from outside, a JVM blocked on
   * `System.in.read()` looks exactly like one that is working — and a bar that appeared and
   * vanished on a guess while you were reading the log would be worse than one that sits
   * still.
   */
  let inputOpen = $state(false);
  let input = $state('');
  let inputEl = $state<HTMLInputElement | null>(null);

  // A new run starts collapsed; within one run it stays open once you have opened it, since
  // a program that asks a question usually asks another.
  let wasLive = false;
  $effect(() => {
    const now = bennuRunStore.activeIsLive;
    if (now && !wasLive) inputOpen = false;
    wasLive = now;
  });

  // Focus on expand — reading `inputEl` here is what makes this re-run once the field is in
  // the DOM, which `queueMicrotask` would be racing.
  $effect(() => {
    if (inputOpen) inputEl?.focus();
  });

  function send() {
    // An empty line is a legitimate answer to a prompt (it means "the default"), so it is
    // sent rather than swallowed.
    void bennuRunStore.sendInput(input);
    input = '';
  }

  function onInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      send();
    } else if (e.key === 'Escape') {
      // Back to the strip. Esc is what closes things everywhere else in the app, and this is
      // a thing that opened.
      e.preventDefault();
      inputOpen = false;
    }
  }
</script>

<div class="rp">
  <BottomPanelHeader title="Run" onClose={() => bennuUiStore.closeBottom()}>
    <!-- Green, the way every IDE marks the thing that starts a program — and the only icon
         in this rail that names an action rather than a place. -->
    {#snippet icon()}<span class="rp-run-icon"><Play size={13} /></span>{/snippet}
    {#snippet actions()}
      {#if running}
        <button
          class="ps-btn"
          type="button"
          use:tooltip={'Stop the program'}
          aria-label="Stop the program"
          disabled={stopping}
          onclick={() => void bennuRunStore.stop()}
        >
          <Square size={12} />
        </button>
      {/if}
      <!-- Repeats THIS tab's run, into a new tab — so ⟳ on an old transcript reruns what
           that transcript was, which is the reason you were looking at it. -->
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Rerun this'}
        aria-label="Rerun this"
        disabled={!bennuRunStore.canRerun || bennuRunStore.building}
        onclick={() => void bennuRunStore.rerunApp()}
      >
        <RotateCw size={13} />
      </button>
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Edit run configurations'}
        aria-label="Edit run configurations"
        onclick={() => bennuUiStore.openRunConfig()}
      >
        <SlidersHorizontal size={13} />
      </button>
      <!-- Closes the finished runs. The live one stays: tidying the console is not a way to
           kill a program, and Stop is right there. -->
      <button
        class="ps-btn"
        type="button"
        use:tooltip={'Close the finished runs'}
        aria-label="Close the finished runs"
        disabled={!bennuRunStore.tabs.some((t) => !t.live)}
        onclick={() => bennuRunStore.clearRun()}
      >
        <Trash2 size={13} />
      </button>
    {/snippet}
  </BottomPanelHeader>

  {#if !bennuRunStore.tabs.length}
    <div class="rp-empty">
      <Play size={20} />
      <EmptyState message="Nothing running. Press ▷ (Shift+F10) to run the project." />
    </div>
  {:else}
    {#if bennuRunStore.tabs.length > 1}
      <div class="rp-tabs">
        <Tabs
          items={tabItems}
          value={bennuRunStore.activeTabId ?? ''}
          variant="panel"
          size="sm"
          closable
          ariaLabel="Runs"
          onSelect={(id) => bennuRunStore.showTab(id)}
          onClose={(id) => void bennuRunStore.closeTab(id)}
        />
      </div>
    {/if}

    <div class="rp-status">
      {#if isLive}
        <Spinner size={13} />
        <span class="rp-text">{stopping ? 'Stopping…' : 'Running'}</span>
      {:else if bennuRunStore.building && bennuRunStore.activeTab && !bennuRunStore.runFinished}
        <Spinner size={13} />
        <span class="rp-text">Building…</span>
      {:else if verdict}
        <span class="rp-dot" class:ok={verdict.ok} class:bad={!verdict.ok}></span>
        <span class="rp-text">{verdict.text}</span>
      {:else}
        <!-- Reached when the compile failed: the program never started, and the last line
             of the console says why. A spinner here would claim a build is still going. -->
        <span class="rp-dot bad"></span>
        <span class="rp-text">Did not start</span>
      {/if}
      {#if label}<span class="rp-label">{label}</span>{/if}
      {#if bennuRunStore.runWorkingDir}
        <span class="rp-cwd" title={bennuRunStore.runWorkingDir}>{bennuRunStore.runWorkingDir}</span>
      {/if}
    </div>

    {#if command}
      <!-- Selectable on purpose: this is the line you copy into a terminal. -->
      <p class="rp-cmd">{command}</p>
    {/if}

    <div class="rp-log" bind:this={logEl} onscroll={onScroll}>
      {#each lines as l, i (i)}
        <div class="rp-line stream-{l.stream}">
          {#if l.stream === 'in'}<span class="rp-caret">&gt;</span>{/if}
          {#each ansiSpans(l.text) as s, j (j)}
            <span class={s.cls}>{s.text}</span>
          {/each}
        </div>
      {/each}
    </div>

    <!-- Only on the tab that IS the live run: there is nothing to answer on a transcript. -->
    {#if isLive}
      {#if inputOpen}
        <div class="rp-input">
          <span class="rp-caret">&gt;</span>
          <input
            bind:this={inputEl}
            bind:value={input}
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder="Type here to answer the program…"
            aria-label="Program input"
            onkeydown={onInputKeydown}
          />
          <button
            type="button"
            class="rp-send"
            use:tooltip={{ content: 'Send', shortcut: 'Enter' }}
            aria-label="Send the line to the program"
            onclick={send}
          >
            <CornerDownLeft size={12} />
          </button>
          <button
            type="button"
            class="rp-send"
            use:tooltip={{ content: 'Hide', shortcut: 'Esc' }}
            aria-label="Hide the input line"
            onclick={() => (inputOpen = false)}
          >
            <ChevronDown size={12} />
          </button>
        </div>
      {:else}
        <!-- The collapsed state: one line saying the option is there, for the programs that
             ask a question. Most never do. -->
        <button
          type="button"
          class="rp-input-strip"
          onclick={() => (inputOpen = true)}
          use:tooltip={'Write to the program’s standard input'}
        >
          <span class="rp-caret">&gt;</span>
          <span>Send input</span>
        </button>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .rp { display: flex; flex-direction: column; height: 100%; width: 100%; min-height: 0; background: var(--bg-base); overflow: hidden; }
  .rp-run-icon { display: inline-flex; color: var(--success); }

  /* One tab per run, shown only once there are two — a strip captioned with the single
     thing already named in the status line below it is a row of chrome saying nothing. */
  .rp-tabs {
    display: flex; align-items: stretch;
    height: 28px; min-height: 28px; flex-shrink: 0;
    border-bottom: 1px solid var(--border-subtle);
  }
  .rp-tabs :global(.tabs) { flex: 1; min-width: 0; }
  .rp-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 6px; color: var(--text-disabled);
  }

  .rp-status {
    display: flex; align-items: center; gap: 7px; flex-shrink: 0;
    padding: 6px 12px; font-size: var(--font-size-xs); color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .rp-text { font-weight: 500; }
  .rp-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .rp-dot.ok { background: var(--success); }
  .rp-dot.bad { background: var(--error); }
  .rp-label {
    padding: 0 6px; border-radius: var(--radius-sm);
    background: var(--bg-overlay); color: var(--text-secondary); font-size: var(--font-size-2xs);
  }
  .rp-cwd {
    margin-left: auto; min-width: 0;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl;
  }

  .rp-cmd {
    flex-shrink: 0; margin: 0; padding: 5px 12px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    white-space: pre-wrap; word-break: break-all;
    user-select: text;
  }

  .rp-log {
    flex: 1; min-height: 0; overflow-y: auto;
    padding: 6px 12px;
    font-family: var(--font-code); font-size: var(--font-size-xs); line-height: 1.5;
    user-select: text;
  }
  .rp-line { white-space: pre-wrap; word-break: break-word; color: var(--text-secondary); }
  .rp-line.stream-err { color: var(--error); }
  .rp-line.stream-meta { color: var(--text-muted); font-style: italic; }
  /* What you typed, marked the way a shell marks it — so a transcript reads as a
     conversation rather than as the program talking to itself. */
  .rp-line.stream-in { color: var(--text-primary); }
  .rp-caret { color: var(--accent); margin-right: 6px; user-select: none; }

  /* ANSI SGR — the theme's own hues, so a coloured log sits in the app rather than
     next to it. */
  .rp-log :global(.a-bold) { font-weight: 700; }
  .rp-log :global(.a-black) { color: var(--text-disabled); }
  .rp-log :global(.a-red) { color: var(--error); }
  .rp-log :global(.a-green) { color: var(--success); }
  .rp-log :global(.a-yellow) { color: var(--warning); }
  .rp-log :global(.a-blue) { color: var(--info); }
  .rp-log :global(.a-magenta) { color: var(--accent); }
  .rp-log :global(.a-cyan) { color: var(--info); }
  .rp-log :global(.a-white) { color: var(--text-primary); }

  .rp-input {
    display: flex; align-items: center; gap: 0; flex-shrink: 0;
    padding: 5px 12px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
  }
  .rp-input input {
    flex: 1; min-width: 0;
    background: none; border: none; outline: none;
    color: var(--text-primary);
    font-family: var(--font-code); font-size: var(--font-size-xs);
  }
  .rp-input input::placeholder { color: var(--text-disabled); font-family: var(--font-ui-sans); }
  .rp-send {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; padding: 0;
    background: none; border: none; border-radius: var(--radius-sm);
    color: var(--text-muted); cursor: pointer;
  }
  .rp-send:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* Collapsed: a third of the height of the field it opens, and quiet enough to be ignored
     by the runs that never need it. */
  .rp-input-strip {
    display: flex; align-items: center; gap: 6px; flex-shrink: 0;
    width: 100%; padding: 1px 12px;
    background: var(--bg-elevated);
    border: none; border-top: 1px solid var(--border-subtle);
    color: var(--text-disabled);
    font: var(--font-size-2xs) var(--font-ui-sans);
    text-align: left; cursor: pointer;
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .rp-input-strip:hover { background: var(--bg-hover); color: var(--text-secondary); }
  .rp-input-strip .rp-caret { margin-right: 0; }
</style>
