<script lang="ts">
  /**
   * Settings › Language Servers — the catalogue, what is running, and how to add a language.
   *
   * Two lists, because they answer different questions: what Bennu *can* run on this machine (with
   * an install hint for each one it cannot find), and what is running *right now* for the open
   * projects. A page that showed only the first would never explain a server that is installed and
   * crashing.
   */
  import { Braces, CircleCheck, Download, Package, RefreshCw, ServerCog, TriangleAlert } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import type { LspConfigDto } from '$lib/ipc/bennu/config';

  /** The `[lsp]` section, with a complete default so a config written before it existed reads as
   *  "on, nothing disabled" rather than as a half-populated object. */
  const lspCfg = $derived<LspConfigDto>(
    bennuConfigStore.cfg?.lsp ?? {
      enabled: true,
      rust_check_command: 'check',
      disabled: [],
      server_paths: {},
      servers: [],
      background_idle_timeout_secs: 600,
    },
  );
  const lspEnabled = $derived(lspCfg.enabled);
  const lspDisabled = $derived(lspCfg.disabled ?? []);
  const serverPaths = $derived(lspCfg.server_paths ?? {});
  // `?? 600` and not `|| 600`: zero is a real choice here — never reclaim — and `||` would read it
  // as absent and silently put ten minutes back.
  const backgroundIdle = $derived(String(lspCfg.background_idle_timeout_secs ?? 600));

  async function patchLsp(patch: Partial<LspConfigDto>, restart = true) {
    await bennuConfigStore.patch({ lsp: { ...lspCfg, ...patch } });
    if (restart) await bennuLspStore.reloadServers();
  }

  async function toggleServer(id: string, on: boolean) {
    const next = on ? lspDisabled.filter((d) => d !== id) : [...new Set([...lspDisabled, id])];
    await patchLsp({ disabled: next });
  }

  /**
   * Install a language server from its own package manager, streaming into the Build panel.
   *
   * The toast is where the outcome lands rather than an inline banner: the interesting part of a
   * `cargo install --git` is the three minutes of log, which is already on screen in the panel, and
   * what is left to say afterwards is one sentence.
   */
  async function installServer(id: string) {
    const res = await bennuLspStore.install(id);
    toastStore.show(res.message, res.ok ? 'success' : 'error', res.ok ? 5000 : 9000);
  }

  async function commitServerPath(id: string, path: string) {
    const next = { ...serverPaths };
    const trimmed = path.trim();
    if (trimmed) next[id] = trimmed;
    else delete next[id];
    await patchLsp({ server_paths: next });
  }
</script>

<div class="section-header">
  <h2>Language Servers</h2>
  <p>
    Bennu's Java intelligence is its own engine. Every other language — Rust first — is served by an
    external <strong>language server</strong>: it supplies completion, go-to, find-usages,
    diagnostics, rename, formatting and the semantic colouring.
  </p>
</div>

