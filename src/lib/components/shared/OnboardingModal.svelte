<script lang="ts">
  /**
   * OnboardingModal — first-run welcome tour.
   *
   *   - Auto-opens once per `CURRENT_ONBOARDING_VERSION` bump (AppShell wires
   *     this through `onboardingStore.shouldAutoOpen()`).
   *   - Reopens manually via the Command Palette entry or `arbor:open-onboarding`
   *     window event (Docs panel link, settings, …).
   *   - Every step is skippable individually; "Skip tour" in the footer is
   *     always available and confirms via ConfirmModal.
   *
   * The design priority is visual impact at first launch — large headers,
   * accent gradient washes, generous spacing, kbd chips, and a calm rhythm
   * so the user knows immediately "this is a polished tool".
   */
  import { onMount } from 'svelte';
  import {
    Sparkles, Compass, GitBranch, Plug, Network, Folders, Ticket,
    Command as CommandIcon, Github, Gitlab, FolderOpen, FolderPlus, Download,
    ArrowLeft, ArrowRight, Check,
  } from 'lucide-svelte';

  import Modal         from './Modal.svelte';
  import ModalHeader   from './ModalHeader.svelte';
  import ModalFooter   from './ModalFooter.svelte';
  import ConfirmModal  from './ConfirmModal.svelte';
  import Callout       from './ui/Callout.svelte';
  import Button        from './ui/Button.svelte';
  import IconCard      from './ui/IconCard.svelte';
  import StepIndicator from './ui/StepIndicator.svelte';
  import ArborLogo     from './internal/ArborLogo.svelte';
  import Kbd           from './internal/Kbd.svelte';

  import { onboardingStore } from '$lib/stores/onboarding.svelte';
  import { uiStore }         from '$lib/stores/ui.svelte';

  // ── Step model ─────────────────────────────────────────────────────────────

  type StepId = 'welcome' | 'identity' | 'provider' | 'first-repo' | 'power' | 'finish';

  interface StepDef {
    id:    StepId;
    label: string;   // short label shown in the breadcrumb stepper
  }

  const STEPS: StepDef[] = [
    { id: 'welcome',    label: 'Welcome'    },
    { id: 'identity',   label: 'Identity'   },
    { id: 'provider',   label: 'Provider'   },
    { id: 'first-repo', label: 'First repo' },
    { id: 'power',      label: 'Features'   },
    { id: 'finish',     label: 'Ready'      },
  ];

  let stepIdx = $state(0);
  const step      = $derived(STEPS[stepIdx]);
  const isFirst   = $derived(stepIdx === 0);
  const isLast    = $derived(stepIdx === STEPS.length - 1);

  // Confirm-skip state — ConfirmModal is rendered on top of this modal when set.
  let skipConfirm = $state(false);

  // ── Actions ────────────────────────────────────────────────────────────────

  function next() {
    if (isLast) { finish(); return; }
    stepIdx = Math.min(stepIdx + 1, STEPS.length - 1);
  }
  function back() {
    stepIdx = Math.max(stepIdx - 1, 0);
  }
  function finish() {
    onboardingStore.finish();
  }
  function requestSkip() {
    // No prompt at step 0 — there's nothing to lose yet.
    if (isFirst) { finish(); return; }
    skipConfirm = true;
  }
  function confirmSkip() {
    skipConfirm = false;
    finish();
  }

  // ── Link handlers ──────────────────────────────────────────────────────────
  // The tour modal stays mounted from open to finish — we never close it
  // mid-flow. Only actions that open a sub-surface ON TOP of the tour are
  // wired here; everything else (Settings, Docs) appears as informational
  // text in the relevant step so the user can find them afterwards.
  //
  // Stacking targets — Command Palette, Plugin Marketplace, FilePicker,
  // Clone modal, Init modal — render OVER the tour because they're either
  // Modal-component-backed (push to the modal stack) or fixed overlays at
  // higher z-index. We just dispatch / open them; the tour stays put and
  // is visible again when the sub-flow closes.

  function openPalette() {
    // Defer to next tick so the modal's focus trap finishes settling before
    // the palette's input grabs focus — otherwise the palette opens with
    // an unfocused search field.
    queueMicrotask(() => uiStore.setCommandPaletteOpen(true));
  }

  function openMarketplace() {
    queueMicrotask(() => uiStore.openMarketplace());
  }

  function dispatchRepoVerb(verb: 'open-repo' | 'clone-repo' | 'init-repo') {
    // The sub-modal (FilePicker / Clone / Init) mounts on top of the tour
    // via the modal stack. After it closes, the tour stays put so the user
    // can hit Next.
    queueMicrotask(() => window.dispatchEvent(new CustomEvent(`arbor:${verb}`)));
  }

  // ── Keyboard contract ──────────────────────────────────────────────────────
  // Enter / Ctrl+Enter → Next, Alt+← / Alt+→ → Back / Next.
  // We intentionally do NOT bind Esc here — Modal.svelte already handles Esc
  // via its stack and calls onClose, which in our case maps to `requestSkip`.
  function onKey(e: KeyboardEvent) {
    if (skipConfirm) return;
    // Don't hijack typing inside form fields (none today, but defensive
    // for when we extend step 2 with the identity form inline).
    const t = e.target as HTMLElement | null;
    const tag = t?.tagName ?? '';
    const inField = tag === 'INPUT' || tag === 'TEXTAREA';
    if (e.key === 'Enter') {
      if (inField && !(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      next();
      return;
    }
    if (e.altKey && e.key === 'ArrowRight') { e.preventDefault(); next(); return; }
    if (e.altKey && e.key === 'ArrowLeft')  { e.preventDefault(); back(); return; }
  }

  onMount(() => {
    // Reset to step 0 on every open so re-runs from the palette don't
    // pick up stale state from a previous dismissal.
    stepIdx = 0;
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- Icon snippets reused by Button's iconStart / iconEnd props.  Declared
     up-front so consumers below pick them up regardless of Svelte 5's
     snippet-hoisting rules. -->
{#snippet backIcon()}<ArrowLeft size={14} />{/snippet}
{#snippet nextIcon()}<ArrowRight size={14} />{/snippet}
{#snippet checkIcon()}<Check size={14} strokeWidth={3} />{/snippet}

<Modal
  onClose={requestSkip}
  width="720px"
  height="560px"
  padBody={false}
  ariaLabel="Welcome to Arbor"
>
  {#snippet header()}
    <ModalHeader onClose={requestSkip}>
      <Sparkles size={14} class="hdr-icon" />
      <span class="modal-title">Welcome to Arbor</span>
      <span class="header-steps">
        <StepIndicator
          steps={STEPS}
          current={step.id}
          variant="pill"
          collapseLabels
          onStepClick={(_id, i) => { stepIdx = i; }}
        />
      </span>
    </ModalHeader>
  {/snippet}

  <!-- Body — one section per step. Background fills the body card and we
       handle our own padding so each step can opt into a hero wash. -->
  <div class="ob-body" data-step={step.id}>
    {#if step.id === 'welcome'}
      <section class="hero">
        <div class="hero-logo"><ArborLogo size={64} /></div>
        <h1>Arbor</h1>
        <p class="tagline">A keyboard-first Git client that gets out of your way.</p>
        <ul class="pillars" role="list">
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Everything by keyboard"
              description="Command Palette & rich shortcuts cover every action."
            >
              {#snippet icon()}<CommandIcon size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Extend with Lua"
              description="Plugins add panels, pipelines, and integrations — natively."
            >
              {#snippet icon()}<Plug size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Multi-repo at heart"
              description="Workspaces and linked worktrees keep big setups in sync."
            >
              {#snippet icon()}<Network size={16} />{/snippet}
            </IconCard>
          </li>
        </ul>
      </section>

    {:else if step.id === 'identity'}
      <section class="step-section">
        <header class="step-header">
          <div class="step-icon"><GitBranch size={22} /></div>
          <div>
            <h2>Git identity</h2>
            <p>Commits need a name and email. Arbor falls back to your global <code>git config</code> in the meantime — set Arbor-specific values later from <strong>Settings → Authentication</strong>.</p>
          </div>
        </header>
        <Callout variant="tip" title="No setup required to start">
          You can commit immediately if your system <code>git</code> identity is already configured. Otherwise Arbor will prompt you on the first commit.
        </Callout>
      </section>

    {:else if step.id === 'provider'}
      <section class="step-section">
        <header class="step-header">
          <div class="step-icon teaser-icon"><Plug size={22} /></div>
          <div>
            <h2>Connect a remote provider</h2>
            <p>Optional but it unlocks merge / pull requests, pipeline status, issue tracking and security findings — all inline with your commits.</p>
          </div>
        </header>
        <div class="three-up">
          <IconCard
            title="GitHub"
            description="PRs, issues, Actions, code scanning."
            layout="stack"
            tone="accent"
          >
            {#snippet icon()}<Github size={22} />{/snippet}
          </IconCard>
          <IconCard
            title="GitLab"
            description="MRs, issues, pipelines, security reports."
            layout="stack"
            tone="accent"
          >
            {#snippet icon()}<Gitlab size={22} />{/snippet}
          </IconCard>
          <IconCard
            title="Linear · Jira"
            description="Click ticket chips on commits to open the issue."
            layout="stack"
            tone="accent"
          >
            {#snippet icon()}<Ticket size={22} />{/snippet}
          </IconCard>
        </div>
        <Callout variant="info" title="Connect when you're ready">
          Provider tokens live in <strong>Settings → Authentication</strong>. Everything below works offline first — you can wire a provider in any time without losing context.
        </Callout>
      </section>

    {:else if step.id === 'first-repo'}
      <section class="step-section">
        <header class="step-header">
          <div class="step-icon"><FolderOpen size={22} /></div>
          <div>
            <h2>Your first repository</h2>
            <p>Open one you already have on disk, clone from a remote, or start fresh.</p>
          </div>
        </header>
        <div class="three-up">
          <IconCard
            title="Open local"
            description="Pick a folder you already cloned."
            layout="stack"
            size="lg"
            tone="accent"
            interactive
            onclick={() => dispatchRepoVerb('open-repo')}
          >
            {#snippet icon()}<FolderOpen size={26} />{/snippet}
            {#snippet trailing()}<Kbd action="open_repo" size="sm" />{/snippet}
          </IconCard>
          <IconCard
            title="Clone"
            description="Pull a remote repo into a new tab."
            layout="stack"
            size="lg"
            tone="accent"
            interactive
            onclick={() => dispatchRepoVerb('clone-repo')}
          >
            {#snippet icon()}<Download size={26} />{/snippet}
            {#snippet trailing()}<Kbd action="clone_repo" size="sm" />{/snippet}
          </IconCard>
          <IconCard
            title="Initialize"
            description="Start a brand-new repo from a folder."
            layout="stack"
            size="lg"
            tone="accent"
            interactive
            onclick={() => dispatchRepoVerb('init-repo')}
          >
            {#snippet icon()}<FolderPlus size={26} />{/snippet}
            {#snippet trailing()}<Kbd action="init_repo" size="sm" />{/snippet}
          </IconCard>
        </div>
      </section>

    {:else if step.id === 'power'}
      <section class="step-section">
        <header class="step-header">
          <div class="step-icon"><Compass size={22} /></div>
          <div>
            <h2>What makes Arbor click</h2>
            <p>A short tour of the features power users reach for first. Press the shortcut, or pick "Try it" to jump straight in.</p>
          </div>
        </header>

        <ul class="feature-list" role="list">
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Command Palette"
              description="Every action — branches, commits, plugin commands, themes — one fuzzy search away. If you don't remember the shortcut, just type the verb."
            >
              {#snippet icon()}<CommandIcon size={18} />{/snippet}
              {#snippet titleExtra()}<Kbd action="command_palette" size="sm" />{/snippet}
              {#snippet trailing()}<Button variant="secondary" size="sm" onclick={openPalette}>Try it</Button>{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Plugin marketplace"
              description="Install plugins and themes written in Lua. They add panels, pipelines, integrations and even new file-format studios — all sandboxed."
            >
              {#snippet icon()}<Plug size={18} />{/snippet}
              {#snippet titleExtra()}<Kbd action="open_marketplace" size="sm" />{/snippet}
              {#snippet trailing()}<Button variant="secondary" size="sm" onclick={openMarketplace}>Browse</Button>{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Issue tracker integration"
            >
              {#snippet icon()}<Ticket size={18} />{/snippet}
              {#snippet extra()}
                <span class="feat-desc-rich">Arbor auto-detects ticket IDs in commit messages and branch names (Linear, Jira, GitHub, GitLab). A chip appears next to each commit — <em>click it to open the issue instantly</em>, no context switching.</span>
              {/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Linked worktrees"
              description="Pin sibling repos together — checking out a branch in one propagates to all of them, with conflict detection. Microservice setups stop feeling like 12 separate windows."
            >
              {#snippet icon()}<Network size={18} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Workspaces"
              description="Group repos by project / customer / context. Switch context with one shortcut — tabs, pinned branches and sidebar all follow."
            >
              {#snippet icon()}<Folders size={18} />{/snippet}
              {#snippet titleExtra()}<Kbd action="workspace_manager" size="sm" />{/snippet}
            </IconCard>
          </li>
        </ul>
      </section>

    {:else if step.id === 'finish'}
      <section class="hero finish">
        <div class="finish-mark"><Check size={36} strokeWidth={3} /></div>
        <h1>You're ready.</h1>
        <p class="tagline">Press <Kbd action="command_palette" size="sm" /> any time — it's the fastest way to discover the rest.</p>
        <div class="finish-links">
          <!-- On the FINAL step the action buttons mark the tour completed
               (`finish()`) before navigating: the user is signalling
               "I'm done with the tour, take me to <target>", not
               "show me <target> and bring me back". -->
          <Button
            variant="secondary"
            onclick={() => { finish(); queueMicrotask(() => uiStore.setPanel('docs')); }}
          >Open documentation</Button>
          <Button
            variant="ghost"
            onclick={() => { finish(); queueMicrotask(() => uiStore.setCommandPaletteOpen(true)); }}
          >Show command palette</Button>
        </div>
      </section>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" onclick={requestSkip}>
        {isLast ? 'Close' : 'Skip tour'}
      </Button>
      <div class="footer-right">
        <Button variant="secondary" onclick={back} disabled={isFirst} iconStart={backIcon}>
          Back
        </Button>
        {#if isLast}
          <Button variant="primary" onclick={finish} iconStart={checkIcon}>
            Finish
          </Button>
        {:else}
          <Button variant="primary" onclick={next} iconEnd={nextIcon}>
            Next
          </Button>
        {/if}
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if skipConfirm}
  <ConfirmModal
    title="Skip the welcome tour?"
    message="You can re-open it anytime from the command palette or from the Documentation panel."
    confirmLabel="Skip"
    cancelLabel="Keep going"
    variant="info"
    onConfirm={confirmSkip}
    onCancel={() => (skipConfirm = false)}
  />
{/if}

<style>
  /* ── Header ──────────────────────────────────────────────────────────── */
  :global(.hdr-icon) { color: var(--accent); flex-shrink: 0; }

  /* The StepIndicator floats to the right of the modal title. Pushing it
     here (instead of inside the widget) keeps StepIndicator generic — it
     doesn't know whether it lives in a modal header, a sidebar panel, or
     a settings page. */
  .header-steps {
    margin-left: auto;
    display: flex;
    align-items: center;
    min-width: 0;
  }

  /* ── Body shell ────────────────────────────────────────────────────────── */
  .ob-body {
    height: 100%;
    overflow: auto;
    padding: 28px 32px;
    background:
      radial-gradient(120% 60% at 0% 0%, color-mix(in srgb, var(--accent) 10%, transparent), transparent 70%),
      radial-gradient(80% 60% at 100% 100%, color-mix(in srgb, var(--accent) 6%, transparent), transparent 70%),
      var(--bg-base);
  }

  /* ── Hero (welcome + finish) ──────────────────────────────────────────── */
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    height: 100%;
    gap: 14px;
    padding: 4px;
  }
  .hero-logo {
    width: 88px;
    height: 88px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 24px;
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent),
      0 14px 40px -10px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .hero h1 {
    margin: 4px 0 0;
    font-size: 30px;
    font-weight: 600;
    letter-spacing: -0.5px;
  }
  .hero .tagline {
    color: var(--text-secondary);
    font-size: var(--font-size-md);
    margin: 0 0 6px;
    max-width: 460px;
    line-height: 1.45;
  }
  /* Pillars list — IconCard does the visual work, this is just the layout
     container (vertical stack, max-width to keep the hero readable). */
  .pillars {
    list-style: none;
    margin: 18px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    max-width: 460px;
    text-align: left;
  }
  .pillars li { display: contents; }

  .finish .finish-mark {
    width: 88px;
    height: 88px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--success) 18%, transparent);
    color: var(--success);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--success) 45%, transparent),
      0 14px 40px -10px color-mix(in srgb, var(--success) 55%, transparent);
  }
  .finish-links {
    display: flex;
    gap: 10px;
    margin-top: 12px;
  }

  /* ── Generic step section ─────────────────────────────────────────────── */
  .step-section {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .step-header {
    display: flex;
    align-items: flex-start;
    gap: 14px;
  }
  .step-header h2 {
    margin: 2px 0 4px;
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.2px;
  }
  .step-header p {
    margin: 0;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    line-height: 1.5;
    max-width: 560px;
  }
  .step-icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-subtle);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .step-icon.teaser-icon {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }
  .step-header p code {
    font-family: var(--font-code);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: 4px;
    border: 1px solid var(--border-subtle);
    color: var(--text-secondary);
    font-size: 0.9em;
  }

  /* ── Three-up grid (provider tiles + first-repo tiles) ───────────────── */
  /* Both the provider teaser row and the open/clone/init picker render
     three IconCards side by side; this is the only layout still owned
     here, the tiles themselves are stock IconCard. */
  .three-up {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 12px;
  }

  /* ── Feature list (power-features step) ──────────────────────────────── */
  /* Same trick as `.pillars`: the list element is structural, IconCard
     paints the row. */
  .feature-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .feature-list li { display: contents; }

  /* Issue-tracker description carries an inline accent emphasis ("click it
     to open the issue instantly"). Lives here because IconCard's stock
     `description` is plain text — when the copy needs richer markup we
     pass it via the `extra` snippet, and this rule styles it. */
  .feat-desc-rich {
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .feat-desc-rich em {
    font-style: normal;
    color: var(--accent);
    font-weight: 500;
  }

  /* ── Footer ───────────────────────────────────────────────────────────── */
  .footer-right { display: flex; gap: 8px; }
</style>
