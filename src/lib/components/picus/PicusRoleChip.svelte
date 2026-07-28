<script lang="ts">
  /**
   * The folder-role badge — what a folder of scripts is FOR.
   *
   * The role decides the generator's defaults (an init script gets bare
   * statements, an update script a guarded block), so it is worth showing
   * wherever a destination or a folder appears. Update is tinted because it is
   * the role that carries version guards, and getting those wrong is the
   * expensive mistake.
   *
   * Like the dialect, a role is either **declared on this folder** or inherited
   * from the nearest ancestor that declares one — and the chip says which, for
   * the same reason: the user has to know whether this is the row to correct.
   */
  import Badge from '$lib/components/shared/ui/Badge.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { FOLDER_ROLE_LABELS, FOLDER_ROLE_SHORT, type FolderRole } from '$lib/types/picus';

  interface Props {
    role: FolderRole;
    size?: 'sm' | 'md';
    /** Short form (`init` / `upd`) for cramped rows. */
    terse?: boolean;
    /** The role came from an ancestor rather than from this folder. */
    inherited?: boolean;
    /** Project-relative path of the folder that declared it — named in the tooltip. */
    from?: string;
  }

  let { role, size = 'sm', terse = false, inherited = false, from = '' }: Props = $props();

  const HINTS: Record<FolderRole, string> = {
    init: 'Initialisation — runs on a fresh install, bare statements',
    update: 'Update — runs on an existing database, guarded by version',
    routines: 'Routines — packages, procedures, functions and triggers',
    data: 'Data — reference rows loaded alongside the schema',
    ignored: 'Ignored — not indexed and never written to',
  };

  const hint = $derived.by(() => {
    const base = HINTS[role];
    if (role === 'ignored' && inherited) {
      return `${base}. Nothing above this folder gives it a purpose either.`;
    }
    if (!inherited) return `${base} — declared on this folder`;
    return from ? `${base} — inherited from ${from}` : `${base} — inherited from a folder above`;
  });

  const tone = $derived(role === 'update' ? 'info' : 'neutral') as 'info' | 'neutral';
</script>

<span class="prc" class:prc-inherited={inherited} use:tooltip={hint}>
  <Badge variant="tone" {tone} {size} label={terse ? FOLDER_ROLE_SHORT[role] : FOLDER_ROLE_LABELS[role]} />
</span>

<style>
  .prc { display: inline-flex; }
  .prc-inherited { opacity: 0.66; }
</style>
