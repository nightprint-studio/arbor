<script lang="ts">
  import { onMount } from 'svelte';
  import { getMrConfig, setMrConfig } from '$lib/ipc/config';
  import type { MrConfig } from '$lib/types/config';
  import SectionHeader from '$lib/components/shared/ui/SectionHeader.svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';

  let cfg = $state<MrConfig>({
    default_show_comments: true,
    default_show_bots:     true,
    default_show_activity: true,
  });
  let loaded = $state(false);
  let saved  = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;

  onMount(async () => {
    try { cfg = await getMrConfig(); }
    catch { /* keep defaults */ }
    finally { loaded = true; }
  });

  // Persist whenever any flag changes — but skip the initial load echo.
  // Without this guard the very first effect run would write the same values
  // back to disk on modal open, generating useless write churn.
  $effect(() => {
    if (!loaded) return;
    // Track all three flags
    const snapshot: MrConfig = {
      default_show_comments: cfg.default_show_comments,
      default_show_bots:     cfg.default_show_bots,
      default_show_activity: cfg.default_show_activity,
    };
    setMrConfig(snapshot)
      .then(() => {
        saved = true;
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(() => { saved = false; }, 1500);
      })
      .catch(() => { /* swallow — surface on next save attempt */ });
  });
</script>

<SectionHeader
  title="Merge Requests"
  description="Default visibility for the Activity timeline filter chips when opening a merge request or pull request. Each chip can still be toggled inside the modal — those toggles are session-only."
/>

<div class="card">
  <FormRow
    label="Show comments by default"
    description="Human-authored comments on the PR/MR thread."
  >
    <Toggle bind:checked={cfg.default_show_comments} />
  </FormRow>

  <FormRow
    label="Show bot comments by default"
    description="Comments from automated accounts (CI bots, security policy bots, dependency scanners)."
  >
    <Toggle bind:checked={cfg.default_show_bots} />
  </FormRow>

  <FormRow
    label="Show activity by default"
    description="System events: state changes, label edits, assignments, force-pushes, review requests."
  >
    <Toggle bind:checked={cfg.default_show_activity} />
  </FormRow>
</div>

{#if saved}
  <p class="saved-label">Saved</p>
{/if}
