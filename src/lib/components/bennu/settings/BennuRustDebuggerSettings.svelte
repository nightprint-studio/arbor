<script lang="ts">
  /**
   * Settings › Rust › Debugger — which adapter drives a native debug session.
   *
   * A JVM is debugged by the VM itself; a native binary is debugged by whatever **starts** it, and
   * the three adapters are not interchangeable in the way that matters: only CodeLLDB renders Rust's
   * own types, so a `Vec<T>` is its elements under one and a pointer and a length under the others.
   * That is why this is a setting and not a detail — and why a pinned adapter that is missing is
   * reported rather than quietly replaced.
   */
  import { Bug, FolderSearch } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';

  const cfg = $derived(bennuConfigStore.cfg);
  const adapter = $derived(cfg?.debug_adapter ?? '');
  const path = $derived(cfg?.debug_adapter_path ?? '');

  const ADAPTERS = [
    { value: '', label: 'Whichever is installed — CodeLLDB first' },
    { value: 'codelldb', label: 'CodeLLDB — renders Rust’s own types' },
    { value: 'lldb-dap', label: 'lldb-dap — LLVM’s own, on most Macs' },
    { value: 'gdb', label: 'GDB — 14 or newer, in DAP mode' },
  ];
</script>

<div class="section-header">
  <h2>Debugger</h2>
  <p>What starts a Rust binary when you debug it, and what it can tell you about the values inside.</p>
</div>

<div class="card">
  <div class="card-section-title"><Bug size={12} /> Debug adapter</div>
  <FormRow
    label="Adapter"
    description="Left to itself Bennu takes the first of CodeLLDB, lldb-dap and GDB that is installed. Pin one when the machine has several and they do not agree about what a value is — the difference is whether a Vec is its elements or a pointer and a length."
  >
    <Select
      value={adapter}
      options={ADAPTERS}
      ariaLabel="Debug adapter"
      onchange={(v) => void bennuConfigStore.patch({ debug_adapter: v })}
    />
  </FormRow>
  <FormRow
    label="Executable path"
    description="For an adapter installed somewhere the search does not look — a CodeLLDB inside a VS Code extension directory, a GDB built by hand. Empty searches PATH and the usual places."
  >
    <Input
      value={path}
      placeholder="leave empty to search PATH and the usual install locations"
      onchange={(v) => void bennuConfigStore.patch({ debug_adapter_path: v.trim() })}
    />
  </FormRow>
</div>

<div class="card">
  <div class="card-section-title"><FolderSearch size={12} /> What a session reads</div>
  <p class="set-hint">
    Neither LLDB nor GDB knows Rust's types on its own. CodeLLDB ships the formatters; for
    <code>lldb-dap</code> Bennu loads the two the <strong>Rust toolchain</strong> installs, the same
    pair <code>rust-lldb</code> loads, so a <code>String</code> is its text and an <code>Option</code>
    is <code>Some(3)</code>. Without a toolchain the variables tree says so at the top rather than
    showing you a control block. GDB reads the printers the binary names and is left to itself.
  </p>
  <p class="set-hint">
    A struct of your own has no formatter anywhere: what fills that gap is the summaries Bennu turns
    on, and the <strong>&#123;&#125;</strong> button on a row, which reads the whole value at once —
    see <em>Frames, values &amp; watches</em> in the documentation.
  </p>
</div>
