<script lang="ts">
  /**
   * Bennu's first-run tour — the content of it.
   *
   * The dialog is `shared/OnboardingShell`, the same one Arbor's welcome tour goes through:
   * stepper, footer, keyboard contract, skip confirm, and the hero / step-header / card-grid
   * vocabulary the markup below is written in. Only the steps are Bennu's.
   *
   * The steps answer the questions somebody actually has in their first ten minutes, in the
   * order they have them: what is this, how do I open something, where is everything, how do I
   * get around a codebase I have not read, and what does the editor know about a language that
   * is not Java. Each one names the shortcut, because the shortcut is the point.
   */
  import {
    Rocket, FolderOpen, Compass, ServerCog, Command as CommandIcon,
    LayoutDashboard, Search, History, ShieldCheck, Boxes, Cog, Gamepad2, Check,
  } from 'lucide-svelte';

  import OnboardingShell, { type OnboardingStep } from '$lib/components/shared/OnboardingShell.svelte';
  import ProductIcon from '$lib/components/shared/internal/ProductIcon.svelte';
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import IconCard from '$lib/components/shared/ui/IconCard.svelte';

  import { bennuOnboardingStore } from '$lib/stores/bennu/onboarding.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';

  const STEPS: OnboardingStep[] = [
    { id: 'welcome',  label: 'Welcome'   },
    { id: 'project',  label: 'Project'   },
    { id: 'layout',   label: 'Layout'    },
    { id: 'navigate', label: 'Navigate'  },
    { id: 'languages', label: 'Languages' },
    { id: 'finish',   label: 'Ready'     },
  ];

  function finish() {
    bennuOnboardingStore.finish();
  }

  // Sub-surfaces open ON TOP of the tour — it is never closed mid-flow, and it is right there
  // again when the sub-flow closes. Deferred a tick so the modal's focus trap has settled
  // before the next surface takes focus.
  function openProjectPicker() {
    // The same event Ctrl+O fires; the title bar's picker listens for it. Going through the
    // event rather than reaching into the title bar keeps one route to the picker.
    queueMicrotask(() => window.dispatchEvent(new CustomEvent('bennu:open-project')));
  }
  function openPalette() {
    queueMicrotask(() => bennuUiStore.togglePalette());
  }
  function openDocs() {
    finish();
    queueMicrotask(() => bennuUiStore.toggleDocs());
  }
</script>

