<script lang="ts">
  /**
   * The products this repository installs, and which row of the version table
   * belongs to each.
   *
   * The problem this solves: a repository that ships more than one product records
   * a version per product, most often as rows of one table told apart by a column
   * (`MODULO = 'PORTALE'`). Which row a generated block should read and stamp is
   * then a property of **where the script is going** — and nothing in the SQL says
   * it, so the repository has to.
   *
   * Two halves, deliberately separate:
   *
   *  • here, what a product **is** — its name and its predicate;
   *  • on the folder (Ctrl+Shift+F), which product's scripts **live there**.
   *
   * Splitting them is what makes it worth having. Naming the product once at the
   * top of `PORTALE/` answers for every version folder underneath it, including
   * the ones created next month, and the predicate is written once rather than
   * retyped on every destination of every generation.
   *
   * Empty for the ordinary repository, which installs one thing — and then nothing
   * anywhere behaves differently.
   */
  import { Plus, Trash2 } from 'lucide-svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Button from '$lib/components/shared/ui/Button.svelte';
  import { picusSettingsStore } from '$lib/stores/picus/settings.svelte';
  import type { ProductSetting } from '$lib/ipc/picus/project';

  const products = $derived(picusSettingsStore.products);

  function patch(index: number, change: Partial<ProductSetting>) {
    picusSettingsStore.setProducts(
      products.map((p, i) => (i === index ? { ...p, ...change } : p)),
    );
  }

  function add() {
    picusSettingsStore.setProducts([...products, { name: '', versionFilter: '' }]);
  }

  function remove(index: number) {
    picusSettingsStore.setProducts(products.filter((_, i) => i !== index));
  }

  /** A name two rows claim — the lookup takes the first, so the second is inert. */
  const duplicates = $derived.by(() => {
    const seen = new Set<string>();
    const twice = new Set<string>();
    for (const p of products) {
      const key = p.name.trim().toLowerCase();
      if (!key) continue;
      if (seen.has(key)) twice.add(key);
      seen.add(key);
    }
    return twice;
  });
</script>

<div class="pp">
  {#if !products.length}
    <p class="pp-empty">
      This repository installs one product, so every generated block reads and stamps the same
      version row. Add a product only if it installs several into one version table.
    </p>
  {:else}
    <div class="pp-list">
      <div class="pp-head">
        <span>Product</span>
        <span>Selects its version row</span>
        <span></span>
      </div>
      {#each products as product, i (i)}
        {@const clash = duplicates.has(product.name.trim().toLowerCase())}
        <div class="pp-row">
          <Input
            value={product.name}
            placeholder="PORTALE"
            ariaLabel="Product name"
            error={clash ? 'Another product is already called this' : null}
            oninput={(v) => patch(i, { name: String(v) })}
          />
          <Input
            value={product.versionFilter}
            placeholder="MODULO = 'PORTALE'"
            ariaLabel="Predicate selecting this product's version row"
            oninput={(v) => patch(i, { versionFilter: String(v) })}
          />
          <Button
            variant="icon"
            size="xs"
            ariaLabel={`Remove ${product.name || 'this product'}`}
            tooltip={'Remove — folders naming it fall back to the project’s own row'}
            onclick={() => remove(i)}
          >
            {#snippet iconStart()}<Trash2 size={13} />{/snippet}
          </Button>
        </div>
        {#if clash}
          <!-- Said rather than silently dropped on save: two rows with one name is
               a predicate the user believes is applied and that never runs. -->
          <p class="pp-warn">
            Two products are called <code>{product.name.trim()}</code>. Only the first would ever
            be used; the other is dropped when this is saved.
          </p>
        {/if}
      {/each}
    </div>
  {/if}

  <div class="pp-actions">
    <Button variant="secondary" size="sm" onclick={add}>
      {#snippet iconStart()}<Plus size={13} />{/snippet}
      Add a product
    </Button>
    {#if products.length}
      <span class="pp-hint">
        Say which folders hold each product's scripts from the folder classifier —
        <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>. A folder that names none reads the
        project's own row.
      </span>
    {/if}
  </div>
</div>

<style>
  .pp { display: flex; flex-direction: column; gap: 10px; }

  .pp-empty { font-size: 11.5px; line-height: 1.55; color: var(--text-muted); max-width: 80ch; }

  .pp-list { display: flex; flex-direction: column; gap: 6px; }
  .pp-head,
  .pp-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr) 28px;
    gap: 8px;
    align-items: center;
  }
  .pp-head span {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .pp-warn {
    grid-column: 1 / -1;
    font-size: 11px;
    line-height: 1.45;
    color: var(--warning);
  }
  .pp-warn code { font-family: var(--font-code); }

  .pp-actions { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .pp-hint {
    flex: 1;
    min-width: 240px;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-muted);
  }
</style>
