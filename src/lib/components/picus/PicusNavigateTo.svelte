<script lang="ts">
  /**
   * What "go to" searches in Picus.
   *
   * The overlay itself is `shared/navigate/NavigateTo.svelte` — cross-product
   * chrome, and Bennu's classes/files/symbols box is the same component with
   * different categories. This file is the Picus half: which lists exist, what a
   * row looks like, and what opening one does.
   *
   * Three categories, and each answers a question the tree answers badly:
   *
   *  • **Scripts** — the reason it exists. A repository with a folder set per
   *    delivered version has eleven files called `4_13.sql`, and finding one by
   *    expanding folders is the interaction this replaces.
   *  • **Objects** — "where is `PPCOMMON_PROPERTIES` touched" is asked far more
   *    often than "what is in this directory", and it lands in the Inventory with
   *    the object in view.
   *  • **Connections** — cheap to include and the fastest way to switch.
   *
   * Nothing is loaded over IPC: all three lists are already in the stores, so the
   * box opens instantly and stays instant while typing.
   */
  import { FileCode2, Database, Layers } from 'lucide-svelte';
  import NavigateTo, { type NavigateCategory } from '../shared/navigate/NavigateTo.svelte';
  import { OBJECT_KIND_LABELS } from './PicusObjectKindIcon.svelte';
  import { picusProjectStore } from '$lib/stores/picus/project.svelte';
  import { picusTabsStore } from '$lib/stores/picus/tabs.svelte';
  import { connectionsStore } from '$lib/stores/picus/connections.svelte';
  import { engineLabel, type ScriptFile } from '$lib/types/picus';

  let { onClose }: { onClose: () => void } = $props();

  /** The directory part of a project-relative path — '' for a file at the root. */
  function directoryOf(path: string): string {
    const cut = path.lastIndexOf('/');
    return cut === -1 ? '' : path.slice(0, cut);
  }

  /**
   * Excluded scripts are left out.
   *
   * Not a filter for tidiness: an excluded file is one the repository has
   * declared is not its business, so opening it from here would be the one place
   * in the product that ignores that declaration.
   */
  const scripts = $derived(picusProjectStore.allFiles.filter((f) => !f.effectiveExcluded));

  function scriptTag(file: ScriptFile): string | undefined {
    const engine = file.effectiveEngine;
    return engine ? engineLabel(engine) : undefined;
  }

  const categories = $derived<NavigateCategory[]>([
    {
      id: 'files',
      label: 'Scripts',
      items: () =>
        scripts.map((file) => ({
          id: file.path,
          name: file.name,
          detail: directoryOf(file.path),
          icon: FileCode2,
          tag: scriptTag(file),
          onOpen: () =>
            picusTabsStore.openFile(file.path, file.name, file.effectiveEngine),
        })),
    },
    {
      id: 'objects',
      label: 'Objects',
      items: () =>
        picusProjectStore.inventory.map((object) => ({
          id: `${object.kind}/${object.name}`,
          name: object.name,
          // The kind is the detail rather than the path: an indexed object lives
          // in several files by definition, so there is no one path to show, and
          // "which of the four things called this" is answered by the kind.
          detail: OBJECT_KIND_LABELS[object.kind] ?? object.kind,
          // One icon for the category rather than one per kind: the row's icon
          // slot takes a plain component, and the kind is already spelled out
          // beside the name — saying it twice buys nothing.
          icon: Layers,
          tag: object.external ? 'read only' : undefined,
          onOpen: () => picusTabsStore.openInventory(),
        })),
    },
    {
      id: 'connections',
      label: 'Connections',
      items: () =>
        connectionsStore.rows.map((connection) => ({
          id: connection.id,
          name: connection.name,
          detail: `${connection.schema}@${connection.host}`,
          icon: Database,
          // `engine` is what the backend row carries; `dialect` is the UI's name
          // for it, and only the projected `Connection` has that field.
          tag: engineLabel(connection.engine),
          onOpen: () => connectionsStore.setActive(connection.id),
        })),
    },
  ]);
</script>

<NavigateTo {categories} title="Go to" {onClose} />
