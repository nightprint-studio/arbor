<script lang="ts">
  /**
   * The folder-role badge — what a folder of scripts is FOR.
   *
   * The role decides the generator's defaults (an init script gets bare
   * statements, an update script a guarded block), so it is worth showing
   * wherever a destination or a folder appears. Update is tinted because it is
   * the role that carries version guards, and getting those wrong is the
   * expensive mistake.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { FOLDER_ROLE_LABELS, FOLDER_ROLE_SHORT, type FolderRole } from '$lib/types/picus';

  interface Props {
    role: FolderRole;
    size?: 'sm' | 'md';
    /** Short form (`init` / `upd`) for cramped rows. */
    terse?: boolean;
  }

  let { role, size = 'sm', terse = false }: Props = $props();

  const HINTS: Record<FolderRole, string> = {
    init: 'Initialisation — runs on a fresh install, bare statements',
    update: 'Update — runs on an existing database, guarded by version',
    routines: 'Routines — packages, procedures, functions and triggers',
    data: 'Data — reference rows loaded alongside the schema',
    ignored: 'Ignored — not indexed and never written to',
  };

  const tone = $derived(
    role === 'update' ? 'info' : role === 'ignored' ? 'neutral' : 'neutral',
  ) as 'info' | 'neutral';
</script>

<span use:tooltip={HINTS[role]}>
  <Badge variant="tone" {tone} {size} label={terse ? FOLDER_ROLE_SHORT[role] : FOLDER_ROLE_LABELS[role]} />
</span>