<OnboardingShell steps={STEPS} title="Welcome to Bennu" onFinish={finish}>
  {#snippet content(stepId: string)}
    {#if stepId === 'welcome'}
      <section class="hero">
        <div class="hero-logo"><ProductIcon id="bennu" size={64} /></div>
        <h1>Bennu</h1>
        <p class="tagline">A code intelligence engine, with an editor around it.</p>
        <ul class="pillars" role="list">
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="It reads the configuration"
              description="Struts, Spring XML, MyBatis, JSP — where the behaviour is, not just where the code is."
            >
              {#snippet icon()}<Boxes size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Rust as a first-class citizen"
              description="A Cargo workspace opens with its crates, its tests, its debugger and rust-analyzer."
            >
              {#snippet icon()}<Cog size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard
              size="sm"
              tone="accent"
              title="Everything by keyboard"
              description="Every action has a shortcut and a Command Palette entry. The mouse is optional."
            >
              {#snippet icon()}<CommandIcon size={16} />{/snippet}
            </IconCard>
          </li>
        </ul>
      </section>

    {:else if stepId === 'project'}
      <section class="step-section">
        <div class="step-header">
          <span class="step-icon"><FolderOpen size={20} /></span>
          <h2>Open a project</h2>
          <p>
            A project is the folder with the root manifest in it — a Maven <code>pom.xml</code>
            or a Cargo <code>Cargo.toml</code>. Bennu reads the build model from it: the modules
            or workspace crates, the JDK language level, and which frameworks the code relies on.
          </p>
        </div>

        <div class="three-up">
          <IconCard
            title="Open a folder"
            description="Ctrl+O, or the project switcher in the title bar."
          >
            {#snippet icon()}<FolderOpen size={18} />{/snippet}
          </IconCard>
          <IconCard
            title="Several at once"
            description="A workspace holds more than one project; the switcher moves between them."
          >
            {#snippet icon()}<Boxes size={18} />{/snippet}
          </IconCard>
          <IconCard
            title="The index"
            description="Builds in the background. The footer says how far along it is."
          >
            {#snippet icon()}<ShieldCheck size={18} />{/snippet}
          </IconCard>
        </div>

        <div class="finish-links">
          <Button variant="secondary" onclick={openProjectPicker}>Open a project now</Button>
        </div>

        <Callout variant="info" title="Nothing to hand?">
          <strong>Load demo project</strong> in the hamburger menu opens a realistic sample — a
          Struts portal — with a populated tree and a highlighted Java file. Everything in this
          tour can be tried on it.
        </Callout>
      </section>

    {:else if stepId === 'layout'}
      <section class="step-section">
        <div class="step-header">
          <span class="step-icon"><LayoutDashboard size={20} /></span>
          <h2>Where everything is</h2>
          <p>
            Two icon rails and a dock, IntelliJ-style. Each rail button toggles its tool window,
            and each one has a shortcut — <kbd>Alt</kbd> + a digit, shown in its tooltip.
          </p>
        </div>

        <ul class="feature-list" role="list">
          <li>
            <IconCard size="sm" title="Left rail" description="Project files, the Structure of the open file, and Dependencies.">
              {#snippet icon()}<Boxes size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard size="sm" title="Right rail" description="Maven or Cargo, the test catalogue, the parse trees, and the panels a framework asked for.">
              {#snippet icon()}<Cog size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard size="sm" title="Bottom dock" description="Build and Problems, the Run console (the debugger lives there too), TODO, Forms, and the Terminal.">
              {#snippet icon()}<ServerCog size={16} />{/snippet}
            </IconCard>
          </li>
        </ul>

        <Callout variant="tip" title="Make it yours">
          <strong>Customize Activity Bar…</strong> — in the Command Palette and under the gear —
          reorders the icons on both rails and hides the tools you never use. Nothing becomes
          unreachable: every one of them keeps its shortcut and its palette entry.
        </Callout>
      </section>

    {:else if stepId === 'navigate'}
      <section class="step-section">
        <div class="step-header">
          <span class="step-icon"><Compass size={20} /></span>
          <h2>Getting around code you have not read</h2>
          <p>
            All of this answers from the index rather than from the open tabs, so it finds what
            you have never opened — including names that only exist in an XML file.
          </p>
        </div>

        <div class="three-up">
          <IconCard title="Go to declaration" description="Ctrl+B, or Ctrl+click. Follows an action= string into its Struts config and its class.">
            {#snippet icon()}<Compass size={18} />{/snippet}
          </IconCard>
          <IconCard title="Find usages" description="Alt+F7. A real reference index, so a name that merely looks the same is not a hit.">
            {#snippet icon()}<Search size={18} />{/snippet}
          </IconCard>
          <IconCard title="Local history" description="Alt+Shift+H. Every version of a file Bennu has seen, whether or not it was committed.">
            {#snippet icon()}<History size={18} />{/snippet}
          </IconCard>
        </div>

        <Callout variant="tip" title="When in doubt">
          <kbd>Ctrl</kbd> + <kbd>K</kbd> opens the <strong>Command Palette</strong>. Every action
          Bennu has is in it, by name, with its shortcut beside it.
          <div class="finish-links">
            <Button variant="ghost" onclick={openPalette}>Show the palette</Button>
          </div>
        </Callout>
      </section>

    {:else if stepId === 'languages'}
      <section class="step-section">
        <div class="step-header">
          <span class="step-icon"><ServerCog size={20} /></span>
          <h2>Java is Bennu's own; the rest is a language server</h2>
          <p>
            The Java intelligence is an engine Bennu owns. Every other language is served by an
            external <strong>language server</strong>, and the editor treats both identically —
            same completion, same go-to, same hover.
          </p>
        </div>

        <ul class="feature-list" role="list">
          <li>
            <IconCard size="sm" title="Install one from inside Bennu" description="Settings → Language Servers lists what it knows about and installs the missing ones through the package manager you already have.">
              {#snippet icon()}<ServerCog size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard size="sm" title="Rust" description="rust-analyzer, plus Cargo's own tool window, the test catalogue, and a real debugger.">
              {#snippet icon()}<Cog size={16} />{/snippet}
            </IconCard>
          </li>
          <li>
            <IconCard size="sm" title="Shaders and Bevy" description="A .wgsl gets completion, go-to, hover and naga's own compiler errors with nothing installed at all.">
              {#snippet icon()}<Gamepad2 size={16} />{/snippet}
            </IconCard>
          </li>
        </ul>
      </section>

    {:else if stepId === 'finish'}
      <section class="hero finish">
        <div class="finish-mark"><Check size={40} strokeWidth={3} /></div>
        <h1>That is the tour</h1>
        <p class="tagline">
          The manual is under <kbd>F1</kbd> — forty short pages, searchable, one per topic.
        </p>
        <div class="finish-links">
          <Button variant="secondary" onclick={openDocs}>Open the documentation</Button>
          <Button variant="ghost" onclick={openPalette}>Show the command palette</Button>
        </div>
      </section>
    {/if}
  {/snippet}
</OnboardingShell>
