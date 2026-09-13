<script lang="ts">
  /**
   * Project Configuration — everything that is true of **this project** rather than of you.
   *
   * ## Why it looks like Settings now
   *
   * It is the same `SettingsShell`, the same card vocabulary, the same colours. The two dialogs
   * answer neighbouring questions — "how do I work" and "what is this project" — and somebody who
   * has just closed one of them should recognise the other on sight instead of learning a second
   * layout. It also ends the shape this modal had grown into: one column that scrolled through six
   * unrelated subjects, with the naming grid two thirds of the way down where nobody found it.
   *
   * ## The line between here and Settings
   *
   * Settings is you and this machine; this is the project. Where a setting genuinely has both — a
   * naming convention, a dictionary — the profile holds your answer and the project states what it
   * says differently, and the project page marks each value with where it came from.
   *
   * ## Who writes what
   *
   * Every section writes as you go, like Settings does, **except naming**: that one is a document
   * rather than a row of switches, and writing it per keystroke would have the backend re-reading
   * the conventions of every open project while one is still being chosen. So it carries its own
   * Save, and the footer is a Done.
   *
   * Keyboard-first: `Esc` closes, the nav is a tree walked with Tab and the arrow keys, every
   * control inside a section is reachable in reading order.
   */
  import { untrack } from 'svelte';
  import { SlidersHorizontal, FolderCog, CaseSensitive, SpellCheck, Boxes, ListFilter } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import SettingsShell, { type SettingsNavGroup } from '$lib/components/shared/ui/SettingsShell.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import BennuProjectFacts from './project-config/BennuProjectFacts.svelte';
  import BennuProjectFrameworks from './project-config/BennuProjectFrameworks.svelte';
  import BennuProjectInspections from './project-config/BennuProjectInspections.svelte';
  import BennuProjectSpelling from './project-config/BennuProjectSpelling.svelte';
  import BennuNamingSettings from './BennuNamingSettings.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuNamingStore } from '$lib/stores/bennu/naming.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';

  let { onClose }: { onClose: () => void } = $props();

  const project = $derived(projectStore.project);
  const root = $derived(project?.root ?? null);

  let active = $state('project');

  const naming = $derived(bennuNamingStore.project);

  const groups = $derived<SettingsNavGroup[]>([
    {
      label: 'This project',
      items: [
        { id: 'project', label: 'Project', icon: FolderCog, iconColor: 'var(--accent)' },
        { id: 'frameworks', label: 'Frameworks', icon: Boxes, iconColor: 'var(--success)' },
        // Per project and with no profile level, unlike the two below: "is an unused import worth
        // a warning" is a question about this codebase's state, not about you.
        { id: 'inspections', label: 'Inspections', icon: ListFilter, iconColor: 'var(--error)' },
      ],
    },
    {
      // Both have a counterpart in Settings, and the group label is what says so — a project page
      // for something that exists at two levels is not the same kind of page as one that does not.
      label: 'Overrides your profile',
      items: [
        { id: 'naming', label: 'Naming', icon: CaseSensitive, iconColor: 'var(--warning)' },
        { id: 'spelling', label: 'Spelling', icon: SpellCheck, iconColor: 'var(--info)' },
      ],
    },
  ]);

  // The config (for the JDK / encoding overrides) and both naming documents. Untracked because
  // each call reads the state it then writes, and depending on that would re-run this per answer.
  $effect(() => {
    const r = root;
    untrack(() => {
      void bennuConfigStore.load();
      void bennuNamingStore.loadCatalog(r);
      void bennuNamingStore.profile.load();
      if (r) void bennuNamingStore.load(r);
    });
  });

  async function saveNaming() {
    if (!(await naming.apply())) {
      toastStore.show("Couldn't save the naming conventions", 'error');
    }
  }
</script>

<Modal {onClose} width="980px" height="660px" padBody={false} ariaLabel="Bennu Project Configuration">
  {#snippet header()}
    <ModalHeader {onClose}>
      <SlidersHorizontal size={14} />
      <span class="modal-title">Project Configuration</span>
      {#if project}<span class="hdr-name">{project.name}</span>{/if}
    </ModalHeader>
  {/snippet}

  <SettingsShell {groups} bind:active searchPlaceholder="Search this project’s settings…">
    {#snippet content()}
      {#if !project}
        <EmptyState message="Open a project to configure it." />
      {:else if active === 'project'}
        <BennuProjectFacts />
      {:else if active === 'frameworks'}
        <BennuProjectFrameworks />
      {:else if active === 'inspections'}
        <BennuProjectInspections />
      {:else if active === 'spelling'}
        <BennuProjectSpelling />
      {:else if active === 'naming'}
        <div class="section-header">
          <h2>Naming</h2>
          <p>
            What this project spells differently. Everything it does not state comes from
            <strong>Settings › Editor › Naming</strong>, and each row says which of the two it is
            showing.
          </p>
        </div>
        <BennuNamingSettings
          doc={naming}
          inherited={bennuNamingStore.profile.config}
          inheritedLabel="your profile"
        />
        <div class="actions">
          <Button variant="ghost" size="sm" onclick={() => naming.revert()} disabled={!naming.dirty}>
            Discard changes
          </Button>
          <Button
            variant="primary"
            size="sm"
            onclick={saveNaming}
            disabled={!naming.dirty || naming.saving}
          >
            {naming.saving ? 'Saving…' : 'Save for this project'}
          </Button>
        </div>
      {/if}
    {/snippet}
  </SettingsShell>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="foot-note">
        {#if root}Stored in <code>{root}/.arbor/bennu/</code> and in your profile{/if}
      </span>
      <Button variant="primary" size="sm" onclick={onClose}>Done</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .modal-title { font-size: var(--font-size-md); font-weight: 600; color: var(--text-primary); }
  .hdr-name {
    font-size: var(--font-size-xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .foot-note {
    font-size: var(--font-size-2xs); color: var(--text-muted);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
  }
  .foot-note code { font-family: var(--font-code); }
  .actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
</style>
