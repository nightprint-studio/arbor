<script lang="ts">
  /**
   * The key hints under a find list — move, open, close. Lives in `internal/` because it draws
   * `Kbd`; `FindResults` (generic, `ui/`) takes it through its `footer` snippet.
   */
  import { CornerDownLeft } from 'lucide-svelte';
  import Kbd from './Kbd.svelte';

  interface Props {
    /** What Enter does with the highlighted hit. */
    openLabel?: string;
  }

  let { openLabel = 'open' }: Props = $props();
</script>

<Kbd keys={['↑']} size="sm" /><Kbd keys={['↓']} size="sm" /><span>move</span>
<span class="ff-open"><CornerDownLeft size={11} /> {openLabel}</span>
<span class="ff-spacer"></span>
<Kbd keys={['Esc']} size="sm" /><span>close</span>

<style>
  .ff-open { display: inline-flex; align-items: center; gap: 4px; }
  .ff-spacer { flex: 1; }
</style>