<div class="card">
  <div class="card-section-title"><ServerCog size={12} /> General</div>
  <FormRow
    label="Enable language servers"
    description="A server only starts for a project whose root carries the matching manifest (a Cargo.toml for Rust) and whose binary is installed — so leaving this on costs nothing when neither is true."
  >
    <Toggle checked={lspEnabled} onchange={(v) => void patchLsp({ enabled: v })} ariaLabel="Enable language servers" />
  </FormRow>
  <!-- Only meaningful while servers can start at all. -->
  {#if lspEnabled}
    <FormRow
      label="Stop unattended servers after"
      description="A language server can be started by an AI client asking about a project you do not have open. Nothing on screen is using it, and rust-analyzer holds most of a gigabyte, so it is stopped once it goes quiet — this is how long that takes. A server for a project you have open is never stopped. Applies to servers already running."
    >
      <Select
        value={backgroundIdle}
        options={[
          { value: '300', label: '5 minutes' },
          { value: '600', label: '10 minutes' },
          { value: '1800', label: '30 minutes' },
          { value: '3600', label: '1 hour' },
          { value: '0', label: 'Never — keep them running' },
        ]}
        onchange={(v) => void patchLsp({ background_idle_timeout_secs: Number(v) }, false)}
      />
    </FormRow>
  {/if}
</div>

<!-- What is running RIGHT NOW. Separate from the catalogue below because a server can be installed
     and still be failing, and only this list can say so. -->
{#if bennuLspStore.statuses.length}
  <div class="card">
    <div class="card-section-title"><RefreshCw size={12} /> Running</div>
    {#each bennuLspStore.statuses as st (st.root + st.language)}
      <div class="lsp-run">
        <div class="lsp-run-main">
          <div class="lsp-run-head">
            <span class="lsp-name">{st.version ?? st.name}</span>
            <Badge
              variant="tone"
              tone={st.state === 'ready' ? 'success' : st.state === 'starting' ? 'info' : 'error'}
            >{st.state}</Badge>
            {#if st.progress}<span class="lsp-progress">{st.progress}</span>{/if}
          </div>
          <div class="lsp-run-sub">{st.root}</div>
          {#if st.message}
            <div class="lsp-run-msg">{st.message}</div>
          {/if}
          {#if st.state !== 'ready' && st.log_tail.length}
            <!-- The server's own stderr. Usually the only place a refusal to start explains
                 itself, so it is shown rather than kept in a log nobody opens. -->
            <pre class="lsp-log">{st.log_tail.slice(-8).join('\n')}</pre>
          {/if}
        </div>
        <div class="lsp-run-actions">
          <Button size="sm" variant="ghost" onclick={() => void bennuLspStore.restart(st.root, st.language)}>Restart</Button>
          {#if st.state === 'ready'}
            <Button size="sm" variant="ghost" onclick={() => void bennuLspStore.stop(st.root, st.language)}>Stop</Button>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<div class="card">
  <div class="card-section-title"><Package size={12} /> Installed servers</div>
  {#if !bennuLspStore.servers.length}
    <EmptyState
      message="No servers in the catalogue"
      description="Bennu could not read the language-server list from the backend."
    />
  {:else}
    {#each bennuLspStore.servers as srv (srv.id)}
      <div class="lsp-srv">
        <div class="lsp-srv-head">
          {#if srv.path}
            <CircleCheck size={13} class="lsp-ok" />
          {:else}
            <TriangleAlert size={13} class="lsp-warn" />
          {/if}
          <span class="lsp-name">{srv.name}</span>
          <span class="lsp-exts">{srv.extensions.map((e) => `.${e}`).join(' ')}</span>
          {#if srv.custom}<Badge variant="tone" tone="info">custom</Badge>{/if}
          <span class="lsp-spacer"></span>
          <Toggle
            checked={srv.enabled}
            disabled={!lspEnabled}
            onchange={(v) => void toggleServer(srv.id, v)}
            ariaLabel={`Enable ${srv.name}`}
          />
        </div>
        {#if srv.path}
          <div class="lsp-srv-path" use:tooltip={srv.path}>{srv.path}</div>
        {:else}
          <!-- Not "not found" full stop: the hint is what turns a dead end into a next step, which
               is the whole reason the catalogue carries one. The command below is shown whether or
               not there is a button — a user who prefers their own terminal needs it, and a failed
               install leaves them with exactly that. -->
          <div class="lsp-srv-hint">
            <code>{srv.command}</code> was not found. {srv.install_hint}
          </div>
          {#if srv.install?.length}
            <div class="lsp-install">
              <Button
                size="sm"
                variant="primary"
                disabled={bennuLspStore.installing !== null}
                loading={bennuLspStore.installing === srv.id}
                onclick={() => void installServer(srv.id)}
              >
                {#snippet iconStart()}<Download size={13} />{/snippet}
                {bennuLspStore.installing === srv.id ? 'Installing…' : 'Install'}
              </Button>
              <code class="lsp-install-cmd" use:tooltip={srv.install.join(' ')}>{srv.install.join(' ')}</code>
              <CopyButton value={srv.install.join(' ')} title="Copy the install command" />
            </div>
          {/if}
        {/if}
        <label class="lsp-override">
          <span>Executable path</span>
          <Input
            value={serverPaths[srv.id] ?? ''}
            placeholder="leave empty to search PATH and the usual install locations"
            onchange={(v) => void commitServerPath(srv.id, v)}
          />
        </label>
      </div>
    {/each}
  {/if}
</div>

<div class="card">
  <div class="card-section-title"><Braces size={12} /> Adding a language</div>
  <p class="lsp-note">
    A language the catalogue does not cover is added in <code>bennu/config.toml</code> under
    <code>[[lsp.servers]]</code> — the same fields the built-in entries carry, so it gets the same
    features with no code change:
  </p>
  <pre class="lsp-sample">{`[[lsp.servers]]
id = "zls"
name = "Zig"
language = "zig"
command = "zls"
extensions = ["zig", "zon"]
root_markers = ["build.zig"]
initialization_options = ""`}</pre>
  <p class="lsp-note">
    <strong>root_markers</strong> is the gate: without one of those files above the file being
    edited there is no workspace to open, so nothing starts. An entry whose <code>id</code> matches
    a built-in replaces it.
  </p>
</div>

<style>
  .lsp-run, .lsp-srv {
    display: flex; flex-direction: column; gap: 6px;
    padding: 9px 10px; background: var(--bg-base);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-md);
  }
  .lsp-run { flex-direction: row; align-items: flex-start; gap: 10px; }
  .lsp-run + .lsp-run, .lsp-srv + .lsp-srv { margin-top: 6px; }
  .lsp-run-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .lsp-run-head, .lsp-srv-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .lsp-run-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
  .lsp-name { font-size: var(--font-size-sm); font-weight: 600; color: var(--text-primary); }
  .lsp-exts { font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }
  .lsp-spacer { flex: 1; }
  .lsp-progress { font-size: var(--font-size-xs); color: var(--accent); }
  .lsp-run-sub, .lsp-srv-path {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lsp-run-msg { font-size: var(--font-size-xs); color: var(--warning); line-height: 1.4; }
  .lsp-install { display: flex; align-items: center; gap: 10px; margin-top: 6px; }
  .lsp-install-cmd {
    font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-faint);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .lsp-srv-hint { font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.45; }
  .lsp-srv-hint code, .lsp-note code { font-family: var(--font-code); color: var(--text-primary); }
  .lsp-log, .lsp-sample {
    margin: 0; padding: 7px 9px; max-height: 140px; overflow: auto;
    font-family: var(--font-code); font-size: var(--font-size-2xs); line-height: 1.5;
    color: var(--text-muted); background: var(--bg-elevated);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
    white-space: pre;
  }
  .lsp-override { display: flex; align-items: center; gap: 8px; }
  .lsp-override > span { font-size: var(--font-size-xs); color: var(--text-muted); flex-shrink: 0; }
  .lsp-override :global(.input-wrap) { flex: 1; }
  .lsp-note {
    margin: 0; padding: 2px 2px 6px;
    font-size: var(--font-size-xs); color: var(--text-muted); line-height: 1.5;
  }
  :global(.lsp-ok) { color: var(--success); flex-shrink: 0; }
  :global(.lsp-warn) { color: var(--warning); flex-shrink: 0; }
</style>
