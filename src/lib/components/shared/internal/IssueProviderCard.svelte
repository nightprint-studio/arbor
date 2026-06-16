<script lang="ts" module>
  /** A user identity shown on a connected provider card. */
  export interface ProviderUserInfo {
    displayName: string;
    email:       string | null;
    avatarUrl:   string | null;
  }

  /** Current connection status for one provider. */
  export interface ProviderStatus {
    authenticated: boolean;
    user:          ProviderUserInfo | null;
    domain?:       string | null;
  }

  /**
   * The irreducible per-provider glue the generic card can't derive from the
   * descriptor: the OAuth browser dance (provider-specific command + event),
   * the field-form submit, disconnect, and status fetch — plus optional UX
   * nuances (extra validation, dynamic hints).
   */
  export interface ProviderBinding {
    /** Brand-coloured CSS value for CTAs, e.g. `"var(--brand-linear)"`. */
    brandColor: string;
    /** Tauri event fired when the OAuth redirect completes (payload: boolean). */
    oauthEvent: string;
    /** Begin the OAuth flow; returns the authorize URL to open in the browser. */
    startOAuth(): Promise<string>;
    /** Connect using a field form (values keyed by `AuthField.key`). */
    connectFields(values: Record<string, string>): Promise<void>;
    /** Disconnect / forget credentials. */
    disconnect(): Promise<void>;
    /** Fetch the current connection status. */
    loadStatus(): Promise<ProviderStatus>;
    /** Called after a successful connect/disconnect (e.g. to sync a store). */
    afterChange?(): void;
    /** Extra validation beyond per-field `required` (e.g. Jira email when Cloud). */
    validateFields?(values: Record<string, string>): boolean;
    /** Dynamic hint shown above a field form (e.g. Cloud vs Server guidance). */
    fieldsHint?(values: Record<string, string>): string;
    /** OAuth form copy. */
    oauthHintIdle:    string;
    oauthHintWaiting: string;
    oauthIdleLabel:   string;
    /** Provider key for the "use my own OAuth app" advanced panel (optional). */
    advancedProvider?: 'linear' | 'jira';
  }
</script>

