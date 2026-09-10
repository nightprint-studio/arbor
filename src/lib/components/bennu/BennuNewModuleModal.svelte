<script lang="ts">
  /**
   * BennuNewModuleModal — add a Maven module to the reactor.
   *
   * ## Why this is a dialog and not a "New Folder"
   *
   * A module is a directory **its parent knows about**. Making the folder, writing a pom and
   * stopping there produces something that looks entirely right on disk and builds for nobody: it
   * is in no reactor, so `mvn` never descends into it and no IDE lists it. The missing line is one
   * `<module>` in a file you were not looking at, which is exactly the kind of omission that
   * survives for weeks. So the parent is part of the form, not an afterthought — and picking it is
   * the first thing you do.
   *
   * ## Inherited fields show what they will inherit
   *
   * `groupId` and `version` are placeholders, not values: what you see is what the parent gives,
   * greyed, and leaving them alone writes **nothing** into the new pom. That is the correct pom in
   * nearly every case, and typing over the placeholder is how you say you meant otherwise. A module
   * that repeats its parent's version is the reason a release needs eleven files edited.
   *
   * ## The directory follows the name until you say otherwise
   *
   * Typing `portale-api` names the artifact and the folder. Editing the folder unlatches it, and it
   * stops following — because the two are the same thing right up until the one time they are not,
   * and a field that silently re-syncs after you edited it is worse than one that never did.
   *
   * Keyboard-first: the name field auto-focuses, Ctrl/Cmd+Enter creates from anywhere in the form,
   * Enter creates from a single-line field, Esc cancels (Modal owns it).
   */
  import { Boxes, TriangleAlert } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import FormField from '$lib/components/shared/ui/FormField.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { newModule, newModuleContext, type ModuleParent } from '$lib/ipc/bennu/scaffold';

  let {
    /** The directory the dialog was opened from — used only to preselect the nearest parent. */
    fromDir = null,
    onClose,
  }: { fromDir?: string | null; onClose: () => void } = $props();

  let parents = $state<ModuleParent[]>([]);
  let loading = $state(true);
  let parentPom = $state('');

  let artifactId = $state('');
  let dirName = $state('');
  /** Whether the folder is still following the name. Unlatched by the first edit of the folder. */
  let dirFollows = $state(true);
  let groupId = $state('');
  let version = $state('');
  let packaging = $state('jar');
  let displayName = $state('');
  let busy = $state(false);

  const PACKAGING = [
    { value: 'jar', label: 'jar — a library or a service' },
    { value: 'war', label: 'war — a web application' },
    { value: 'pom', label: 'pom — an aggregator for further modules' },
  ];

  const parent = $derived(parents.find((p) => p.pom === parentPom) ?? null);
  const folder = $derived((dirFollows ? artifactId : dirName).trim());

  $effect(() => {
    const root = projectStore.project?.root;
    if (!root) return;
    let cancelled = false;
    void newModuleContext(root)
      .then((ctx) => {
        if (cancelled) return;
        parents = ctx.parents;
        // The pom nearest the row you right-clicked, which is nearly always the one you meant —
        // falling back to the root, which is the answer for a project with one pom.
        const here = fromDir?.replace(/\\/g, '/') ?? '';
        const nearest = [...ctx.parents]
          .filter((p) => here === p.dir || here.startsWith(`${p.dir}/`))
          .sort((a, b) => b.dir.length - a.dir.length)[0];
        parentPom = (nearest ?? ctx.parents[0])?.pom ?? '';
        loading = false;
      })
      .catch((e) => {
        if (cancelled) return;
        toastStore.show(`Could not read the reactor: ${e}`, 'error');
        loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  const parentOptions = $derived(
    parents.map((p) => ({
      value: p.pom,
      label: p.relative ? `${p.artifact_id}  ·  ${p.relative}` : `${p.artifact_id}  ·  project root`,
      disabled: !!p.blocked,
    })),
  );

  /** The characters that are neither a Maven artifactId nor a folder name. */
  const NAME_OK = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;

  const problem = $derived.by(() => {
    if (!artifactId.trim()) return null; // nothing typed yet is not an error
    if (!NAME_OK.test(artifactId.trim()))
      return 'Letters, digits, - _ and . — and not starting with . or -';
    if (folder && !NAME_OK.test(folder)) return `“${folder}” can't be a folder name`;
    if (parent?.blocked) return parent.blocked;
    return null;
  });

  /** Where the module will land, project-relative — the line that makes the parent choice real. */
  const preview = $derived.by(() => {
    if (!parent || !folder) return '';
    return parent.relative ? `${parent.relative}/${folder}` : folder;
  });

  const canCreate = $derived(!!parent && !!artifactId.trim() && !!folder && !problem && !busy);

  async function create() {
    if (!canCreate || !parent) return;
    const root = projectStore.project?.root;
    if (!root) return;
    busy = true;
    try {
      const res = await newModule({
        root,
        parent_pom: parent.pom,
        artifact_id: artifactId.trim(),
        dir_name: folder,
        group_id: groupId.trim(),
        version: version.trim(),
        packaging,
        name: displayName.trim(),
      });
      // The tree first, then the reveal: the reveal searches the tree, so a row that has not
      // arrived yet is a row it reports as missing.
      await projectStore.refreshTree();
      bennuUiStore.focusInTree(res.module_dir);
      await projectStore.openFile(res.pom);
      toastStore.show(
        res.parent_became_aggregator
          ? `Created ${artifactId.trim()} — ${parent.artifact_id} is now <packaging>pom</packaging>`
          : `Created ${artifactId.trim()}`,
        'success',
      );
      onClose();
    } catch (e) {
      toastStore.show(e instanceof Error ? e.message : String(e), 'error');
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void create();
    }
  }
</script>

<Modal {onClose} width="560px" height="auto" padBody={false} ariaLabel="New module">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Boxes size={14} />
      <span class="modal-title">New Module</span>
    </ModalHeader>
  {/snippet}

  <div class="nm" onkeydown={onKey} role="presentation">
    {#if loading}
      <div class="nm-loading">Reading the reactor…</div>
    {:else if parents.length === 0}
      <Alert variant="warning" compact title="No pom to add a module to">
        This project has no Maven reactor — a module needs a parent pom to be listed in.
      </Alert>
    {:else}
      <FormField
        label="Parent"
        hint={parent && !parent.aggregator
          ? `${parent.artifact_id} will become <packaging>pom</packaging> so it can list modules.`
          : 'The pom that will list this module — and whose groupId and version it inherits.'}
      >
        <Select bind:value={parentPom} options={parentOptions} />
      </FormField>

      <FormField label="Name" required error={problem}>
        <Input bind:value={artifactId} placeholder="portale-api" autofocus />
      </FormField>

      <div class="nm-row">
        <FormField label="Folder" hint="Follows the name until you change it.">
          <Input
            value={dirFollows ? artifactId : dirName}
            placeholder={artifactId || 'portale-api'}
            oninput={(v) => {
              dirFollows = false;
              dirName = v;
            }}
          />
        </FormField>
        <FormField label="Packaging">
          <Select bind:value={packaging} options={PACKAGING} />
        </FormField>
      </div>

      <div class="nm-row">
        <FormField label="Group" optionalText="inherited">
          <Input bind:value={groupId} placeholder={parent?.group_id ?? ''} />
        </FormField>
        <FormField label="Version" optionalText="inherited">
          <Input bind:value={version} placeholder={parent?.version ?? ''} />
        </FormField>
      </div>

      <FormField label="Display name" optionalText="optional">
        <Input bind:value={displayName} placeholder="Portale API" />
      </FormField>

      <!-- One line, always present: a preview that appears and disappears makes the dialog jump. -->
      <div class="nm-note">
        {#if parent?.blocked}
          <span class="nm-bad"><TriangleAlert size={12} /> {parent.blocked}</span>
        {:else if preview}
          <span class="nm-preview"><code>{preview}/pom.xml</code></span>
        {:else}
          <span class="nm-hint">The folder, its pom and its source roots — and one line in the parent.</span>
        {/if}
      </div>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!canCreate}
        loading={busy}
        tooltip={{ content: 'Create the module', shortcut: 'Ctrl+Enter' }}
        onclick={() => void create()}
      >Create</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .nm { display: flex; flex-direction: column; gap: 10px; padding: 12px; min-height: 0; }
  .nm-row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .nm-loading { color: var(--text-disabled); font-size: 12px; padding: 6px 0; }
  .nm-note { min-height: 16px; font-size: 11px; display: flex; align-items: center; gap: 5px; }
  .nm-bad { color: var(--error); display: flex; align-items: center; gap: 4px; }
  .nm-hint { color: var(--text-disabled); }
  .nm-preview { color: var(--info); }
  .nm-preview code { font-family: var(--font-code); color: var(--text-secondary); }
</style>
