<script lang="ts">
  /**
   * Settings › Java › JDK locations — where Bennu looks for a JDK on **this machine**.
   *
   * A machine-level list, which is why it is here and not in Project Configuration: the level a
   * project targets is the project's, the directories a JDK is installed in are the laptop's.
   */
  import { FolderOpen, Plus, Trash2 } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import FileExplorerModal from '$lib/components/sitta/FileExplorerModal.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuDiagnosticsStore } from '$lib/stores/bennu/diagnostics.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';

  let pickerOpen = $state(false);

  const paths = $derived(bennuConfigStore.cfg?.jdk_paths ?? []);

  async function commit(next: string[]) {
    await bennuConfigStore.patch({ jdk_paths: next });
    // Re-fetch the JDK status so the title bar, Problems and Project Configuration reflect it.
    const root = projectStore.project?.root;
    if (root) void bennuDiagnosticsStore.refresh(root);
  }
</script>

<div class="section-header">
  <h2>JDK locations</h2>
  <p>Where a JDK is looked for on this machine. Which one a project uses is in <strong>Project Configuration</strong>.</p>
</div>
<div class="card">
  <div class="card-section-title"><FolderOpen size={12} /> Search paths</div>
  <p class="set-hint">
    Extra JDK install directories, searched before <code>JAVA_HOME</code> and the standard install
    roots — for a JDK installed somewhere non-standard. On macOS either the <code>.jdk</code> bundle
    or the <code>Contents/Home</code> inside it works.
  </p>
  {#if paths.length}
    <div class="set-list">
      {#each paths as p (p)}
        <div class="set-list-row">
          <span class="set-list-text" use:tooltip={p}>{p}</span>
          <button class="set-list-del" type="button" onclick={() => void commit(paths.filter((x) => x !== p))} aria-label="Remove JDK path">
            <Trash2 size={13} />
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <p class="set-empty">No extra paths — only JAVA_HOME and the standard roots are searched.</p>
  {/if}
  <div class="set-list-add">
    <Button variant="ghost" size="sm" onclick={() => (pickerOpen = true)}>
      {#snippet iconStart()}<Plus size={13} />{/snippet}
      Add JDK directory…
    </Button>
  </div>
</div>

{#if pickerOpen}
  <FileExplorerModal
    mode="folder"
    title="Select a JDK install directory"
    onConfirm={(dir: string) => {
      pickerOpen = false;
      if (!paths.includes(dir)) void commit([...paths, dir]);
    }}
    onCancel={() => (pickerOpen = false)}
    onClose={() => (pickerOpen = false)}
  />
{/if}
