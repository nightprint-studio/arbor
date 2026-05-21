<script lang="ts">
  /**
   * PluginExportTemplateModal — advanced "scaffold a plugin" form.
   *
   * Walks the user through Identity → Permissions → Hooks → Recipes via the
   * shared `<Tabs>` widget; on submit hits `export_plugin_template` and pipes
   * the resulting zip bytes to a save dialog. All fields are optional with
   * sensible defaults, so the modal is also a one-click shortcut to "give me a
   * starter zip".
   *
   * The recipe checkboxes inject canonical Lua snippets (command palette
   * entry, keybinding, settings panel, modal form, toolbar action, sidebar,
   * notifications, background job, scheduler, HTTP) into main.lua. The
   * snippet text itself lives in src-tauri/templates/plugin/recipes/*.lua and
   * is bundled at compile time — see plugin_template_commands.rs.
   */
  import {
    FileCheck, Lock, Zap, Sparkles, Download, ListChecks,
    Tag, GitBranch, HardDrive, TerminalSquare, Globe, FolderTree,
    BookOpen, Settings, MousePointer, Bell, Play, Clock, MessageSquare,
    KeyRound, LayoutDashboard, Workflow, Network as NetworkIcon,
  } from 'lucide-svelte';

  import Modal            from '$lib/components/shared/Modal.svelte';
  import ModalHeader      from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter      from '$lib/components/shared/ModalFooter.svelte';
  import FilePickerModal  from '$lib/components/shared/FilePickerModal.svelte';
  import Tabs             from '$lib/components/shared/ui/Tabs.svelte';
  import Toggle           from '$lib/components/shared/ui/Toggle.svelte';
  import Input            from '$lib/components/shared/ui/Input.svelte';
  import Select           from '$lib/components/shared/ui/Select.svelte';
  import Button           from '$lib/components/shared/ui/Button.svelte';
  import Badge            from '$lib/components/shared/ui/Badge.svelte';
  import Card             from '$lib/components/shared/ui/Card.svelte';
  import Alert            from '$lib/components/shared/ui/Alert.svelte';

  import { uiStore } from '$lib/stores/ui.svelte';
  import {
    exportPluginTemplateToPath,
    type ExportPluginTemplateOpts,
  } from '$lib/ipc/plugin';

  let { onClose, onExported }: {
    onClose:     () => void;
    onExported?: (slug: string) => void;
  } = $props();

  // ── Form state — every field has a default so omitting a tab still works.
  let opts = $state<ExportPluginTemplateOpts>({
    name:        'my-plugin',
    version:     '0.1.0',
    description: 'A new Arbor plugin.',
    author:      '',
    license:     '',
    repository:  '',
    keywords:    [],

    fs:                   'none',
    fs_scope:             [],
    git:                  'none',
    terminal:             'none',
    terminal_scope:       [],
    network:              [],
    env_read:             false,
    issues:               'none',
    toolchain:            'none',
    service_export:       false,
    service_call:         false,
    settings_read_others: false,

    hook_on_plugin_load:  true,
    hook_on_repo_open:    false,
    hook_on_repo_close:   false,
    hook_on_tab_switch:   false,
    hook_on_commit:       false,
    hook_on_push:         false,
    hook_on_pull:         false,
    hook_on_fetch:        false,
    hook_on_checkout:     false,
    hook_on_branch_create: false,
    hook_on_branch_delete: false,
    hook_on_mr_opened:    false,
    hook_on_mr_merged:    false,

    include_scheduler: false,

    snippet_command:        true,
    snippet_keybinding:     false,
    snippet_settings_panel: false,
    snippet_modal:          false,
    snippet_action_toolbar: false,
    snippet_sidebar:        false,
    snippet_notification:   false,
    snippet_job_spawn:      false,
    snippet_scheduler:      false,
    snippet_http_get:       false,
  });

  // CSV-style helpers for list inputs (keywords / fs_scope / network / terminal_scope).
  // Toggling between CSV strings and string[] arrays is easier UX than a custom chip editor.
  let keywordsCsv       = $state('');
  let fsScopeCsv        = $state('');
  let terminalScopeCsv  = $state('');
  let networkCsv        = $state('');
  function csvToArr(s: string): string[] {
    return s.split(',').map(t => t.trim()).filter(Boolean);
  }

  // ── Tabs --------------------------------------------------------------
  type TabId = 'identity' | 'permissions' | 'hooks' | 'recipes';
  let activeTab = $state<TabId>('identity');

  const permissionCount = $derived(
    [
      opts.fs !== 'none',
      opts.git !== 'none',
      opts.terminal !== 'none',
      opts.issues !== 'none',
      opts.toolchain !== 'none',
      opts.network.length > 0,
      opts.env_read,
      opts.service_export,
      opts.service_call,
      opts.settings_read_others,
    ].filter(Boolean).length,
  );

  const hookCount = $derived(
    [
      opts.hook_on_plugin_load, opts.hook_on_repo_open, opts.hook_on_repo_close,
      opts.hook_on_tab_switch,  opts.hook_on_commit,    opts.hook_on_push,
      opts.hook_on_pull,        opts.hook_on_fetch,     opts.hook_on_checkout,
      opts.hook_on_branch_create, opts.hook_on_branch_delete,
      opts.hook_on_mr_opened, opts.hook_on_mr_merged,
    ].filter(Boolean).length,
  );

  const snippetCount = $derived(
    [
      opts.snippet_command, opts.snippet_keybinding, opts.snippet_settings_panel,
      opts.snippet_modal, opts.snippet_action_toolbar, opts.snippet_sidebar,
      opts.snippet_notification, opts.snippet_job_spawn, opts.snippet_scheduler,
      opts.snippet_http_get,
    ].filter(Boolean).length,
  );

  const tabItems = $derived([
    { id: 'identity',    label: 'Identity',    icon: FileCheck                                                  },
    { id: 'permissions', label: 'Permissions', icon: Lock,     badge: permissionCount > 0 ? permissionCount : '' },
    { id: 'hooks',       label: 'Hooks',       icon: Zap,      badge: hookCount       > 0 ? hookCount       : '' },
    { id: 'recipes',     label: 'Recipes',     icon: Sparkles, badge: snippetCount    > 0 ? snippetCount    : '' },
  ]);

  // ── Validation --------------------------------------------------------
  const slugRe = /^[A-Za-z0-9][A-Za-z0-9_-]*$/;
  const nameError = $derived(
    !opts.name.trim()              ? 'Name is required' :
    !slugRe.test(opts.name.trim()) ? 'Use letters, digits, "-" or "_"' :
    null
  );
  const versionError = $derived(
    !opts.version.trim() ? 'Version is required' : null
  );
  const isValid = $derived(!nameError && !versionError);

  // ── Submit ------------------------------------------------------------
  // Two-step flow: (1) "Export ZIP" opens Arbor's FilePickerModal in save
  // mode; (2) the picker's onConfirm fires `runExport(path)` which calls the
  // backend command. Keeping the picker separate (not a Tauri-native dialog)
  // means the user gets the same look + keyboard shortcuts as everywhere
  // else in the app.
  let exporting   = $state(false);
  let pickerOpen  = $state(false);

  function handleExport() {
    if (!isValid || exporting) return;
    // Sync CSV-backed fields back into the payload before we open the picker
    // so the slug used for the suggested filename matches what we'll write.
    opts.keywords       = csvToArr(keywordsCsv);
    opts.fs_scope       = csvToArr(fsScopeCsv);
    opts.terminal_scope = csvToArr(terminalScopeCsv);
    opts.network        = csvToArr(networkCsv);
    pickerOpen = true;
  }

  async function runExport(targetPath: string) {
    pickerOpen = false;
    exporting  = true;
    try {
      const written = await exportPluginTemplateToPath(opts, targetPath);
      uiStore.showToast(`Plugin template saved to ${written}`, 'success');
      onExported?.(opts.name);
      onClose();
    } catch (err) {
      uiStore.showToast(`Export failed: ${err}`, 'error');
    } finally {
      exporting = false;
    }
  }

  // ── Recipe metadata (used to render the Recipes tab grid) ──────────────
  // Order matches the same recipe array in plugin_template_commands.rs so the
  // generated main.lua reads top-down in the same order shown in the UI.
  type RecipeKey =
    | 'snippet_command' | 'snippet_keybinding' | 'snippet_settings_panel'
    | 'snippet_modal'   | 'snippet_action_toolbar' | 'snippet_sidebar'
    | 'snippet_notification' | 'snippet_job_spawn' | 'snippet_scheduler'
    | 'snippet_http_get';

  const RECIPES: Array<{
    key:   RecipeKey;
    title: string;
    icon:  any;
    blurb: string;
    requires?: string;
  }> = [
    { key: 'snippet_command',        title: 'Command palette',  icon: MessageSquare,
      blurb: 'Register a Ctrl+K command and an event handler.' },
    { key: 'snippet_keybinding',     title: 'Keyboard shortcut', icon: KeyRound,
      blurb: 'Bind a key combo to a plugin action.' },
    { key: 'snippet_settings_panel', title: 'Settings panel',   icon: Settings,
      blurb: 'Gear icon + persisted form (toggle / text / number).' },
    { key: 'snippet_modal',          title: 'Modal form',       icon: LayoutDashboard,
      blurb: 'One-shot ad-hoc form with on_submit callback.' },
    { key: 'snippet_action_toolbar', title: 'Toolbar action',   icon: MousePointer,
      blurb: 'Adds a button to the graph header toolbar.' },
    { key: 'snippet_sidebar',        title: 'Right-side panel', icon: BookOpen,
      blurb: 'Custom panel on the right ActivityBar with lazy content.' },
    { key: 'snippet_notification',   title: 'Notification',     icon: Bell,
      blurb: 'arbor.notify — toast + bell tray entry.' },
    { key: 'snippet_job_spawn',      title: 'Background job',   icon: Play,
      blurb: 'Run a process and stream output to the Jobs panel.',
      requires: 'terminal' },
    { key: 'snippet_scheduler',      title: 'Scheduler',        icon: Clock,
      blurb: 'Fire an action on a fixed-rate / cron / fixed-delay trigger.',
      requires: 'scheduler' },
    { key: 'snippet_http_get',       title: 'HTTP request',     icon: NetworkIcon,
      blurb: 'arbor.http.get — needs a host in the network allowlist.',
      requires: 'network' },
  ];