<script lang="ts">
  import { XCircle, Eye, EyeOff, ChevronDown, ChevronRight, Settings2 } from 'lucide-svelte';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import type { ProviderDescriptor, AuthMethod } from '$lib/types/issues';
  import { uiStore } from '$lib/stores/ui.svelte';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import BrandTile, { type Brand } from './BrandTile.svelte';
  import ProviderConnectionStatus from './ProviderConnectionStatus.svelte';
  import OAuthBrowserAuthForm from './OAuthBrowserAuthForm.svelte';
  import ProviderUserBadge from './ProviderUserBadge.svelte';
  import OAuthAdvancedPanel from '$lib/components/shared/OAuthAdvancedPanel.svelte';

  type ConnState = 'checking' | 'disconnected' | 'connecting' | 'connected';

  interface Props {
    descriptor: ProviderDescriptor;
    binding:    ProviderBinding;
  }

  let { descriptor, binding }: Props = $props();

  let state       = $state<ConnState>('checking');
  let method      = $state<string | null>(null);
  let fieldValues = $state<Record<string, string>>({});
  let showSecret  = $state<Record<string, boolean>>({});
  let saving      = $state(false);
  let formError   = $state('');
  let oauthWaiting = $state(false);
  let oauthError   = $state('');
  let advancedOpen = $state(false);
  let user        = $state<ProviderUserInfo | null>(null);
  let domain      = $state<string | null>(null);
  let oauthUnsub: (() => void) | null = null;

  const activeMethod = $derived<AuthMethod | undefined>(
    descriptor.authMethods.find((m) => m.id === method),
  );

  // ── Status ─────────────────────────────────────────────────────────────────
  $effect(() => {
    // Re-checks whenever the bound provider changes.
    void descriptor.id;
    void refreshStatus();
  });

  async function refreshStatus() {
    state = 'checking';
    try {
      const s = await binding.loadStatus();
      if (s.authenticated) {
        state  = 'connected';
        user   = s.user;
        domain = s.domain ?? null;
      } else {
        state = 'disconnected';
        user = null; domain = null;
      }
    } catch {
      state = 'disconnected';
      user = null; domain = null;
    }
  }

  // ── Method selection ─────────────────────────────────────────────────────────
  function pickMethod(id: string) {
    method = id;
    formError = ''; oauthError = '';
    const m = descriptor.authMethods.find((x) => x.id === id);
    if (m?.kind.type === 'oauth') startOAuthFlow();
  }

  function cancelMethod() {
    method = null; formError = ''; oauthError = '';
  }

  // ── OAuth ──────────────────────────────────────────────────────────────────
  async function startOAuthFlow() {
    oauthWaiting = true; oauthError = '';
    state = 'connecting';
    oauthUnsub?.();
    oauthUnsub = await listen<boolean>(binding.oauthEvent, ({ payload }) => {
      oauthUnsub?.(); oauthUnsub = null;
      oauthWaiting = false;
      if (payload) {
        method = null;
        void refreshStatus();
        binding.afterChange?.();
        uiStore.showToast(`${descriptor.displayName} connected via OAuth`, 'success');
      } else {
        state = 'disconnected';
        oauthError = 'OAuth failed — check your client ID or try again.';
      }
    });
    try {
      const url = await binding.startOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (err) {
      oauthWaiting = false; state = 'disconnected';
      oauthError = String(err);
      oauthUnsub?.(); oauthUnsub = null;
    }
  }

  // ── Field form ───────────────────────────────────────────────────────────────
  function fieldsOf(m: AuthMethod | undefined) {
    return m && m.kind.type === 'fields' ? m.kind.fields : [];
  }

  const canSubmit = $derived.by(() => {
    const fields = fieldsOf(activeMethod);
    if (fields.length === 0) return false;
    for (const f of fields) {
      if (f.required && !(fieldValues[f.key]?.trim())) return false;
    }
    if (binding.validateFields && !binding.validateFields(fieldValues)) return false;
    return true;
  });

  async function submitFields() {
    if (!canSubmit) return;
    saving = true; formError = '';
    try {
      const values: Record<string, string> = {};
      for (const f of fieldsOf(activeMethod)) values[f.key] = (fieldValues[f.key] ?? '').trim();
      await binding.connectFields(values);
      fieldValues = {}; method = null;
      await refreshStatus();
      binding.afterChange?.();
      uiStore.showToast(`${descriptor.displayName} connected`, 'success');
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Disconnect / cancel ──────────────────────────────────────────────────────
  async function disconnect() {
    oauthUnsub?.(); oauthUnsub = null; oauthWaiting = false;
    await binding.disconnect().catch(() => {});
    state = 'disconnected'; method = null;
    user = null; domain = null; oauthError = ''; formError = '';
    binding.afterChange?.();
    uiStore.showToast(`${descriptor.displayName} disconnected`, 'info');
  }

  function cancelConnecting() {
    oauthWaiting = false; state = 'disconnected'; method = null;
    oauthUnsub?.(); oauthUnsub = null;
  }

  const connectOptions = $derived(descriptor.authMethods.map((m) => ({ id: m.id, label: m.label })));
</script>

<div class="provider-card" class:flow-active={state === 'connecting'}>
  <BrandTile brand={descriptor.icon as Brand} />
  <div class="provider-main">
    <div class="provider-top">
      <div class="provider-info">
        <span class="provider-name">{descriptor.displayName}</span>
        {#if descriptor.description}
          <span class="provider-desc">{descriptor.description}</span>
        {/if}
      </div>
      <ProviderConnectionStatus
        state={state}
        connectingLabel="Waiting for browser…"
        onDisconnect={disconnect}
        onCancel={cancelConnecting}
      >
        {#snippet connect()}
          {#if method === null}
            <SplitButton
              label="Connect"
              color={binding.brandColor}
              direction="down"
              options={connectOptions}
              onclick={() => { const first = descriptor.authMethods[0]; if (first) pickMethod(first.id); }}
              onselect={(id) => pickMethod(id)}
            />
          {/if}
        {/snippet}
      </ProviderConnectionStatus>
    </div>

    {#if state === 'connected' && user}
      <ProviderUserBadge
        avatarUrl={user.avatarUrl}
        name={user.displayName}
        secondary={user.email ?? domain}
      />
    {/if}

    <!-- OAuth method -->
    {#if activeMethod?.kind.type === 'oauth'}
      <OAuthBrowserAuthForm
        waiting={oauthWaiting}
        error={oauthError}
        brandColor={binding.brandColor}
        hintIdle={binding.oauthHintIdle}
        hintWaiting={binding.oauthHintWaiting}
        idleLabel={binding.oauthIdleLabel}
        busyLabel="Waiting for browser…"
        onAuthorize={startOAuthFlow}
        onCancel={() => { oauthWaiting = false; method = null; oauthError = ''; oauthUnsub?.(); }}
      />
    {/if}

    <!-- Field-form method -->
    {#if activeMethod?.kind.type === 'fields'}
      <div class="inline-form">
        {#if binding.fieldsHint}
          <p class="form-hint">{binding.fieldsHint(fieldValues)}</p>
        {/if}
        {#each fieldsOf(activeMethod) as field (field.key)}
          {#if field.widget === 'secret'}
            <div class="input-with-addon">
              <input
                class="text-input"
                type={showSecret[field.key] ? 'text' : 'password'}
                placeholder={field.placeholder ?? field.label}
                bind:value={fieldValues[field.key]}
              />
              <button class="addon-btn" type="button" onclick={() => showSecret[field.key] = !showSecret[field.key]}>
                {#if showSecret[field.key]}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
              </button>
            </div>
          {:else}
            <input
              class="text-input"
              type="text"
              placeholder={field.placeholder ?? field.label}
              bind:value={fieldValues[field.key]}
            />
          {/if}
        {/each}
        <div class="inline-form-row">
          <button class="btn-save" style="background:{binding.brandColor}" onclick={submitFields} disabled={saving || !canSubmit}>
            {saving ? 'Connecting…' : 'Connect'}
          </button>
          <button class="btn-cancel" type="button" onclick={cancelMethod}>Cancel</button>
        </div>
        {#if formError}<div class="provider-error"><XCircle size={12} />{formError}</div>{/if}
      </div>
    {/if}

    {#if binding.advancedProvider}
      <button class="advanced-toggle" type="button" onclick={() => advancedOpen = !advancedOpen}>
        {#if advancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
        <Settings2 size={11} />
        Advanced — use my own OAuth app
      </button>
      {#if advancedOpen}
        <OAuthAdvancedPanel provider={binding.advancedProvider} />
      {/if}
    {/if}
  </div>
</div>

<style>
  .provider-card {
    display: flex; align-items: flex-start; gap: 13px;
    padding: 13px 14px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    transition: border-color var(--transition-fast);
  }
  .provider-card.flow-active { border-color: var(--accent); }

  .provider-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 10px; }
  .provider-top  { display: flex; align-items: center; gap: 10px; }
  .provider-info { flex: 1; display: flex; flex-direction: column; gap: 2px; }
  .provider-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .provider-desc { font-size: 11px; color: var(--text-muted); }

  .inline-form { display: flex; flex-direction: column; gap: 8px; }
  .inline-form-row { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; }

  .btn-save {
    padding: 5px 14px; border: none; border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500; color: #fff;
    cursor: pointer; transition: filter var(--transition-fast); white-space: nowrap;
    display: flex; align-items: center; gap: 5px;
  }
  .btn-save:hover:not(:disabled) { filter: brightness(1.12); }
  .btn-save:disabled { opacity: 0.45; cursor: not-allowed; }

  .btn-cancel {
    padding: 5px 10px; background: transparent;
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px; color: var(--text-muted);
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .btn-cancel:hover { background: var(--bg-hover); color: var(--text-primary); }

  .form-hint { font-size: 10.5px; color: var(--text-muted); margin: 0; line-height: 1.5; }

  .advanced-toggle {
    display: inline-flex; align-items: center; gap: 5px;
    align-self: flex-start;
    padding: 4px 8px;
    background: transparent; color: var(--text-muted);
    border: 1px dashed var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast);
  }
  .advanced-toggle:hover { color: var(--text-primary); border-color: var(--accent); background: var(--bg-hover); }

  .provider-error {
    display: flex; align-items: center; gap: 6px;
    font-size: 11px; color: var(--error, #f87171);
  }

  .text-input {
    padding: 5px 8px; background: var(--bg-input); color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 12px;
    outline: none; transition: border-color var(--transition-fast);
    flex: 1; min-width: 90px;
  }
  .text-input:focus { border-color: var(--accent); }

  .input-with-addon { display: flex; }
  .input-with-addon .text-input { border-radius: var(--radius-sm) 0 0 var(--radius-sm); flex: 1; }
  .addon-btn {
    display: flex; align-items: center; justify-content: center;
    width: 28px; background: var(--bg-input); border: 1px solid var(--border);
    border-left: none; border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    cursor: pointer; color: var(--text-muted); transition: color var(--transition-fast);
  }
  .addon-btn:hover { color: var(--text-primary); }
</style>
