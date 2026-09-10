<script lang="ts">
  /**
   * The facts about this project that make the ▷ in the Tests panel a lie — and, where there is
   * one, the button that fixes them.
   *
   * Two today, and they have the same shape: something outside the test you are looking at means
   * pressing Run will not do what pressing Run appears to do. One component rather than one per
   * fact, because they belong in the same place, must not stack into a wall, and a third one
   * should cost a block here rather than a fourth mount site in two panels.
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
   * thing in both, the property spellings must stay excluded from both, and — now — the fix must be
   * offered in both.
   *
   * ## Why the fix is a button and not something that just happens
   *
   * It edits a build file that everybody on the team builds from. Nothing here rewrites a pom
   * because it noticed something; it says what it would write, to which file, and waits. The
   * conversion is also planned twice — once to show, once to apply — so a pom somebody edited in
   * between is not silently reverted to the text this side was shown.
   *
   * ## The engine gap
   *
   * The JUnit Platform runs *engines*, one per dialect. A project migrated to Jupiter that forgot
   * `junit-vintage-engine` still compiles every JUnit 4 test it has, and Surefire simply never runs
   * them — the build is green and a hundred classes were skipped. Nothing reports it, because
   * Surefire reports on what it ran.
   *
   * Nothing at all for a project that pins nothing and wires its engines, which is nearly all of
   * them.
   */
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import ConfirmModal from '$lib/components/shared/ConfirmModal.svelte';
  import { Wand2 } from 'lucide-svelte';
  import { toastStore } from '$lib/feedback/stores/toasts.svelte';
  import {
    applySuiteProperty,
    planSuiteProperty,
    testEngineGap,
    testSelectionPinned,
    type EngineGap,
    type SuiteConversionFile,
  } from '$lib/ipc/bennu/inspect';
  import { reindex } from '$lib/ipc/bennu/nav';

  interface Props {
    /** The open project's root, or `null` — which draws nothing. */
    root: string | null;
  }

  let { root }: Props = $props();

  let pinned = $state<string | null>(null);
  /** The dialects this project has tests in and no engine to run them with. */
  let gaps = $state<EngineGap[]>([]);
  /** The planned conversion, which is also what opens the confirmation. */
  let plan = $state<SuiteConversionFile[] | null>(null);
  let busy = $state(false);

  /** Bumped after a successful conversion to re-ask whether the project is still pinned — the
   *  warning disappearing is the only visible proof the button did anything. */
  let generation = $state(0);

  $effect(() => {
    const path = root;
    // Read so the effect re-runs when a conversion lands.
    void generation;
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
    // Asked separately rather than folded into one call: the pin is read off the poms and is
    // instant, while this one is empty until the classpath has resolved — and a warning that
    // waited for the slower of the two would arrive after the run it is meant to precede.
    void testEngineGap(path)
      .then((found) => {
        if (!cancelled) gaps = found;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  });

  /** The property the conversion would introduce — `test` whenever the pom leaves it free, which
   *  is the one Surefire already documents and therefore the one that also works from a bare
   *  terminal. */
  const property = $derived(plan?.[0]?.property ?? 'test');
  const files = $derived(plan ?? []);

  async function propose() {
    if (!root) return;
    busy = true;
    try {
      const planned = await planSuiteProperty(root);
      if (planned.files.length === 0) {
        // The pin went away between the warning and the click — an edit in another window, or a
        // `git pull`. Nothing to do, and saying so beats a confirmation for an empty change.
        toastStore.show('This project no longer pins a test selector', 'info');
        generation += 1;
        return;
      }
      plan = planned.files;
    } catch (e) {
      toastStore.show(`Could not read the pom: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }

  async function convert() {
    if (!root) return;
    busy = true;
    try {
      const { written } = await applySuiteProperty(root);
      plan = null;
      generation += 1;
      toastStore.show(
        written.length === 1
          ? `Surefire's test selector is now \${${property}}`
          : `Rewrote ${written.length} poms — the test selector is now \${${property}}`,
        'success',
      );
      // The classpath and the module model are read off the poms, so the project has to be looked
      // at again before anything else is asked of it.
      void reindex(root).catch(() => {});
    } catch (e) {
      toastStore.show(`Could not write the pom: ${e}`, 'error');
    } finally {
      busy = false;
    }
  }
</script>

{#each gaps as gap (gap.framework)}
  <div class="pw">
    <Alert
      variant="warning"
      compact
      title="{gap.classes} {gap.framework} test {gap.classes === 1 ? 'class is' : 'classes are'} never run"
    >
      This project runs its tests on the JUnit Platform, which needs one <em>engine</em> per
      dialect — and <code>{gap.missing}</code> is not on the test classpath. {gap.sample}
      {gap.classes === 1 ? 'compiles and is' : 'and the others compile and are'} skipped in
      silence: Surefire reports on what it ran, so the build stays green. Adding
      <code>{gap.missing}</code> as a test dependency is the whole fix.
    </Alert>
  </div>
{/each}

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
      {#snippet actions()}
        <Button size="sm" variant="secondary" disabled={busy} onclick={propose}>
          {#snippet iconStart()}<Wand2 size={13} />{/snippet}
          Convert to a property
        </Button>
      {/snippet}
    </Alert>
  </div>
{/if}

{#if plan}
  <ConfirmModal
    title="Rewrite the test selector as a property"
    message={files.length === 1
      ? `${files[0].pom} will declare ${property} = ${files[0].suite}, and its <test> becomes \${${property}}.`
      : `${files.length} poms will declare ${property} and reference it from their <test>.`}
    detail={`A plain \`mvn test\` still runs ${pinned} — the property defaults to it. What changes is that -Dtest= reaches Surefire again, so one class or one case can be run from here, from a terminal, and from any other tool. Nothing else in the file is touched.`}
    variant="warning"
    confirmLabel={busy ? 'Writing…' : 'Rewrite the pom'}
    {busy}
    onConfirm={convert}
    onCancel={() => (plan = null)}
  />
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