</script>

<Modal {onClose} width="780px" height="640px" padBody={false} ariaLabel="Export plugin template">
  {#snippet header()}
    <ModalHeader {onClose}>
      <Sparkles size={14} />
      <span class="modal-title">Export Plugin Template</span>
      <Badge variant="pill" size="sm">.zip</Badge>
    </ModalHeader>
  {/snippet}

  <div class="ept-root">
    <div class="ept-tabbar">
      <Tabs
        items={tabItems}
        value={activeTab}
        variant="underline"
        onSelect={(id) => activeTab = id as TabId}
      />
    </div>

    <div class="ept-body">
      {#if activeTab === 'identity'}
        <section class="ept-section">
          <h4 class="ept-h4"><Tag size={11}/> Identity</h4>
          <p class="ept-desc">These end up in <code>plugin.toml</code>.</p>

          <div class="ept-grid two">
            <label class="ept-field">
              <span>Name</span>
              <Input bind:value={opts.name} placeholder="my-plugin" error={nameError} />
            </label>
            <label class="ept-field">
              <span>Version</span>
              <Input bind:value={opts.version} placeholder="0.1.0" error={versionError} />
            </label>
            <label class="ept-field span-2">
              <span>Description</span>
              <Input bind:value={opts.description} placeholder="A short summary of what the plugin does." />
            </label>
            <label class="ept-field">
              <span>Author</span>
              <Input bind:value={opts.author} placeholder="Your name" />
            </label>
            <label class="ept-field">
              <span>License</span>
              <Input bind:value={opts.license} placeholder="MIT, Apache-2.0, …" />
            </label>
            <label class="ept-field span-2">
              <span>Repository</span>
              <Input bind:value={opts.repository} placeholder="https://github.com/you/my-plugin" />
            </label>
            <label class="ept-field span-2">
              <span>Keywords <small>(comma-separated)</small></span>
              <Input bind:value={keywordsCsv} placeholder="git, productivity, ci" />
            </label>
          </div>
        </section>

        <Alert variant="info">
          The bundle ships with <code>sdk.d.lua</code> + <code>.luarc.json</code>
          so lua-language-server picks up <code>arbor.*</code> autocomplete out of the box.
        </Alert>
      {/if}

      {#if activeTab === 'permissions'}
        <section class="ept-section">
          <h4 class="ept-h4"><Lock size={11}/> Capabilities</h4>
          <p class="ept-desc">
            Higher tiers include lower ones. Anything left at <code>none</code>
            is silently omitted from <code>plugin.toml</code>.
          </p>

          <div class="ept-grid two">
            <label class="ept-field">
              <span><HardDrive size={11}/> Filesystem</span>
              <Select bind:value={opts.fs} options={[
                { value: 'none',  label: 'none — no fs access' },
                { value: 'read',  label: 'read — arbor.fs read ops' },
                { value: 'write', label: 'write — read + write' },
              ]}/>
            </label>
            <label class="ept-field">
              <span>FS scope <small>(empty = active repo)</small></span>
              <Input bind:value={fsScopeCsv} placeholder="*  or  /home/me/foo, /tmp"
                     disabled={opts.fs === 'none'} />
            </label>

            <label class="ept-field">
              <span><GitBranch size={11}/> Git</span>
              <Select bind:value={opts.git} options={[
                { value: 'none',            label: 'none' },
                { value: 'read',            label: 'read — arbor.repo / notes read' },
                { value: 'write',           label: 'write — commit/push/fetch/clone' },
                { value: 'history_rewrite', label: 'history_rewrite — rebase/reset/force-push' },
              ]}/>
            </label>
            <div></div>

            <label class="ept-field">
              <span><TerminalSquare size={11}/> Terminal</span>
              <Select bind:value={opts.terminal} options={[
                { value: 'none',     label: 'none' },
                { value: 'commands', label: 'commands — allowlist below' },
                { value: 'any',      label: 'any — arbitrary commands' },
              ]}/>
            </label>
            <label class="ept-field">
              <span>Allowed commands <small>(when "commands")</small></span>
              <Input bind:value={terminalScopeCsv} placeholder="cargo, mvn, gradle"
                     disabled={opts.terminal !== 'commands'} />
            </label>

            <label class="ept-field span-2">
              <span><Globe size={11}/> Network allowlist <small>(comma-separated hostnames)</small></span>
              <Input bind:value={networkCsv} placeholder="api.github.com, gitlab.com" />
            </label>

            <label class="ept-field">
              <span><FolderTree size={11}/> Issues</span>
              <Select bind:value={opts.issues} options={[
                { value: 'none',  label: 'none' },
                { value: 'read',  label: 'read' },
                { value: 'write', label: 'write — transition / comment' },
              ]}/>
            </label>
            <label class="ept-field">
              <span>Toolchain</span>
              <Select bind:value={opts.toolchain} options={[
                { value: 'none',  label: 'none' },
                { value: 'read',  label: 'read — list / detect' },
                { value: 'write', label: 'write — add / set active' },
              ]}/>
            </label>
          </div>

          <h4 class="ept-h4 mt"><Workflow size={11}/> Cross-plugin & env</h4>
          <div class="ept-toggle-grid">
            <Toggle bind:checked={opts.env_read}              label="Read environment variables"
                    description="os.getenv inside the sandbox" />
            <Toggle bind:checked={opts.service_export}        label="Export services"
                    description="arbor.service.export — let other plugins call yours" />
            <Toggle bind:checked={opts.service_call}          label="Call services"
                    description="arbor.service.call — invoke other plugins' services" />
            <Toggle bind:checked={opts.settings_read_others}  label="Read other plugins' settings"
                    description="arbor.settings.read(plugin, key)" />
          </div>
        </section>
      {/if}

      {#if activeTab === 'hooks'}
        <section class="ept-section">
          <h4 class="ept-h4"><Zap size={11}/> Lifecycle hooks</h4>
          <p class="ept-desc">
            Each toggled hook is declared in <code>[hooks]</code> AND
            stubbed in <code>main.lua</code> with a TODO body.
          </p>

          <div class="ept-toggle-grid">
            <Toggle bind:checked={opts.hook_on_plugin_load}   label="on_plugin_load"   description="Plugin constructor — fired once on load" />
            <Toggle bind:checked={opts.hook_on_repo_open}     label="on_repo_open"     description="A repo tab is opened" />
            <Toggle bind:checked={opts.hook_on_repo_close}    label="on_repo_close"    description="A repo tab is closed" />
            <Toggle bind:checked={opts.hook_on_tab_switch}    label="on_tab_switch"    description="User switched the active tab" />
            <Toggle bind:checked={opts.hook_on_commit}        label="on_commit"        description="A commit was created" />
            <Toggle bind:checked={opts.hook_on_push}          label="on_push"          description="A push completed" />
            <Toggle bind:checked={opts.hook_on_pull}          label="on_pull"          description="A pull completed" />
            <Toggle bind:checked={opts.hook_on_fetch}         label="on_fetch"         description="A fetch completed" />
            <Toggle bind:checked={opts.hook_on_checkout}      label="on_checkout"      description="A checkout happened" />
            <Toggle bind:checked={opts.hook_on_branch_create} label="on_branch_create" description="A branch was created" />
            <Toggle bind:checked={opts.hook_on_branch_delete} label="on_branch_delete" description="A branch was deleted" />
            <Toggle bind:checked={opts.hook_on_mr_opened}     label="on_mr_opened"     description="A PR / MR was opened" />
            <Toggle bind:checked={opts.hook_on_mr_merged}     label="on_mr_merged"     description="A PR / MR was merged" />
          </div>

          <h4 class="ept-h4 mt"><Clock size={11}/> Scheduler</h4>
          <Toggle bind:checked={opts.include_scheduler}
                  label="Enable [scheduler]"
                  description="Lets main.lua call arbor.scheduler.register" />
        </section>
      {/if}

      {#if activeTab === 'recipes'}
        <section class="ept-section">
          <h4 class="ept-h4"><Sparkles size={11}/> Lua recipes</h4>
          <p class="ept-desc">
            Toggled recipes are appended to <code>main.lua</code> as ready-to-run blocks
            with comments — pick the surfaces you want, edit afterwards.
          </p>

          <div class="ept-recipes">
            {#each RECIPES as r (r.key)}
              {@const Icon = r.icon}
              {@const checked = opts[r.key]}
              <Card padding="none" class={'ept-recipe ' + (checked ? 'on' : '')}>
                <button
                  type="button"
                  class="ept-recipe-btn"
                  aria-pressed={checked}
                  onclick={() => opts[r.key] = !opts[r.key]}
                >
                  <span class="ept-recipe-icon"><Icon size={14}/></span>
                  <span class="ept-recipe-body">
                    <span class="ept-recipe-title">{r.title}</span>
                    <span class="ept-recipe-blurb">{r.blurb}</span>
                  </span>
                  <span class="ept-recipe-toggle">
                    <Toggle checked={checked} ariaLabel={r.title} />
                  </span>
                </button>
                {#if r.requires === 'terminal' && checked && opts.terminal === 'none'}
                  <div class="ept-recipe-warn">
                    <Alert variant="warning" compact>
                      Requires the <code>terminal</code> permission — set it under "Permissions".
                    </Alert>
                  </div>
                {:else if r.requires === 'network' && checked && opts.network.length === 0 && !networkCsv.trim()}
                  <div class="ept-recipe-warn">
                    <Alert variant="warning" compact>
                      Add a hostname to the network allowlist under "Permissions".
                    </Alert>
                  </div>
                {:else if r.requires === 'scheduler' && checked && !opts.include_scheduler}
                  <div class="ept-recipe-warn">
                    <Alert variant="warning" compact>
                      Enable <code>[scheduler]</code> in the "Hooks" tab.
                    </Alert>
                  </div>
                {/if}
              </Card>
            {/each}
          </div>
        </section>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <ModalFooter align="between">
      <span class="ept-footer-hint">
        <ListChecks size={11}/>
        {permissionCount} permission{permissionCount === 1 ? '' : 's'} ·
        {hookCount} hook{hookCount === 1 ? '' : 's'} ·
        {snippetCount} recipe{snippetCount === 1 ? '' : 's'}
      </span>
      <div class="ept-footer-actions">
        <Button variant="ghost" onclick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          onclick={handleExport}
          disabled={!isValid}
          loading={exporting}
        >
          {#snippet iconStart()}<Download size={13}/>{/snippet}
          Export ZIP
        </Button>
      </div>
    </ModalFooter>
  {/snippet}
</Modal>

{#if pickerOpen}
  <FilePickerModal
    mode="save"
    title="Save plugin template"
    extensions={['zip']}
    initialFilename={`${opts.name}.zip`}
    onConfirm={runExport}
    onCancel={() => { pickerOpen = false; }}
  />
{/if}

<style>
  .modal-title {
    font-weight: 500;
    color: var(--text-primary);
  }

  .ept-root {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .ept-tabbar {
    flex-shrink: 0;
    padding: 0 14px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .ept-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px 18px 22px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .ept-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .ept-h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: var(--text-muted);
  }
  .ept-h4.mt { margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border-subtle); }

  .ept-desc {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    line-height: 1.5;
  }
  .ept-desc code,
  .ept-recipe-warn code,
  :global(.ept-body code) {
    font-family: var(--font-code);
    font-size: 10.5px;
    background: var(--bg-overlay);
    color: var(--accent);
    padding: 0 4px;
    border-radius: var(--radius-sm);
  }

  .ept-grid {
    display: grid;
    gap: 10px 14px;
  }
  .ept-grid.two   { grid-template-columns: 1fr 1fr; }
  .ept-grid .span-2 { grid-column: 1 / -1; }

  .ept-field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    min-width: 0;
  }
  .ept-field span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-secondary);
    font-weight: 500;
  }
  .ept-field small { color: var(--text-disabled); font-weight: 400; }

  .ept-toggle-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 18px;
  }

  /* ── Recipes grid ─────────────────────────────────────────────────────── */
  .ept-recipes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  :global(.ept-recipe) {
    transition: border-color var(--transition-fast),
                box-shadow var(--transition-fast),
                background var(--transition-fast);
  }
  :global(.ept-recipe.on) {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent) !important;
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 18%, transparent);
    background: color-mix(in srgb, var(--accent) 4%, var(--bg-elevated)) !important;
  }

  .ept-recipe-btn {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    padding: 11px 12px;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans);
  }
  .ept-recipe-btn:hover { background: var(--bg-hover); border-radius: inherit; }

  .ept-recipe-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    background: var(--accent-subtle);
    color: var(--accent);
    flex-shrink: 0;
  }
  .ept-recipe-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .ept-recipe-title {
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-weight: 500;
  }
  .ept-recipe-blurb {
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .ept-recipe-toggle {
    flex-shrink: 0;
    /* Nest the toggle inside the button — pointer-events:none so the parent
       button captures the click and we drive `checked` from there. */
    pointer-events: none;
  }
  .ept-recipe-warn {
    padding: 0 10px 10px;
  }

  /* ── Footer ────────────────────────────────────────────────────────────── */
  .ept-footer-hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .ept-footer-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
