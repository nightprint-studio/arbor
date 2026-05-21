<!--
  Lazy — defers a heavy component's module evaluation until the gate first
  flips true. Once loaded the module stays cached, so subsequent open/close
  cycles incur no further import cost.

  Used in AppShell to keep big panels (DocsPanel, SettingsPanel) and Studio
  modals out of the initial V8 heap. Dev warmup pre-fires the same loaders
  in onMount so hot paths feel instant during local development; the `dev`
  branch is dead-code-eliminated by Rollup in release builds.

  Loose typing: each call site already passes a known loader + matching
  props, so a strict generic here would force every caller to annotate.
  The `any` shape on `Comp` keeps the spread `<C {...rest} />` callable
  whether the loaded component takes no props (Studio modals) or several
  (SettingsPanel, DocsPanel).
-->
<script lang="ts">
  type Props = {
    gate?: boolean;
    loader: () => Promise<{ default: any }>;
    [key: string]: any;
  };

  let { gate = true, loader, ...rest }: Props = $props();

  let Comp: any = $state(null);
  let started = false;

  $effect(() => {
    if (gate && !started) {
      started = true;
      loader().then((m) => { Comp = m.default; }).catch((err) => {
        started = false;
        console.error('Lazy: failed to load component', err);
      });
    }
  });
</script>

{#if Comp && gate}
  {@const C = Comp}
  <C {...rest} />
{/if}
