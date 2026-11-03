<script lang="ts">
  /**
   * What an unanswered question, confirmation or access request becomes once the run that asked it is over —
   * cancelled, timed out, or the conversation reopened later. Its buttons would reach no one, so they give way
   * to this line and the card stays as a record of what was asked.
   */
  import { Clock } from 'lucide-svelte';
</script>

<span class="stale"><Clock size={10} /> No longer waiting for an answer — the run that asked it has ended.</span>

<style>
  .stale { display: inline-flex; align-items: center; gap: 5px; font-size: var(--font-size-2xs); color: var(--text-muted); }
</style>
