<script lang="ts">
  import { XCircle, Eye, EyeOff, ChevronDown, ChevronRight, Settings2, Copy, ExternalLink } from 'lucide-svelte';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { onDestroy } from 'svelte';
  import type { ProviderDescriptor, AuthMethod, ProviderUserInfo, ProviderOAuthDone } from '$lib/types/providers';
  import { type ProviderConnectionService, PROVIDER_OAUTH_DONE_EVENT } from '$lib/ipc/providers';
  import { fieldsOf, isFieldRequired, resolveHint, canSubmitFields } from '$lib/utils/providerRules';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import { tooltip } from '$lib/actions/tooltip';
  import SplitButton from '$lib/components/shared/ui/SplitButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import BrandTile, { type Brand } from './BrandTile.svelte';
  import ProviderConnectionStatus from './ProviderConnectionStatus.svelte';
  import OAuthBrowserAuthForm from './OAuthBrowserAuthForm.svelte';
  import ProviderUserBadge from './ProviderUserBadge.svelte';
  import OAuthAdvancedPanel from '$lib/components/shared/OAuthAdvancedPanel.svelte';

  type ConnState = 'checking' | 'disconnected' | 'connecting' | 'connected';

  interface Props {
    descriptor: ProviderDescriptor;
    /** The domain's generic, by-id connection IPC (issue or git). The card is
     *  identical for both — only the injected service differs. */
    service:    ProviderConnectionService;
    /** Optional callback after a successful connect/disconnect (e.g. sync a store). */
    onchange?:  () => void;
    /** Layout. `card` (default) is the horizontal settings-row card; `compact`
     *  is a centered vertical layout for narrow contexts (e.g. the Issues
     *  sidebar setup panel). The connection logic is identical — only the
     *  surrounding chrome differs. */
    variant?:   'card' | 'compact';
  }

  let { descriptor, service, onchange, variant = 'card' }: Props = $props();

  let state        = $state<ConnState>('checking');
  let method       = $state<string | null>(null);
  let fieldValues  = $state<Record<string, string>>({});
  let showSecret   = $state<Record<string, boolean>>({});
  let saving       = $state(false);
  let formError    = $state('');
  let oauthWaiting = $state(false);
  let oauthError   = $state('');
  let deviceInfo   = $state<{ userCode: string; verificationUri: string } | null>(null);
  let advancedOpen = $state(false);
  let user         = $state<ProviderUserInfo | null>(null);
  let accountLabel = $state<string | null>(null);
  let oauthUnsub: (() => void) | null = null;

  const activeMethod = $derived<AuthMethod | undefined>(
    descriptor.authMethods.find((m) => m.id === method),
  );
  const hasOAuth = $derived(descriptor.authMethods.some((m) => m.kind.type === 'oauth'));
  const connectOptions = $derived(descriptor.authMethods.map((m) => ({ id: m.id, label: m.label })));
  const brandColor = $derived(descriptor.brandColor ?? 'var(--accent)');
  const formHint = $derived(
    activeMethod?.kind.type === 'fields' ? resolveHint(activeMethod.kind.hints, fieldValues) : null,
  );
  const canSubmit = $derived(canSubmitFields(activeMethod, fieldValues));

  // Re-check whenever the bound descriptor changes.
  $effect(() => {
    void descriptor.id;
    void refreshStatus();
  });

  onDestroy(() => { oauthUnsub?.(); });

  async function refreshStatus() {
    state = 'checking';
    try {
      const s = await service.authStatus(descriptor.id);
      if (s.authenticated) {
        state = 'connected';
        user = s.user ?? null;
        accountLabel = s.accountLabel ?? null;
      } else {
        state = 'disconnected'; user = null; accountLabel = null;
      }
    } catch {
      state = 'disconnected'; user = null; accountLabel = null;
    }
  }

  function pickMethod(id: string) {
    method = id; formError = ''; oauthError = '';
    const m = descriptor.authMethods.find((x) => x.id === id);
    if (m?.kind.type === 'oauth') void startOAuthFlow(id);
  }

  function cancelMethod() { method = null; formError = ''; oauthError = ''; }

  // ── OAuth ──────────────────────────────────────────────────────────────────
  async function startOAuthFlow(methodId: string) {
    oauthWaiting = true; oauthError = ''; deviceInfo = null;
    state = 'connecting';
    oauthUnsub?.();
    // One listener on the unified event, routed by provider id — so two
    // concurrent OAuth logins each settle their own card.
    oauthUnsub = await listen<ProviderOAuthDone>(PROVIDER_OAUTH_DONE_EVENT, ({ payload }) => {
      if (payload.id !== descriptor.id) return;
      oauthUnsub?.(); oauthUnsub = null;
      oauthWaiting = false; deviceInfo = null;
      if (payload.ok) {
        method = null;
        void refreshStatus();
        onchange?.();
        uiStore.showToast(`${descriptor.displayName} connected`, 'success');
      } else {
        state = 'disconnected';
        oauthError = payload.error ?? 'OAuth failed — please try again.';
      }
    });
    try {
      const start = await service.startOauth(descriptor.id, methodId);
      if (start.type === 'redirect') {
        try { await openUrl(start.url); } catch { /* user can copy from the browser */ }
      } else {
        deviceInfo = { userCode: start.userCode, verificationUri: start.verificationUri };
        try { await openUrl(start.verificationUri); } catch { /* user can open manually */ }
      }
    } catch (err) {
      oauthWaiting = false; state = 'disconnected';
      oauthError = String(err);
      oauthUnsub?.(); oauthUnsub = null;
    }
  }

  function copyDeviceCode() {
    if (deviceInfo) void copyToClipboard(deviceInfo.userCode, { successToast: 'Code copied to clipboard' });
  }
  function openVerification() {
    if (deviceInfo) openUrl(deviceInfo.verificationUri).catch(() => {});
  }

  // ── Field form ───────────────────────────────────────────────────────────────
  async function submitFields(methodId: string) {
    if (!canSubmit) return;
    saving = true; formError = '';
    try {
      const values: Record<string, string> = {};
      for (const f of fieldsOf(activeMethod)) values[f.key] = (fieldValues[f.key] ?? '').trim();
      await service.connectFields(descriptor.id, methodId, values);
      fieldValues = {}; method = null;
      await refreshStatus();
      onchange?.();
      uiStore.showToast(`${descriptor.displayName} connected`, 'success');
    } catch (e) {
      formError = String(e);
    } finally {
      saving = false;
    }
  }

  // ── Disconnect / cancel ──────────────────────────────────────────────────────
  async function disconnect() {
    oauthUnsub?.(); oauthUnsub = null; oauthWaiting = false; deviceInfo = null;
    await service.disconnect(descriptor.id).catch(() => {});
    state = 'disconnected'; method = null; user = null; accountLabel = null;
    oauthError = ''; formError = '';
    onchange?.();
    uiStore.showToast(`${descriptor.displayName} disconnected`, 'info');
  }

  function cancelConnecting() {
    oauthWaiting = false; deviceInfo = null; state = 'disconnected'; method = null;
    oauthUnsub?.(); oauthUnsub = null;
  }
