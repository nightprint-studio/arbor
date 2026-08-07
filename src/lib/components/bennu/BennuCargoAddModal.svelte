<script lang="ts">
  /**
   * Add a dependency — by name, with a version and features picked from crates.io.
   *
   * ## Why there is no search box
   *
   * The sparse index Cargo itself uses serves one file per crate and has **no search**: you have to
   * know the name. Searching is a separate crates.io API with its own rate limits and etiquette, and it
   * is not needed to answer the question this dialog is for — you came here because you know you want
   * `serde`. So the name is typed, and everything downstream of it (the versions, the features) is
   * looked up as soon as the name stops changing.
   *
   * ## Cargo does the writing
   *
   * The button runs the real `cargo add`. It resolves the requirement the way Cargo would write it,
   * honours `[workspace.dependencies]` inheritance, validates the features against the crate it just
   * resolved and formats the entry in the file's own style. Editing the manifest here instead would be
   * reimplementing Cargo's opinion about a file Cargo owns — and getting it subtly wrong.
   *
   * ## Keyboard
   *
   * The name field auto-focuses; Enter adds; Esc cancels. The version list is a `<select>` so it is
   * reachable and typeable without the mouse.
   */
  import { PackagePlus, RefreshCw } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { projectStore } from '$lib/stores/bennu/project.svelte';
  import { bennuCargoStore } from '$lib/stores/bennu/cargo.svelte';
  import { cargoAdd, cargoVersions, type CrateRelease } from '$lib/ipc/bennu/cargo';
  import { lspReloadWorkspace } from '$lib/ipc/bennu/lsp';

  let {
    /** The workspace root — where cargo runs. */
    root,
    /** Pre-select a member to add to (`-p`), for "add to this crate" from a crate section. */
    initialPackage = '',
    onClose,
  }: { root: string; initialPackage?: string; onClose: () => void } = $props();

  let name = $state('');
  // svelte-ignore state_referenced_locally
  let pkg = $state(initialPackage);
  let version = $state('');
  let kind = $state('');
  let optional = $state(false);
  let noDefaultFeatures = $state(false);
  const features = new Set<string>();
  let chosenFeatures = $state<string[]>([]);
  let busy = $state(false);
  let result = $state<{ ok: boolean; output: string } | null>(null);

  /** The version list for the typed name — `null` while it has not been asked for. */
  let releases = $state<CrateRelease[] | null>(null);
  let looking = $state(false);

  const trimmed = $derived(name.trim());
  const members = $derived(bennuCargoStore.workspace?.crates ?? []);

  /**
   * Look the crate up when the name settles.
   *
   * Debounced, and the answer is dropped when the name has moved on — the list belongs to a name, and
   * showing `tokio`'s versions under `toml` would be worse than showing none. An empty answer is not
   * an error: it means the crate is unknown, the index is unreachable with nothing cached, or the user
   * has turned crates.io off. In every one of those cases the crate can still be added by name and
   * cargo will resolve the version.
   */
  $effect(() => {
    const target = trimmed;
    if (!target) {
      releases = null;
      return;
    }
    let cancelled = false;
    looking = true;
    const t = setTimeout(() => {
      void cargoVersions(target)
        .then((found) => {
          if (cancelled) return;
          releases = found;
          // Default to the newest real release rather than to the top row, which may be a
          // pre-release — the same rule the version hints use.
          const newest = found.find((r) => !r.prerelease && !r.yanked);
          version = newest?.version ?? '';
        })
        .catch(() => { if (!cancelled) releases = null; })
        .finally(() => { if (!cancelled) looking = false; });
    }, 350);
    return () => { cancelled = true; looking = false; clearTimeout(t); };
  });

  /** The features the **chosen version** declares — the index carries them on the same line as the
   *  version, so this costs no extra request. Per version rather than per crate: offering today's
   *  feature list while adding an old release would offer features it does not have. */
  const availableFeatures = $derived(
    (releases ?? []).find((r) => r.version === version)?.features ?? [],
  );

  const versionOptions = $derived(
    (releases ?? []).map((r) => ({
      value: r.version,
      label: r.yanked
        ? `${r.version} — yanked`
        : r.prerelease
          ? `${r.version} — pre-release`
          : r.version,
    })),
  );

  const KINDS = [
    { value: '', label: 'dependencies' },
    { value: 'dev', label: 'dev-dependencies' },
    { value: 'build', label: 'build-dependencies' },
  ];

  function toggleFeature(feature: string, on: boolean) {
    if (on) features.add(feature);
    else features.delete(feature);
    chosenFeatures = [...features];
  }

  async function add() {
    if (!trimmed || busy) return;
    busy = true;
    result = null;
    try {
      const res = await cargoAdd(root, trimmed, {
        version,
        features: chosenFeatures,
        noDefaultFeatures,
        kind,
        optional,
        packageName: pkg,
      });
      result = { ok: res.ok, output: res.output };
      if (!res.ok) return;
      toastStore.show(`Added ${trimmed}${version ? ` ${version}` : ''}`, 'success');
      // The manifest and the lockfile just changed on disk behind every open buffer, so: re-read the
      // crate graph the panel draws, adopt the manifest's new text if a tab is showing it, and tell
      // the language server to re-resolve — this is the one place Bennu itself changes a manifest, so
      // it is the one place a reload is certainly warranted rather than merely possible.
      await bennuCargoStore.load(root, true);
      await projectStore.reload(`${root}/Cargo.toml`);
      if (pkg) {
        const member = members.find((c) => c.name === pkg);
        if (member) await projectStore.reload(member.manifest);
      }
      void lspReloadWorkspace(root);
      onClose();
    } catch (e) {
      result = { ok: false, output: String(e instanceof Error ? e.message : e) };
    } finally {
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); void add(); }
  }

  let nameEl = $state<HTMLInputElement | undefined>();
  let focused = false;
  $effect(() => {
    if (focused || !nameEl) return;
    focused = true;
    nameEl.focus();
  });
