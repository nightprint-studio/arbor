<script lang="ts">
  /**
   * BennuSketchBanner — the small italic "Sketch — …" strip a mock tool panel
   * shows at the top of its body to flag that it isn't wired to a backend yet.
   * Shared by the mock tool windows (Maven, Services, Dependencies) so the note
   * reads identically everywhere and the one padding/type rhythm lives in one
   * place. MOCK affordance — drop the usages as each tool goes live.
   *
   * Bennu-local (a Bennu-specific concept), so it lives in components/bennu/, not
   * shared/internal/ — keeping shared/internal app-Arbor-generic.
   */
  let { text }: { text: string } = $props();
</script>

<div class="sketch-banner">{text}</div>

<style>
  .sketch-banner {
    padding: 6px 12px;
    font-size: var(--font-size-2xs);
    font-style: italic;
    color: var(--text-disabled);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
</style>
