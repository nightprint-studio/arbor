<script lang="ts">
  /**
   * Run — everything you started and are now watching: a program, a debug session, a test run.
   *
   * Its own tool window, not a section of Build, because they answer different questions and
   * have different lifetimes: a build log is finished the moment you read it, a program's
   * output is a live thing you watch, type into and stop. They shared one buffer until now,
   * which meant launching an app appended the JVM's output under Maven's and the next build
   * threw both away.
   *
   * ## One panel, not three
   *
   * Debugging is not a different activity from running — it is the same launch with more to look
   * at — so it does not get a rail button and a window of its own to go and find. The panel grows
   * what the moment needs: the transport controls appear on the status row while a session is
   * attached, and the stopped program's frames and variables take the two columns to the left of
   * the transcript while it is standing still. Nothing appears when nothing is being debugged.
   *
   * **Tests joined for the same reason.** A test run is a launch: it has a command, a live
   * transcript, an exit and a Stop button, and it was getting a second copy of all four in a
   * panel next door — which is how the two came to disagree, the console having learnt to
   * interpret and virtualise its output while the test log had not. Now it is a tab in this
   * strip: {@link BennuTestView} takes the body, {@link BennuTestActions} the header, and the
   * console underneath its detail pane is this one.
   *
   * It behaves like the run tabs: it **appears when there is a run** and closes with it. What
   * does not belong here is the *catalogue* of tests the project declares — that is a property
   * of the sources rather than an event, and it lives in its own tool window
   * ({@link BennuTestsCatalogPanel}), which is also where a run starts.
   *
   * There is one test tab rather than one per run because `bennuTestStore` holds one run: its
   * tree, filters and counters are singletons. When it grows a history the strip grows with it
   * and nothing here changes.
   *
   * Debugging a test is the thing this makes possible rather than the thing it does — but that
   * is the point of putting them in one panel, and where it will land.
   *
   * What makes it a console rather than a log view:
   *   • the **command that actually ran** is its first line — the resolved `java`, the VM
   *     args, the class — so it can be pasted into a terminal when the run misbehaves;
   *   • **you can type back**. stdin is a pipe, so a program that asks a question can be
   *     answered here instead of appearing to hang — from a strip at the bottom that is
   *     collapsed until you want it, since most programs never read a line;
   *   • the output is **read, not just printed**: the backend interprets every line
   *     (`arbor-logscan`) into its level, timestamp, thread, logger, exception and stack
   *     frames — and a frame in this project is a link to the line it names;
   *   • **Stop really stops** — the backend kills the process tree, not the handle;
   *   • it ends with an **exit code and a duration**, which is the whole answer to "how did
   *     that go" and is otherwise something you reconstruct from the last line.
   *
   * Presentation only: the buffer, the lifecycle and the process live in
   * {@link bennuRunStore}.
   */
  import { Play, Bug, CornerDownLeft, ChevronDown } from 'lucide-svelte';
  import BottomPanelHeader from '$lib/components/shared/ui/BottomPanelHeader.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Tabs, { type TabItem } from '$lib/components/shared/ui/Tabs.svelte';
  import BennuConsole from './BennuConsole.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuRunStore, formatMs } from '$lib/stores/bennu/run.svelte';
  import { bennuDebugStore } from '$lib/stores/bennu/debug.svelte';
  import { activeTestStore } from '$lib/stores/bennu/test-runner.svelte';
  import BennuDebugControls from './BennuDebugControls.svelte';
  import BennuDebugFrames from './BennuDebugFrames.svelte';
  import BennuDebugValues from './BennuDebugValues.svelte';
  import BennuTestView from './BennuTestView.svelte';
  import BennuTestActions from './BennuTestActions.svelte';
  import BennuTestSummary from './BennuTestSummary.svelte';
  import BennuRunActions from './BennuRunActions.svelte';
  import { testIcon } from './test-icon';
  import ResizablePanel from '$lib/components/shared/ui/ResizablePanel.svelte';
  import { bennuDebugLayout } from './debug-layout.svelte';

  /** The id of the one Tests tab. Not a run id, and it cannot collide with one — a run id is
   *  minted by the backend. */
  const TESTS = 'tests';

  /** The runner for the open project — Maven's or cargo's. One tab either way: it is the same
   *  activity, and the strip has no business knowing which build system produced it. */
  const testStore = $derived(activeTestStore());

  /**
   * Whether there is a test run to show a tab for.
   *
   * The tab is an *event*, like every other tab here: it exists because you ran something. What
   * the project declares is a different question, answered by the Tests tool window, which is
   * always there and is where a run starts.
   */
  const hasTestRun = $derived(testStore.running || testStore.hasResults);

  /**
   * Which tab is showing.
   *
   * `bennuUiStore.runTab` is the explicit choice (the palette, a "run this test"); `null` means
   * "follow the runs", which is what you want after launching a program. A stale id — the tab it
   * named has since been closed, or the test run cleared — falls back the same way rather than
   * showing an empty panel.
   */
  const activeTab = $derived.by(() => {
    const wanted = bennuUiStore.runTab;
    if (wanted === TESTS && hasTestRun) return TESTS;
    if (wanted && wanted !== TESTS && bennuRunStore.tabs.some((t) => t.id === wanted)) return wanted;
    // No explicit choice, or a tab that has gone: follow the runs. With none at all that is the
    // empty string — the strip highlights nothing and the body says how to start one, which is a
    // better answer to "I opened Run" than quietly showing something else.
    return bennuRunStore.activeTabId ?? (hasTestRun ? TESTS : '');
  });
  const onTests = $derived(activeTab === TESTS);

  const lines = $derived(bennuRunStore.runLines);
  /** Whether the run you are LOOKING at is the live one — what the input strip keys off. An old
   *  tab must read as finished even while a newer run is going. */
  const isLive = $derived(bennuRunStore.activeIsLive);
  const stopping = $derived(bennuRunStore.stopping);
  const exitCode = $derived(bennuRunStore.runExitCode);
  const duration = $derived(bennuRunStore.runDurationMs);
  const command = $derived(bennuRunStore.runCommand);

  /**
   * The tab strip: the test run first when there is one, then one tab per launched program,
   * newest last.
   *
   * A live tab carries a ▷ so a run still going is visible from whichever tab you happen to be
   * reading. Closing the test tab clears the run, which is what closing a run tab does too.
   */
  const tabItems = $derived<TabItem[]>([
    ...(hasTestRun
      ? [{
          id: TESTS,
          label: testStore.label || 'Tests',
          title: 'The test run',
          closable: true,
          icon: testStore.running ? Play : testIcon(),
          iconSize: 11,
        }]
      : []),
    ...bennuRunStore.tabs.map((t) => ({
      id: t.id,
      label: t.label,
      title: t.command || t.subject,
      closable: true,
      icon: t.live ? Play : undefined,
      iconSize: 11,
    })),
  ]);

  /** Show a tab. A run tab also becomes the store's active run, so everything keyed off "the
   *  run you are looking at" — the transcript, ⟳, the debugger's columns — follows the strip. */
  function showTab(id: string) {
    bennuUiStore.showRunTab(id);
    if (id !== TESTS) bennuRunStore.showTab(id);
  }

  /** Close a tab: a test run is cleared, a program's transcript is dropped. Neither kills a
   *  live process — the store refuses that, and Stop is right there. */
  function closeTab(id: string) {
    if (id === TESTS) {
      testStore.clear();
      bennuUiStore.showRunTab(null);
      return;
    }
    void bennuRunStore.closeTab(id);
  }

  /**
   * The strip follows whichever run the store says is in front.
   *
   * That is a launch (it opens a tab and makes it active), and it is also a breakpoint firing in a
   * program you were not reading, which brings its tab forward the way the window comes forward.
   *
   * It used to watch for "the first live tab" instead, which was the same thing back when only one
   * program could be running: with a server already going, the tab it found never changed, and
   * launching anything else filled a tab you could not see.
   */
  let followed = '';
  $effect(() => {
    const id = bennuRunStore.activeTabId;
    if (!id || id === followed) return;
    followed = id;
    bennuUiStore.showRunTab(id);
  });

  /** The same, for a test run: starting one brings its tab forward. Edge-detected rather than
   *  keyed on the label, because rerunning the same suite twice is two launches. */
  let testsWereRunning = false;
  $effect(() => {
    const now = testStore.running;
    if (now && !testsWereRunning) bennuUiStore.showRunTab(TESTS);
    testsWereRunning = now;
  });

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

  // ── the debugger, when this run is one ────────────────────────────────────────
  /** Whether the tab you are LOOKING at is the debugged one. A session belongs to a run, and
   *  its id is that run's — so an old transcript never wears another run's controls. */
  const debugging = $derived(
    bennuDebugStore.live && bennuDebugStore.sessionId === bennuRunStore.activeTab?.runId,
  );
  /** Stopped somewhere, with a stack to show. What turns the two columns on. */
  const atBreakpoint = $derived(debugging && bennuDebugStore.paused);

  /**
   * The one phrase on the right of the status row.
   *
   * Debug-aware, because while a session is stopped "Running" is true of the process and
   * useless to the reader: what they need to know is that it is standing still and why.
   */
  const status = $derived.by(() => {
    const stopped = bennuDebugStore.stopped;
    if (atBreakpoint && stopped) {
      if (stopped.reason === 'exception') {
        return { kind: 'paused' as const, text: `Paused on ${stopped.exception ?? 'an exception'}` };
      }
      return {
        kind: 'paused' as const,
        text: stopped.reason === 'step' ? 'Stepped' : 'Paused at a breakpoint',
      };
    }
    if (isLive) {
      return { kind: 'busy' as const, text: stopping ? 'Stopping…' : 'Running' };
    }
    if (bennuRunStore.building && bennuRunStore.activeTab && !bennuRunStore.runFinished) {
      return { kind: 'busy' as const, text: 'Building…' };
    }
    if (verdict) return { kind: verdict.ok ? ('ok' as const) : ('bad' as const), text: verdict.text };
    // Reached when the compile failed: the program never started, and the last line of the
    // console says why. A spinner here would claim a build is still going.
    return { kind: 'bad' as const, text: 'Did not start' };
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
    <!-- Green, the way every IDE marks the thing that starts a program — and red while a
         debugger is attached, which is the panel saying what it currently is. -->
    {#snippet icon()}
      <span class="rp-run-icon" class:debugging class:testing={onTests}>
        {#if onTests}{@const TestIcon = testIcon()}<TestIcon size={13} />
        {:else if debugging}<Bug size={13} />
        {:else}<Play size={13} />{/if}
      </span>
    {/snippet}
    <!-- The tabs live HERE, beside the title, rather than in a row of their own: one run per
         tab is a label, and a whole strip of chrome to carry a label is a strip that says
         nothing on the common case of having one. -->
    {#if tabItems.length}
      <div class="rp-tabs">
        <Tabs
          items={tabItems}
          value={activeTab}
          variant="panel"
          size="sm"
          closable
          ariaLabel="Runs"
          onSelect={showTab}
          onClose={closeTab}
        />
      </div>
    {/if}
    <!-- One set of actions or the other, whichever tab you are on. -->
    {#snippet actions()}
      {#if onTests}<BennuTestActions />{:else}<BennuRunActions />{/if}
    {/snippet}
  </BottomPanelHeader>

  {#if onTests}
    <!-- The test run's own status row, in the same place a program's is: what it is doing on
         the right, nothing on the left yet — the debugger's transport lands there the day a
         test can be debugged. -->
    <div class="rp-status">
      <span class="rp-status-right"><BennuTestSummary /></span>
    </div>
    <BennuTestView />
  {:else if !bennuRunStore.tabs.length}
    <div class="rp-empty">
      <Play size={20} />
      <EmptyState message="Nothing running. Press ▷ (Shift+F10) to run the project, or Alt+5 for its tests." />
    </div>
  {:else}
    <!-- The status row: what the moment offers on the left, what the moment IS on the right.
         While a debugger is attached the left is its transport; otherwise there is nothing
         there, because a row of disabled buttons is chrome pretending to be a feature. -->
    <div class="rp-status">
      {#if debugging}<BennuDebugControls />{/if}
      <span class="rp-status-right">
        {#if status.kind === 'busy'}
          <Spinner size={13} />
        {:else}
          <span class="rp-dot" class:ok={status.kind === 'ok'} class:bad={status.kind === 'bad'}
                class:paused={status.kind === 'paused'}></span>
        {/if}
        <span class="rp-text">{status.text}</span>
      </span>
    </div>

    {#if command}
      <!-- Selectable on purpose: this is the line you copy into a terminal. -->
      <p class="rp-cmd">{command}</p>
    {/if}

    <!-- Stopped: the stack on the left, what is in scope beside it, the transcript keeping the
         rest. Running: the transcript is all of it. -->
    <div class="rp-body">
      {#if atBreakpoint}
        <!-- Collapsed leaves a labelled strip rather than nothing: a column dragged to zero is
             indistinguishable from a broken layout, and there is nothing left to grab. -->
        {#if bennuDebugLayout.framesOpen}
          <ResizablePanel
            initialSize={bennuDebugLayout.framesWidth}
            minSize={140}
            maxSize={640}
            onResize={(w) => bennuDebugLayout.setFramesWidth(w)}
          >
            <BennuDebugFrames />
          </ResizablePanel>
        {:else}
          <button class="rp-strip" type="button" onclick={() => bennuDebugLayout.toggleFrames()}>
            Frames
          </button>
        {/if}
        {#if bennuDebugLayout.valuesOpen}
          <ResizablePanel
            initialSize={bennuDebugLayout.valuesWidth}
            minSize={180}
            maxSize={720}
            onResize={(w) => bennuDebugLayout.setValuesWidth(w)}
          >
            <BennuDebugValues />
          </ResizablePanel>
        {:else}
          <button class="rp-strip" type="button" onclick={() => bennuDebugLayout.toggleValues()}>
            Variables
          </button>
        {/if}
      {/if}
      <BennuConsole {lines} emptyMessage="No output yet." />
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
  .rp-run-icon.debugging { color: var(--error); }
  /* Tests wins over debugging: the icon says which TAB you are on, not what is happening
     somewhere else in the panel. */
  .rp-run-icon.testing { color: var(--info); }

  /* One tab per run, in the header beside the title. Capped so a run with a long name cannot
     push the header's actions off the end. */
  /* Stretched to the header's full height so the panel variant's active-tab underline lands on
     the header's own bottom border, the way a tool-window tab strip reads in IntelliJ. */
  .rp-tabs {
    display: flex; align-self: stretch;
    min-width: 0; max-width: 60%; margin-left: 2px;
  }
  .rp-tabs :global(.tabs) { flex: 1; min-width: 0; }
  .rp-empty {
    flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 6px; color: var(--text-disabled);
  }

  .rp-status {
    display: flex; align-items: center; gap: 7px; flex-shrink: 0;
    /* A fixed height so the row does not grow by two pixels the moment the debugger's
       controls appear in it, taking the whole console down with it. */
    min-height: 28px;
    padding: 2px 12px; font-size: var(--font-size-xs); color: var(--text-secondary);
    border-bottom: 1px solid var(--border-subtle);
  }
  .rp-status-right { display: flex; align-items: center; gap: 7px; margin-left: auto; }
  .rp-text { font-weight: 500; }
  .rp-dot { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }
  .rp-dot.ok { background: var(--success); }
  .rp-dot.bad { background: var(--error); }
  .rp-dot.paused { background: var(--warning); }

  /* The console, and — while the program is standing still — the stack and the values beside
     it. A row, so the transcript keeps whatever the two columns leave. */
  .rp-body { flex: 1; display: flex; min-height: 0; min-width: 0; }
  /* A collapsed column, as a labelled strip you can click open. Vertical text so it costs the
     width of a line of type rather than of a word. */
  .rp-strip {
    flex: 0 0 auto; width: 22px;
    display: flex; align-items: center; justify-content: center;
    padding: 0; border: 0; border-right: 1px solid var(--border-subtle);
    background: none; color: var(--text-muted); cursor: pointer;
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em;
    writing-mode: vertical-rl; transform: rotate(180deg);
  }
  .rp-strip:hover { background: var(--bg-hover); color: var(--text-primary); }

  .rp-cmd {
    flex-shrink: 0; margin: 0; padding: 5px 12px;
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    white-space: pre-wrap; word-break: break-all;
    user-select: text;
  }

  .rp-caret { color: var(--accent); margin-right: 6px; user-select: none; }

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
