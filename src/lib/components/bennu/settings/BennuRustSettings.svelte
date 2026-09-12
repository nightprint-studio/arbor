<script lang="ts">
  /**
   * Settings › Rust — what rust-analyzer runs after a save, and whether the crates.io index is read.
   *
   * A page of its own rather than a card on the language-server page: these are decisions about the
   * Rust toolchain, and the one about crates.io is not a language-server setting at all.
   */
  import { Package, ServerCog } from 'lucide-svelte';
  import FormRow from '$lib/components/shared/ui/FormRow.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import Toggle from '$lib/components/shared/ui/Toggle.svelte';
  import { bennuConfigStore } from '$lib/stores/bennu/config.svelte';
  import { bennuLspStore } from '$lib/stores/bennu/lsp.svelte';
  import type { CargoConfigDto, LspConfigDto } from '$lib/ipc/bennu/config';

  const cfg = $derived(bennuConfigStore.cfg);

  const lspCfg = $derived<LspConfigDto>(
    cfg?.lsp ?? {
      enabled: true,
      rust_check_command: 'check',
      disabled: [],
      server_paths: {},
      servers: [],
      background_idle_timeout_secs: 600,
    },
  );
  const cargoCfg = $derived<CargoConfigDto>(cfg?.cargo ?? { crates_io: true, index_ttl_hours: 24 });

  /** Whether a Rust server exists to configure at all — the check command is rust-analyzer's, and a
   *  setting for a server this machine has never had is a setting for nothing. */
  const hasRustServer = $derived(bennuLspStore.servers.some((x) => x.language === 'rust'));

  async function setCheckCommand(command: string) {
    await bennuConfigStore.patch({ lsp: { ...lspCfg, rust_check_command: command } });
    // It is an `initializationOptions` value, so it only takes effect on a fresh handshake. Saying
    // so beats a setting that appears to do nothing until the next time the app happens to restart.
    await bennuLspStore.reloadServers();
  }
</script>

<div class="section-header">
  <h2>Rust</h2>
  <p>What the Rust tooling is allowed to do — after a save, and over the network.</p>
</div>

{#if hasRustServer}
  <div class="card">
    <div class="card-section-title"><ServerCog size={12} /> rust-analyzer</div>
    <FormRow
      label="Diagnostics on save"
      description="What rust-analyzer runs after each save to produce the compiler's real diagnostics — types and borrows, as opposed to the syntactic ones the parser alone can see. Clippy is a superset: every cargo check error plus several hundred lints, at the cost of a slower build after every save. Takes effect on the next server start."
    >
      <Select
        value={lspCfg.rust_check_command || 'check'}
        options={[
          { value: 'check', label: 'cargo check — faster' },
          { value: 'clippy', label: 'cargo clippy — also the lints' },
        ]}
        onchange={(v) => void setCheckCommand(v)}
      />
    </FormRow>
  </div>
{:else}
  <div class="card">
    <div class="card-section-title"><ServerCog size={12} /> rust-analyzer</div>
    <p class="set-empty">
      Not installed on this machine, so there is nothing to configure yet — <strong>Language
      Servers</strong> has the install button and what it needs.
    </p>
  </div>
{/if}

<div class="card">
  <div class="card-section-title"><Package size={12} /> Cargo</div>
  <FormRow
    label="Check crates.io for newer versions"
    description="Reads the crates.io index to mark a dependency in Cargo.toml that is behind, and to offer a version list when adding one. Answers come from a cache on disk, refreshed at most once a day per crate. Off keeps Bennu entirely local — adding a dependency still works, cargo just picks the version."
  >
    <Toggle
      checked={cargoCfg.crates_io}
      onchange={(on) => void bennuConfigStore.patch({ cargo: { ...cargoCfg, crates_io: on } })}
      ariaLabel="Query the crates.io index"
    />
  </FormRow>
</div>