</script>

<Modal {onClose} width="560px" height="auto" padBody={false} ariaLabel="Add a dependency">
  {#snippet header()}
    <ModalHeader {onClose}>
      <PackagePlus size={14} />
      <span class="modal-title">Add dependency</span>
    </ModalHeader>
  {/snippet}

  <div class="ca" onkeydown={onKey} role="presentation">
    <div class="ca-name">
      <input
        class="ca-input"
        bind:this={nameEl}
        bind:value={name}
        placeholder="Crate name"
        spellcheck="false"
        autocomplete="off"
        aria-label="Crate name"
        data-modal-autofocus
      />
      {#if looking}<Spinner size={12} />{/if}
    </div>

    <FormRow label="Version">
      {#if versionOptions.length > 0}
        <Select
          value={version}
          options={versionOptions}
          onchange={(v) => (version = v)}
          ariaLabel="Version"
        />
      {:else}
        <!-- No list to pick from: the crate is unknown, the index is unreachable, or crates.io is
             turned off. Typing a requirement still works, and leaving it empty lets cargo choose —
             which is exactly what a bare `cargo add serde` does. -->
        <input
          class="ca-input ca-input-sm"
          bind:value={version}
          placeholder={trimmed ? 'Latest (cargo decides)' : ''}
          spellcheck="false"
          autocomplete="off"
          aria-label="Version"
        />
      {/if}
    </FormRow>

    <FormRow label="Table">
      <Select value={kind} options={KINDS} onchange={(v) => (kind = v)} ariaLabel="Table" />
    </FormRow>

    {#if members.length > 1}
      <FormRow label="Add to">
        <Select
          value={pkg}
          options={[{ value: '', label: 'the root manifest' },
                    ...members.map((c) => ({ value: c.name, label: c.name }))]}
          onchange={(v) => (pkg = v)}
          ariaLabel="Which crate"
        />
      </FormRow>
    {/if}

    {#if availableFeatures.length > 0}
      <FormRow label="Features">
        <div class="ca-features">
          {#each availableFeatures as f (f)}
            <label class="ca-feature">
              <input
                type="checkbox"
                checked={chosenFeatures.includes(f)}
                onchange={(e) => toggleFeature(f, e.currentTarget.checked)}
              />
              <span>{f}</span>
            </label>
          {/each}
        </div>
      </FormRow>
    {/if}

    <FormRow label="Optional">
      <Toggle bind:checked={optional} ariaLabel="Optional dependency" />
    </FormRow>
    <FormRow label="No default features">
      <Toggle bind:checked={noDefaultFeatures} ariaLabel="Disable default features" />
    </FormRow>

    {#if result && !result.ok}
      <!-- Cargo's own words, not a summary of them: when `cargo add` refuses it says exactly why
           (no such crate, no matching version, an unknown feature), and that text is the fix. -->
      <Alert variant="error" compact>
        <pre class="ca-out">{result.output}</pre>
      </Alert>
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter align="end">
      <Button variant="ghost" size="sm" onclick={onClose}>Cancel</Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!trimmed || busy}
        loading={busy}
        tooltip={{ content: 'Run cargo add', shortcut: 'Enter' }}
        onclick={() => void add()}
      >
        {#snippet iconStart()}<RefreshCw size={13} />{/snippet}
        Add
      </Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  .ca { display: flex; flex-direction: column; gap: 8px; padding: 10px; min-height: 0; }
  .ca-name { display: flex; align-items: center; gap: 8px; }
  .ca-input {
    flex: 1;
    padding: 6px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: calc(var(--font-size-md) * 1.05);
  }
  .ca-input:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-subtle); }
  .ca-input-sm { font-size: var(--font-size-md); }
  .ca-features { display: flex; flex-wrap: wrap; gap: 6px 10px; }
  .ca-feature { display: flex; align-items: center; gap: 4px; font-size: 11px; }
  .ca-out {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 10.5px;
    white-space: pre-wrap;
    max-height: 120px;
    overflow: auto;
  }
</style>
