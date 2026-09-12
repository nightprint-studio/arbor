<script lang="ts">
  /**
   * Bennu settings — the two-pane settings surface (shared `SettingsShell`), the same look as
   * Arbor's and merula's.
   *
   * This file is the **nav and nothing else**: every page is a component under `settings/`, and what
   * is here is which pages exist, how they are grouped, and which one is showing. A page that needs
   * a config field reads it from `bennuConfigStore`, so two pages open on the same field cannot hold
   * two snapshots of it.
   *
   * The tree is the IDE shape: a parent is a page in its own right and its children are what it
   * splits into — Editor › Completion, Java › Code Style. Selecting a parent opens it.
   *
   * **Per-project settings are not here.** The JDK a project targets, its encoding, its naming rules
   * and the frameworks detected in it live in *Project Configuration* (the title bar's gear, or the
   * command palette): a dialog whose title is "Settings" and whose contents change with the project
   * open is a dialog nobody can reason about. What stays here is what belongs to you and to this
   * machine — including the JDK *search paths*, which are where JDKs are installed, not which one a
   * project wants.
   *
   * The first group is not Bennu's at all: `Interface` is the shell's shared settings, which already
   * applied to this window and simply had no dialog in it. They are the same components Corvus's
   * settings panel renders.
   */
  import {
    Settings, Coffee, TextCursorInput, ListTree, Bug, FoldVertical, Braces, RotateCcw, Wand2,
    ServerCog, Monitor, Sparkles, Command, Terminal, Beaker, FileCode2, Save, Boxes, Workflow,
    FileStack,
  } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ThemeEditorModal from '$lib/components/shared/ThemeEditorModal.svelte';
  import AppearanceSettings from '$lib/components/shared/internal/AppearanceSettings.svelte';
  import AnimationsSettings from '$lib/components/shared/internal/AnimationsSettings.svelte';
  import KeystrokesSettings from '$lib/components/shared/internal/KeystrokesSettings.svelte';
  import TerminalsSettings from '$lib/components/shared/internal/TerminalsSettings.svelte';
  import BennuTemplatesSettings from './templates/BennuTemplatesSettings.svelte';
  import { TEMPLATE_KIND_ICONS } from './templates/template-kinds';
  import BennuValueRulesSettings from './dtolab/BennuValueRulesSettings.svelte';
  import BennuEditorSettings from './settings/BennuEditorSettings.svelte';
  import BennuFilesSettings from './settings/BennuFilesSettings.svelte';
  import BennuCompletionSettings from './settings/BennuCompletionSettings.svelte';
  import BennuFoldingSettings from './settings/BennuFoldingSettings.svelte';
  import BennuJavaSettings from './settings/BennuJavaSettings.svelte';
  import BennuJavaStyleSettings from './settings/BennuJavaStyleSettings.svelte';
  import BennuJdkSettings from './settings/BennuJdkSettings.svelte';
  import BennuDebuggerSettings from './settings/BennuDebuggerSettings.svelte';
  import BennuLspSettings from './settings/BennuLspSettings.svelte';
  import BennuRustSettings from './settings/BennuRustSettings.svelte';
  import BennuSpringSettings from './settings/BennuSpringSettings.svelte';
  import BennuStrutsSettings from './settings/BennuStrutsSettings.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
  import { bennuTemplatesStore } from '$lib/stores/bennu/templates.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import type { TemplateKindId } from '$lib/ipc/bennu/templates';

  let { onClose }: { onClose: () => void } = $props();

  /** The page showing. Declared before everything that reads it: a `$derived` below would otherwise
   *  close over a name that is not there yet. */
  let active = $state('editor');

  /** The theme editor, opened from the Appearance page. Mounted here as well as on the title bar's
   *  gear because a settings page that names the theme and cannot change it sends you looking
   *  through the chrome for the button — the modal itself is stateless and shared. */
  let themeEditorOpen = $state(false);

  // Every page that writes a config field reads it from here, so the dialog holds one snapshot.
  $effect(() => { void bennuConfigStore.load(); });

  // The kinds of template, for the tree. Re-read when the project changes: which kinds apply is the
  // project's answer, and a kind's own templates are listed per project too.
  $effect(() => {
    void projectStore.project?.root;
    void bennuTemplatesStore.load();
  });

  const isCargo = $derived(projectStore.isCargo);
  const caps = $derived(projectStore.capabilities);
  const hasSpring = $derived(!!(caps?.spring_xml_di || caps?.spring_annotation_di || caps?.spring_data_repo));
  const hasStruts = $derived(!!(caps?.struts_xml_config || caps?.struts_convention || caps?.jsp_views));
  /** A Rust page is worth a row where there is Rust to configure — a Cargo project, or a machine
   *  that has rust-analyzer installed and a polyglot repository to use it on. */
  const hasRust = $derived(isCargo || bennuLspStore.servers.some((x) => x.language === 'rust'));

  /** One child per kind of template. The ids are prefixed so a kind can never collide with a page,
   *  and the icons come from the shared map — the same glyph here, in the overview and in the list. */
  const TEMPLATE_PAGES: { id: string; kind: TemplateKindId; label: string }[] = [
    { id: 'tpl:new-file', kind: 'new-file', label: 'New file' },
    { id: 'tpl:class', kind: 'class', label: 'From a class' },
    { id: 'tpl:config-properties', kind: 'config-properties', label: 'Configuration properties' },
    { id: 'tpl:config-class', kind: 'config-class', label: 'Configuration class' },
    { id: 'tpl:validation-tests', kind: 'validation-tests', label: 'Validation tests' },
    { id: 'tpl:live', kind: 'live', label: 'Abbreviations' },
  ];
  const templateKind = $derived(TEMPLATE_PAGES.find((p) => p.id === active)?.kind ?? null);

  /**
   * The kinds this project can generate with.
   *
   * Four of the six read a Java class, the Spring model or Bean Validation, so on a Cargo workspace
   * they have nothing to run on. Which four is the backend's answer rather than a list repeated
   * here — and until it has answered the branch stays empty, so no page appears and then vanishes.
   */
  const templatePages = $derived(
    TEMPLATE_PAGES.filter((page) => {
      const info = bennuTemplatesStore.of(page.kind);
      return !!info && !(info.java_only && isCargo);
    }),
  );

  /** The Java-only pages drop out on a Cargo project: Code Style, the JDK locations and the Java
   *  page are each a statement about a Java stack. Editor / Completion / Folding apply to every
   *  buffer and stay. */
  const groups = $derived<SettingsNavGroup[]>([
    // First, and above the editor: it is the group that answers "why is everything so small", which
    // is a question asked before any of the others.
    { label: 'Interface', items: [
      { id: 'appearance', label: 'Appearance', icon: Monitor },
      { id: 'animations', label: 'Animations', icon: Sparkles },
      { id: 'keystrokes', label: 'Keyboard Inputs', icon: Command },
      // The shells the built-in Terminal offers. Bennu's terminal is the same one Arbor's is — same
      // store, same backend — so this is not a copy of that page, it is that page.
      { id: 'terminals',  label: 'Terminals', icon: Terminal },
    ] },
    { label: 'Editor', items: [
      { id: 'editor', label: 'Editor', icon: TextCursorInput, children: [
        { id: 'completion', label: 'Completion', icon: ListTree },
        { id: 'folding',    label: 'Folding',    icon: FoldVertical },
        { id: 'files',      label: 'Files',      icon: Save },
      ] },
      // Global, like the templates it lists — a project only chooses among them.
      { id: 'templates', label: 'Code Templates', icon: FileCode2, children: templatePages.map((p) => ({
        id: p.id, label: p.label, icon: TEMPLATE_KIND_ICONS[p.kind],
      })) },
    ] },
    { label: 'Languages & Frameworks', items: [
      // The languages and the frameworks are told apart by colour rather than by reading four labels
      // — the one group where a row stands for a thing rather than for a setting.
      ...(isCargo ? [] : [{
        id: 'java', label: 'Java', icon: Braces, iconColor: 'var(--warning)', children: [
          { id: 'style',       label: 'Code Style',    icon: Wand2 },
          { id: 'jdk',         label: 'JDK locations', icon: Coffee },
          { id: 'debugger',    label: 'Debugger',      icon: Bug },
          // Global like the templates: what a field called `email` is given does not change per project.
          { id: 'test-values', label: 'Test Values',   icon: Beaker },
        ],
      }]),
      ...(hasRust ? [{ id: 'rust', label: 'Rust', icon: FileStack, iconColor: 'var(--error)' }] : []),
      // Always present, on every project kind: it is where a *missing* server is explained, and
      // hiding it on a Java project would hide the answer to "why does my `.rs` file have no go-to"
      // from exactly the polyglot repo that has one.
      { id: 'languages', label: 'Language Servers', icon: ServerCog },
      // A framework gets a page where the project has that framework — a page of Struts settings on
      // a Boot service is a door onto something that will never apply.
      ...(hasSpring ? [{ id: 'spring', label: 'Spring', icon: Boxes, iconColor: 'var(--success)' }] : []),
      ...(hasStruts ? [{ id: 'struts', label: 'Struts & JSP', icon: Workflow, iconColor: 'var(--info)' }] : []),
    ] },
  ]);

  // Something opened Settings *for a reason* (the status bar's "server not running" pill) and asked
  // for a page. Honoured once, then cleared, so ordinary re-opens stay where the user was.
  $effect(() => {
    const requested = bennuUiStore.settingsSection;
    if (!requested) return;
    active = requested;
    bennuUiStore.consumeSettingsSection();
  });

  // A page that just disappeared (the project switched to Cargo while it was open) would leave the
  // shell on something with no nav entry. Fall back to the Editor.
  $effect(() => {
    const ids = groups.flatMap((g) => g.items.flatMap((i) => [i.id, ...(i.children ?? []).map((c) => c.id)]));
    if (!ids.includes(active)) active = 'editor';
  });
