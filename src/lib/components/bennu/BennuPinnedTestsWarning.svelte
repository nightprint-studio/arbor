<script lang="ts">
  /**
   * The warning that **every** run started from here will run something other than what you picked.
   *
   * A pom that configures the Surefire plugin with a literal `<test>` makes a selection impossible:
   * Maven gives a value written in the pom precedence over the `-Dtest` that names it, so a class or
   * a case chosen here is read and discarded and the run does whatever the pom said. Verified on
   * Surefire 3.5.6 — `<test>TestSuite</test>` resolves to `TestSuite` however the command line is
   * written, while `<test>${anything}</test>` lets it through.
   *
   * Drawn wherever there is a ▷ to press — the catalogue in the sidebar and the run view — because it
   * is a fact about the button, not about the panel, and learning it from a result is learning it
   * after the wait. One component rather than one copy per panel: the sentence has to say the same
   * thing in both, and the property spellings must stay excluded from both.
   *
   * Nothing at all for a project that pins nothing, which is nearly all of them.
   */
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { testSelectionPinned } from '$lib/ipc/bennu/inspect';

  interface Props {
    /** The open project's root, or `null` — which draws nothing. */
    root: string | null;
  }

  let { root }: Props = $props();

  let pinned = $state<string | null>(null);

  $effect(() => {
    const path = root;
    if (!path) {
      pinned = null;
      return;
    }
    let cancelled = false;
    void testSelectionPinned(path)
      .then((value) => {
        if (!cancelled) pinned = value;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });
</script>

{#if pinned}
  <div class="pw">
    <Alert variant="warning" compact title="Every run here runs {pinned}">
      This project's pom pins the Surefire plugin to
      <code>&lt;test&gt;{pinned}&lt;/test&gt;</code>, and Maven gives a value written in the pom
      precedence over the one a run names — so picking a class or a case cannot take effect,
      whatever the command line says. Writing it as
      <code>&lt;test&gt;&#36;&#123;a.property&#125;&lt;/test&gt;</code>, with
      <code>{pinned}</code> as that property's default, keeps a plain <code>mvn test</code> running
      the suite and lets a single test be chosen from here.
    </Alert>
  </div>
{/if}

<style>
  .pw { flex: 0 0 auto; padding: 6px 6px 0; }
  /* The pom fragments read as code, at the panel's own size. */
  .pw :global(code) {
    font-family: var(--font-mono);
    font-size: 0.92em;
    background: color-mix(in srgb, var(--warning) 12%, transparent);
    border-radius: var(--radius-sm);
    padding: 0 3px;
  }
</style>
