<script lang="ts">
  /**
   * Mounts the dialog of the generator the window opened. One modal per generator — each with its own
   * icon and form — and this is the only place that knows which is which, so adding a generator is a new
   * modal and one line here (the backend's registry already lists it for the palette).
   */
  import type { GeneratorHost } from '$lib/stores/bennu/generator-session.svelte';
  import BevyGenerateModal from './BevyGenerateModal.svelte';
  import BuilderGenerateModal from './BuilderGenerateModal.svelte';
  import ConverterGenerateModal from './ConverterGenerateModal.svelte';
  import DtoMapperGenerateModal from './DtoMapperGenerateModal.svelte';
  import HttpClientGenerateModal from './HttpClientGenerateModal.svelte';
  import JacksonGenerateModal from './JacksonGenerateModal.svelte';
  import SecurityGenerateModal from './SecurityGenerateModal.svelte';
  import TauriCommandGenerateModal from './TauriCommandGenerateModal.svelte';
  import TestSkeletonGenerateModal from './TestSkeletonGenerateModal.svelte';
  import ValueMembersGenerateModal from './ValueMembersGenerateModal.svelte';
  import CxfRestGenerateModal from './CxfRestGenerateModal.svelte';
  import SoapCallGenerateModal from './SoapCallGenerateModal.svelte';
  import RatatuiAppGenerateModal from './RatatuiAppGenerateModal.svelte';
  import RatatuiDialogGenerateModal from './RatatuiDialogGenerateModal.svelte';
  import RatatuiKeymapGenerateModal from './RatatuiKeymapGenerateModal.svelte';
  import RatatuiListGenerateModal from './RatatuiListGenerateModal.svelte';
  import RatatuiWidgetGenerateModal from './RatatuiWidgetGenerateModal.svelte';
  import RustHttpClientGenerateModal from './RustHttpClientGenerateModal.svelte';

  let { id, host, onClose }: { id: string; host: GeneratorHost; onClose: () => void } = $props();
</script>

{#if id === 'spring-security'}
  <SecurityGenerateModal {host} {onClose} />
{:else if id === 'attribute-converter'}
  <ConverterGenerateModal {host} {onClose} />
{:else if id === 'jackson-serde'}
  <JacksonGenerateModal {host} {onClose} />
{:else if id === 'http-client'}
  <HttpClientGenerateModal {host} {onClose} />
{:else if id === 'dto-mapper'}
  <DtoMapperGenerateModal {host} {onClose} />
{:else if id === 'builder'}
  <BuilderGenerateModal {host} {onClose} />
{:else if id === 'value-members'}
  <ValueMembersGenerateModal {host} {onClose} />
{:else if id === 'test-skeleton'}
  <TestSkeletonGenerateModal {host} {onClose} />
{:else if id === 'tauri-command'}
  <TauriCommandGenerateModal {host} {onClose} />
{:else if id === 'bevy-skeleton'}
  <BevyGenerateModal {host} {onClose} />
{:else if id === 'soap-call'}
  <SoapCallGenerateModal {host} {onClose} />
{:else if id === 'cxf-rest'}
  <CxfRestGenerateModal {host} {onClose} />
{:else if id === 'rust-http-client'}
  <RustHttpClientGenerateModal {host} {onClose} />
{:else if id === 'ratatui-app'}
  <RatatuiAppGenerateModal {host} {onClose} />
{:else if id === 'ratatui-widget'}
  <RatatuiWidgetGenerateModal {host} {onClose} />
{:else if id === 'ratatui-list'}
  <RatatuiListGenerateModal {host} {onClose} />
{:else if id === 'ratatui-keymap'}
  <RatatuiKeymapGenerateModal {host} {onClose} />
{:else if id === 'ratatui-dialog'}
  <RatatuiDialogGenerateModal {host} {onClose} />
{/if}