</script>

<Modal {onClose} width="1020px" height="680px" padBody={false} ariaLabel="Bennu Settings">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Settings size={14} />
      <span class="modal-title">Settings</span>
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active>
    {#snippet content()}
      {#if active === 'appearance'}
        <AppearanceSettings
          onOpenThemeEditor={() => { themeEditorOpen = true; }}
          onCustomizeBars={() => { onClose(); bennuUiStore.openCustomizeRails(); }}
        />
      {:else if active === 'animations'}
        <AnimationsSettings />
      {:else if active === 'keystrokes'}
        <KeystrokesSettings />
      {:else if active === 'terminals'}
        <TerminalsSettings />
      {:else if active === 'editor'}
        <BennuEditorSettings />
      {:else if active === 'files'}
        <BennuFilesSettings />
      {:else if active === 'completion'}
        <BennuCompletionSettings />
      {:else if active === 'folding'}
        <BennuFoldingSettings />
      {:else if active === 'templates'}
        <BennuTemplatesSettings onOpenKind={(k) => (active = `tpl:${k}`)} />
      {:else if templateKind}
        <BennuTemplatesSettings kind={templateKind} />
      {:else if active === 'java'}
        <BennuJavaSettings onGoTo={(page) => (active = page)} />
      {:else if active === 'style'}
        <BennuJavaStyleSettings />
      {:else if active === 'jdk'}
        <BennuJdkSettings />
      {:else if active === 'debugger'}
        <BennuDebuggerSettings />
      {:else if active === 'test-values'}
        <BennuValueRulesSettings />
      {:else if active === 'languages'}
        <BennuLspSettings />
      {:else if active === 'rust'}
        <BennuRustSettings />
      {:else if active === 'spring'}
        <BennuSpringSettings />
      {:else if active === 'struts'}
        <BennuStrutsSettings />
      {/if}
    {/snippet}
  </SettingsShell>

  {#snippet footer()}
    <ModalFooter align="between">
      <Button variant="ghost" size="sm" onclick={() => bennuSettingsStore.resetToDefaults()}>
        {#snippet iconStart()}<RotateCcw size={13} />{/snippet}
        Reset to defaults
      </Button>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

{#if themeEditorOpen}
  <ThemeEditorModal onClose={() => (themeEditorOpen = false)} />
{/if}

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
</style>