</script>

<!-- Connect/disconnect action row — identical across layouts. -->
{#snippet statusRow()}
  <ProviderConnectionStatus
    state={state}
    connectingLabel="Waiting…"
    onDisconnect={disconnect}
    onCancel={cancelConnecting}
  >
    {#snippet connect()}
      {#if method === null}
        <SplitButton
          label="Connect"
          color={brandColor}
          direction="down"
          options={connectOptions}
          onclick={() => { const first = descriptor.authMethods[0]; if (first) pickMethod(first.id); }}
          onselect={(id) => pickMethod(id)}
        />
      {/if}
    {/snippet}
  </ProviderConnectionStatus>
{/snippet}

<!-- User badge (connected) + the active method's form + advanced panel —
     identical across layouts. -->
{#snippet body()}
  {#if state === 'connected' && user}
    <ProviderUserBadge
      avatarUrl={user.avatarUrl ?? null}
      name={user.displayName}
      secondary={user.email ?? accountLabel}
    />
  {/if}

  <!-- OAuth method -->
  {#if activeMethod?.kind.type === 'oauth'}
    {#if deviceInfo}
      <div class="inline-form">
        <p class="form-hint">Open the verification page and enter this code:</p>
        <div class="device-code-row">
          <code class="device-code">{deviceInfo.userCode}</code>
          <button class="device-copy" type="button" use:tooltip={'Copy code'} onclick={copyDeviceCode}><Copy size={12} /></button>
          <button class="device-open" type="button" onclick={openVerification}>
            <ExternalLink size={11} /> Open {deviceInfo.verificationUri.replace(/^https?:\/\//, '')}
          </button>
        </div>
        <p class="form-hint">Arbor will detect the authorisation automatically.</p>
        <div class="inline-form-row">
          {#if oauthWaiting}<Spinner size={11} />{/if}
          <button class="btn-cancel" type="button" onclick={cancelConnecting}>Cancel</button>
        </div>
      </div>
    {:else}
      <OAuthBrowserAuthForm
        waiting={oauthWaiting}
        error={oauthError}
        brandColor={brandColor}
        hintIdle="Opens {descriptor.displayName} in the browser to authorize Arbor."
        hintWaiting="Browser opened — approve access then return here."
        idleLabel="Authorize with {descriptor.displayName}"
        busyLabel="Waiting for browser…"
        onAuthorize={() => startOAuthFlow(method ?? '')}
        onCancel={() => { oauthWaiting = false; method = null; oauthError = ''; oauthUnsub?.(); oauthUnsub = null; }}
      />
    {/if}
  {/if}

  <!-- Field-form method -->
  {#if activeMethod?.kind.type === 'fields'}
    <div class="inline-form">
      {#if formHint}<p class="form-hint">{formHint}</p>{/if}
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
        <button class="btn-save" style="background:{brandColor}" onclick={() => submitFields(method ?? '')} disabled={saving || !canSubmit}>
          {saving ? 'Connecting…' : 'Connect'}
        </button>
        <button class="btn-cancel" type="button" onclick={cancelMethod}>Cancel</button>
      </div>
      {#if formError}<div class="provider-error"><XCircle size={12} />{formError}</div>{/if}
    </div>
  {/if}

  {#if hasOAuth}
    <button class="advanced-toggle" type="button" onclick={() => advancedOpen = !advancedOpen}>
      {#if advancedOpen}<ChevronDown size={11} />{:else}<ChevronRight size={11} />{/if}
      <Settings2 size={11} />
      Advanced — use my own OAuth app
    </button>
    {#if advancedOpen}
      <OAuthAdvancedPanel provider={descriptor.id as 'linear' | 'jira' | 'github' | 'gitlab'} />
    {/if}
  {/if}
{/snippet}

{#if variant === 'compact'}
  <div class="provider-compact" class:flow-active={state === 'connecting'}>
    <BrandTile brand={descriptor.icon as Brand} size={22} tileSize={42} />
    <span class="pc-name">{descriptor.displayName}</span>
    {#if descriptor.description}
      <span class="pc-desc">{descriptor.description}</span>
    {/if}
    <div class="pc-action">{@render statusRow()}</div>
    <div class="pc-body">{@render body()}</div>
  </div>
{:else}
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
        {@render statusRow()}
      </div>
      {@render body()}
    </div>
  </div>
{/if}

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

  /* ── Compact (sidebar setup) layout ───────────────────────────────────────
     Same connection logic, centered vertical chrome for narrow panels. */
  .provider-compact {
    display: flex; flex-direction: column; align-items: center; text-align: center;
    gap: 10px; padding: 28px 20px;
  }
  .pc-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
  .pc-desc { font-size: 11px; color: var(--text-muted); line-height: 1.5; max-width: 220px; }
  .pc-action { display: flex; justify-content: center; }
  .pc-body {
    width: 100%; max-width: 240px;
    display: flex; flex-direction: column; gap: 8px; align-items: stretch;
  }
  /* The shared inline forms are left-aligned inside the centered column. */
  .pc-body :global(.form-hint) { text-align: left; }

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

  .device-code-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .device-code {
    font-family: var(--font-code); font-size: 18px; font-weight: 700; letter-spacing: 0.18em;
    padding: 6px 12px; background: var(--bg-base); color: var(--accent);
    border: 1px solid var(--border); border-radius: var(--radius-sm); user-select: all;
  }
  .device-copy {
    display: flex; align-items: center; justify-content: center; width: 28px; height: 28px;
    background: transparent; color: var(--text-muted);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    cursor: pointer; transition: all var(--transition-fast);
  }
  .device-copy:hover { color: var(--text-primary); background: var(--bg-hover); }
  .device-open {
    display: flex; align-items: center; gap: 5px; padding: 5px 10px;
    background: transparent; color: var(--text-secondary);
    border: 1px solid var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast); white-space: nowrap;
  }
  .device-open:hover { color: var(--text-primary); background: var(--bg-hover); }

  .advanced-toggle {
    display: inline-flex; align-items: center; gap: 5px; align-self: flex-start;
    padding: 4px 8px; background: transparent; color: var(--text-muted);
    border: 1px dashed var(--border); border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans); font-size: 11px;
    cursor: pointer; transition: all var(--transition-fast);
  }
  .advanced-toggle:hover { color: var(--text-primary); border-color: var(--accent); background: var(--bg-hover); }

  .provider-error { display: flex; align-items: center; gap: 6px; font-size: 11px; color: var(--error, #f87171); }

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
