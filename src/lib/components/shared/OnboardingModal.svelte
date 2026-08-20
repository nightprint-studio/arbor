<script lang="ts">
  /**
   * Arbor's first-run welcome tour — the content of it.
   *
   * The dialog itself (stepper, footer, keyboard contract, skip confirm, and the hero /
   * step-header / card-grid vocabulary the markup below uses) is `OnboardingShell`, which
   * Bennu's tour goes through too. What stays here is what is actually about Arbor: which
   * steps there are, and what each one says.
   *
   *   - Auto-opens once per `CURRENT_ONBOARDING_VERSION` bump (AppShell wires this through
   *     `onboardingStore.shouldAutoOpen()`).
   *   - Re-opens from the Command Palette or the `arbor:open-onboarding` window event.
   */
  import {
    Compass, GitBranch, Plug, Network, Folders, Ticket,
    Command as CommandIcon, Github, Gitlab, FolderOpen, FolderPlus, Download,
    HardDrive, Check,
  } from 'lucide-svelte';

  import OnboardingShell, { type OnboardingStep } from './OnboardingShell.svelte';
  import Callout   from './ui/Callout.svelte';
  import Button    from './ui/Button.svelte';
  import IconCard  from './ui/IconCard.svelte';
  import ArborLogo from './internal/ArborLogo.svelte';
  import Kbd       from './internal/Kbd.svelte';

  import { onboardingStore } from '$lib/stores/onboarding.svelte';
  import { uiStore }         from '$lib/stores/ui.svelte';
  import { openExplorerWindow } from '$lib/ipc/app';

  const STEPS: OnboardingStep[] = [
    { id: 'welcome',    label: 'Welcome'    },
    { id: 'identity',   label: 'Identity'   },
    { id: 'provider',   label: 'Provider'   },
    { id: 'first-repo', label: 'First repo' },
    { id: 'power',      label: 'Features'   },
    { id: 'finish',     label: 'Ready'      },
  ];

  // ── Link handlers ──────────────────────────────────────────────────────────
  // The tour stays mounted from open to finish — it is never closed mid-flow. Only actions
  // that open a sub-surface ON TOP of it are wired here; everything else (Settings, Docs)
  // appears as informational text in the relevant step so the user can find it afterwards.
  //
  // The stacking targets — Command Palette, Plugin Marketplace, file picker, Clone, Init —
  // render over the tour because they are either `Modal`-backed (so they push onto the modal
  // stack) or fixed overlays at a higher z-index. Dispatching one leaves the tour where it is,
  // visible again the moment the sub-flow closes.

  function openPalette() {
    // Deferred a tick so the modal's focus trap has settled before the palette's input takes
    // focus — otherwise the palette opens with an unfocused search field.
    queueMicrotask(() => uiStore.setCommandPaletteOpen(true));
  }

  function openMarketplace() {
    queueMicrotask(() => uiStore.openMarketplace());
  }

  function openExplorer() {
    // The dedicated File Explorer, in its own OS window — the tour stays put in this one.
    queueMicrotask(() => { void openExplorerWindow(); });
  }

  function dispatchRepoVerb(verb: 'open-repo' | 'clone-repo' | 'init-repo') {
    queueMicrotask(() => window.dispatchEvent(new CustomEvent(`arbor:${verb}`)));
  }

  function finish() {
    onboardingStore.finish();
  }
</script>

<OnboardingShell steps={STEPS} title="Welcome to Arbor" onFinish={finish}>
  {#snippet content(stepId: string)}
  {#if stepId === 'welcome'}
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

  {:else if stepId === 'identity'}
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

  {:else if stepId === 'provider'}
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

  {:else if stepId === 'first-repo'}
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

  {:else if stepId === 'power'}
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
            title="Built-in File Explorer"
            description="Browse your real filesystem without leaving Arbor — git status overlays, previews and a Changes panel — in its own window. It also powers every file & folder picker in the app."
          >
            {#snippet icon()}<HardDrive size={18} />{/snippet}
            {#snippet trailing()}<Button variant="secondary" size="sm" onclick={openExplorer}>Try it</Button>{/snippet}
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

  {:else if stepId === 'finish'}
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
  {/snippet}
</OnboardingShell>
