<!--
  StudioFooterRight — Save / Save As… split button for <StudioModal>.

  Drops into `<StudioModal>`'s `footerRight` snippet. Wrapper owns the
  actual save pipeline (push to host, mark clean, route through file
  picker for Save As) — this component just renders the split widget
  with consistent labels and disabled-state logic.
-->
<script lang="ts">
  import { Save, FolderOpen } from 'lucide-svelte';
  import SplitButton, { type SplitOption } from '../ui/SplitButton.svelte';
  import { basename } from './helpers';
  import type { StudioFooterDoc } from './studio-footer-types';

  interface Props {
    doc:         StudioFooterDoc;
    saving:      boolean;
    /** Display label for the "Save As…" entry. Default is "Save As…". */
    saveAsLabel?: string;

    onSave:      () => void | Promise<void>;
    onSaveAs:    () => void | Promise<void>;
  }

  let {
    doc,
    saving,
    saveAsLabel = 'Save As…',
    onSave,
    onSaveAs,
  }: Props = $props();

  const saveOptions: SplitOption[] = $derived([
    { id: 'save-as', label: saveAsLabel, icon: FolderOpen },
  ]);

  const saveDisabled = $derived(
    saving || (!doc.dirty && !!doc.sourcePath),
  );

  const saveSplitTooltip = $derived(
    doc.sourcePath
      ? (doc.dirty ? `Save to ${basename(doc.sourcePath)}` : 'Nothing to save')
      : 'No path bound — use Save As…',
  );
</script>

<SplitButton
  variant="primary"
  size="sm"
  direction="up"
  position="fixed"
  loading={saving}
  disabled={saveDisabled}
  onclick={() => void onSave()}
  onselect={(id) => { if (id === 'save-as') void onSaveAs(); }}
  options={saveOptions}
  tooltip={saveSplitTooltip}
>
  <Save size={14} />
  <span>{doc.sourcePath ? 'Save' : saveAsLabel}</span>
</SplitButton>
