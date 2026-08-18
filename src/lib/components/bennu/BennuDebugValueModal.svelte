<script lang="ts">
  /**
   * One value, whole, as RON-shaped text.
   *
   * ## Why this exists beside a perfectly good tree
   *
   * The variables tree is lazy for a good reason — a stopped program has an object graph, and walking
   * it eagerly would be a round trip per node for rows nobody looked at. But laziness has a cost that
   * only shows on real data: a struct with fifteen fields, four of which are structs, is **nineteen
   * disclosure triangles** before you can read the thing, and by the time it is open it does not fit
   * on screen. Reading a value should not be an exercise in clicking.
   *
   * So this is the other half of the same feature: the tree for looking *around*, this for looking
   * *at*. One request, the whole subtree, in a block of text you can read top to bottom, scroll,
   * search with the editor's own find, and paste into a bug report.
   *
   * ## Why RON
   *
   * Because it is a Rust value. RON keeps the three distinctions JSON throws away and that are
   * exactly the ones a debugger is opened for: a named struct is not a map, a tuple is not a list, and
   * an enum variant is a name rather than a tag field somebody invented. And Bennu already colours
   * `.ron`, so this is the real editor rather than a `<pre>`.
   *
   * It is RON-*shaped*, not RON-*exact*, and the footer says so: what arrives from a debugger is a
   * name, a rendered value and a list of children, not a type system, so the shape is inferred. It is
   * for reading, not for feeding to a parser.
   *
   * ## Read-only, and a snapshot
   *
   * Nothing here writes to the debuggee. And the text was rendered against **one stop** — it stays
   * readable after the program runs on, deliberately: you opened it to read a value, and having it
   * blank itself because something continued in the background would lose the thing you came for.
   */
  import { Braces } from 'lucide-svelte';
  import Modal from '$lib/components/shared/Modal.svelte';
  import ModalHeader from '$lib/components/shared/ModalHeader.svelte';
  import ModalFooter from '$lib/components/shared/ModalFooter.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import CopyButton from '$lib/components/shared/ui/CopyButton.svelte';
  import Spinner from '$lib/components/shared/ui/Spinner.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import { CodeEditor } from '$lib/components/shared/ui/code-editor';
  import { ronLanguage } from './languages';
  import { bennuDebugStore, type Inspect } from '$lib/stores/bennu/debug.svelte';

  let { inspect }: { inspect: Inspect } = $props();

  const lines = $derived(inspect.text ? inspect.text.split('\n').length : 0);
</script>

<Modal
  onClose={() => bennuDebugStore.closeInspect()}
  width="820px"
  height="600px"
  padBody={false}
  ariaLabel="Value"
>
  {#snippet header()}
    <ModalHeader onClose={() => bennuDebugStore.closeInspect()}>
      <Braces size={14} />
      <span class="modal-title">Value</span>
      <span class="dvm-name">{inspect.value.name}</span>
      {#if inspect.value.type_name}
        <span class="dvm-type">{inspect.value.type_name}</span>
      {/if}
    </ModalHeader>
  {/snippet}

  <div class="dvm-body">
    {#if inspect.loading}
      <!-- Shown from the click rather than after the answer: the walk is a round trip per node
           against a suspended program, and a control that does nothing for a second reads as one
           that did nothing at all. -->
      <div class="dvm-state"><Spinner size={13} /> Reading the value…</div>
    {:else if inspect.error}
      <div class="dvm-pad"><Alert variant="error" text={inspect.error} /></div>
    {:else}
      {#if inspect.truncated}
        <!-- Said, because a dump silently cut at a cap reads as a complete answer and would be
             quoted as one. -->
        <div class="dvm-pad">
          <Alert
            variant="warning"
            compact
            text="This value was too big or too deep to read all of it — what is below a cut is marked in place."
          />
        </div>
      {/if}
      <CodeEditor value={inspect.text} language={ronLanguage} readOnly lineNumbers wrap={false} />
    {/if}
  </div>

  {#snippet footer()}
    <ModalFooter>
      <span class="dvm-note">
        RON-shaped, for reading — a debugger reports values and children, not types.
      </span>
      {#if !inspect.loading && !inspect.error}
        <span class="dvm-count">
          {inspect.nodes} value{inspect.nodes === 1 ? '' : 's'} · {lines} line{lines === 1 ? '' : 's'}
        </span>
      {/if}
      <span class="dvm-spacer"></span>
      <CopyButton
        variant="inline"
        value={inspect.text}
        label="Copy"
        title="Copy the whole value"
        toastSuccess="Value copied"
      />
      <Button variant="primary" onclick={() => bennuDebugStore.closeInspect()}>Close</Button>
    </ModalFooter>
  {/snippet}
</Modal>

<style>
  /* The row this was opened from, in the header — the answer to "which value am I looking at",
     which stops mattering only until you have opened two. */
  .dvm-name { font-family: var(--font-code); font-size: 11.5px; color: var(--text-secondary); }
  .dvm-type { font-family: var(--font-code); font-size: 10.5px; color: var(--text-muted); }

  .dvm-body { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
  .dvm-pad { padding: 8px 10px 0; flex: 0 0 auto; }
  .dvm-state {
    display: flex; align-items: center; gap: 6px;
    padding: 10px; font-size: 11.5px; color: var(--text-muted);
  }
  .dvm-note { font-size: 10.5px; color: var(--text-disabled); }
  .dvm-count { font-size: 10.5px; color: var(--text-muted); }
  .dvm-spacer { flex: 1; }
</style>
