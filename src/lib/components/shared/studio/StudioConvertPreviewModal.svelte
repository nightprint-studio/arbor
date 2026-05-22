<!--
  StudioConvertPreviewModal — preview the result of a cross-format
  conversion (YAML ↔ .properties for Phase 5.b; future TOML / JSON
  conversions will reuse this shell when needed).

  Three actions:
    · Copy — copy the converted text to clipboard.
    · Save As… — open a FilePickerModal to pick a destination path.
    · Replace — call back into the caller with the converted text so
      it can swap the open doc's buffer (history-tracked via the
      backend's normal setText flow).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { Copy, FolderOpen, Replace, AlertTriangle } from 'lucide-svelte';
  import { copyToClipboard } from '$lib/utils/clipboard';
  import Modal from '../Modal.svelte';
  import Alert from '../ui/Alert.svelte';
  import Button from '../ui/Button.svelte';
  import StudioTextPane from './StudioTextPane.svelte';
  import FilePickerModal from '../FilePickerModal.svelte';
  import {
    yamlToProperties,
    propertiesToYaml,
    type YamlToPropertiesOutput,
    type PropertiesToYamlOutput,
  } from '$lib/ipc/studio-convert';

  type Mode = 'yaml-to-properties' | 'properties-to-yaml';

  interface Props {
    mode:            Mode;
    sourceText:      string;
    defaultFilename: string;
    /** Caller-supplied replace handler. When user clicks "Replace in
     *  editor", we pass the converted text back to the caller, which
     *  routes it through the open doc's setText. `null` disables the
     *  Replace button (used when the converter output isn't for the
     *  currently open doc's format). */
    onReplace?:      ((text: string) => void | Promise<void>) | null;
    onClose:         () => void;
  }

  let { mode, sourceText, defaultFilename, onReplace, onClose }: Props = $props();

  let loading      = $state(true);
  let convertedText = $state<string>('');
  let warnings     = $state<string[]>([]);
  let error        = $state<string | null>(null);
  let stringsOnly  = $state(false);
  let savePickerOpen = $state(false);
  let copyDone     = $state(false);

  /** Outbound file extension based on the conversion direction. */
  const outExtension = $derived(mode === 'yaml-to-properties' ? 'properties' : 'yaml');
  const outLanguage  = $derived(mode === 'yaml-to-properties' ? 'properties' : 'yaml');
  const dialogTitle  = $derived(
    mode === 'yaml-to-properties'
      ? 'Convert YAML → .properties'
      : 'Convert .properties → YAML'
  );

  async function runConvert() {
    loading = true;
    error   = null;
    warnings = [];
    try {
      if (mode === 'yaml-to-properties') {
        const r: YamlToPropertiesOutput = await yamlToProperties(sourceText);
        convertedText = r.properties_text;
        warnings      = r.warnings;
      } else {
        const r: PropertiesToYamlOutput = await propertiesToYaml(sourceText, { stringsOnly });
        convertedText = r.yaml_text;
        warnings      = r.warnings;
      }
    } catch (e: any) {
      error = String(e?.message ?? e);
      convertedText = '';
    } finally {
      loading = false;
    }
  }

  onMount(() => { void runConvert(); });

  async function copyText() {
    if (await copyToClipboard(convertedText)) {
      copyDone = true;
      setTimeout(() => { copyDone = false; }, 1600);
    } else {
      error = 'Copy failed';
    }
  }

  function openSaveAs() { savePickerOpen = true; }
  async function onSaveAsPicked(p: string) {
    savePickerOpen = false;
    try {
      // Save through the host's `fs_write_text_file` command.
      // The codec output is already UTF-8 plain text.
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('fs_write_text_file', { path: p, content: convertedText });
    } catch (e: any) {
      error = `Save failed: ${e?.message ?? e}`;
    }
  }

  async function doReplace() {
    if (!onReplace) return;
    try {
      await onReplace(convertedText);
      onClose();
    } catch (e: any) {
      error = `Replace failed: ${e?.message ?? e}`;
    }
  }
</script>

<Modal
  onClose={onClose}
  width="min(820px, 92vw)"
  height="min(680px, 90vh)"
  padBody={false}
  ariaLabel={dialogTitle}
>
  {#snippet header()}
    <h3 style="margin: 0; font-size: 13px;">{dialogTitle}</h3>
  {/snippet}

  <div class="scv-body">
    {#if mode === 'properties-to-yaml'}
      <div class="scv-toolbar">
        <label class="scv-toggle">
          <input type="checkbox" bind:checked={stringsOnly} onchange={() => void runConvert()} />
          <span>Strings only — disable best-effort type inference (every value stays a quoted string)</span>
        </label>
      </div>
    {/if}

    {#if error}
      <div class="scv-alert-wrap">
        <Alert variant="error" compact text={error} />
      </div>
    {/if}

    {#if warnings.length > 0}
      <div class="scv-alert-wrap">
        <Alert variant="warning" compact>
          <div class="scv-warnings">
            <strong><AlertTriangle size={13} /> {warnings.length} warning{warnings.length === 1 ? '' : 's'}</strong>
            <ul>
              {#each warnings as w}<li>{w}</li>{/each}
            </ul>
          </div>
        </Alert>
      </div>
    {/if}

    <div class="scv-preview">
      {#if loading}
        <div class="scv-loading">Converting…</div>
      {:else}
        <StudioTextPane
          language={outLanguage}
          value={convertedText}
          readOnly
        />
      {/if}
    </div>
  </div>

  {#snippet footer()}
    <div class="scv-footer">
      <Button variant="secondary" onclick={onClose}>Close</Button>
      <div class="scv-footer-spacer"></div>
      <Button variant="secondary" onclick={copyText} disabled={loading || !!error || !convertedText}>
        <Copy size={13} /> <span>{copyDone ? 'Copied!' : 'Copy'}</span>
      </Button>
      <Button variant="secondary" onclick={openSaveAs} disabled={loading || !!error || !convertedText}>
        <FolderOpen size={13} /> <span>Save As…</span>
      </Button>
      {#if onReplace}
        <Button variant="primary" onclick={doReplace} disabled={loading || !!error || !convertedText}>
          <Replace size={13} /> <span>Replace in editor</span>
        </Button>
      {/if}
    </div>
  {/snippet}
</Modal>

{#if savePickerOpen}
  <FilePickerModal
    mode="save"
    title={`Save ${outExtension === 'properties' ? '.properties' : 'YAML'} as`}
    extensions={outExtension === 'properties' ? ['properties'] : ['yaml', 'yml']}
    initialFilename={defaultFilename}
    onConfirm={onSaveAsPicked}
    onCancel={() => savePickerOpen = false}
  />
{/if}

<style>
  .scv-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    min-height: 0;
  }
  .scv-toolbar {
    padding: 8px 14px 0 14px;
    font-size: 11px;
    color: var(--text-secondary);
  }
  .scv-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .scv-alert-wrap { padding: 0 12px; }
  .scv-warnings ul {
    margin: 4px 0 0 16px;
    padding: 0;
    font-size: 11px;
  }
  .scv-preview {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0 12px 12px 12px;
    border: 1px solid var(--border-subtle);
    border-radius: 4px;
    overflow: hidden;
    background: var(--bg-base);
  }
  .scv-loading {
    padding: 24px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
  }

  .scv-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--border-subtle);
  }
  .scv-footer-spacer { flex: 1; }
</style>
