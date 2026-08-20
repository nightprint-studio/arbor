<script lang="ts">
  /**
   * Bennu's driver for the shared rails dialog.
   *
   * All this file does is translate: rail buttons in, editor rows out, and the four edited
   * lists back into the shape `activity_bar.products.bennu` is stored in. The dragging, the
   * eye, the lock and the tabs are `shared/CustomizeRailsModal`, which Corvus uses too — the
   * point being that the two bars keep behaving identically without anybody maintaining that.
   *
   * The rails are passed in rather than re-derived here, because they are derived from the
   * open project's capabilities in `BennuWindow` and re-deriving them would be a second
   * definition of what is on the bar.
   */
  import CustomizeRailsModal, {
    type RailEditorRow, type RailEditorSection, type RailEditorTab,
  } from '$lib/components/shared/CustomizeRailsModal.svelte';
  import type { ActivityRailButton } from '$lib/components/shared/ui/ActivityBar.svelte';
  import type { ActivityBarSections } from '$lib/types/config';
  import { bennuRailsStore, BENNU_MANDATORY, type RailSection } from '$lib/stores/bennu/rails.svelte';
  import { mergeRailOrder, railOrderToConfig } from '$lib/utils/rail-order';

  let { rails, onClose }: {
    /** The four clusters exactly as the window is drawing them right now. */
    rails: Record<RailSection, ActivityRailButton[]>;
    onClose: () => void;
  } = $props();

  /** A rail button as a row: the tooltip is the label, because on the bar it already is. */
  function row(item: ActivityRailButton & { visible: boolean; mandatory: boolean }): RailEditorRow {
    return {
      id: item.id,
      label: item.tooltip ?? item.id,
      icon: item.icon ?? item.emoji,
      visible: item.visible,
      mandatory: item.mandatory,
    };
  }

  function sectionOf(id: RailSection, label: string, hint: string): RailEditorSection {
    return {
      id,
      label,
      hint,
      items: mergeRailOrder(rails[id], bennuRailsStore.saved(id), BENNU_MANDATORY).map(row),
    };
  }

  /** Everything visible, in the order the window would build it with nothing saved. */
  function defaults(id: RailSection): RailEditorRow[] {
    return rails[id].map((item) => ({
      id: item.id,
      label: item.tooltip ?? item.id,
      icon: item.icon ?? item.emoji,
      visible: true,
      mandatory: BENNU_MANDATORY.has(item.id),
    }));
  }

  const LEFT_TOP    = 'Tool windows';
  const LEFT_BOTTOM = 'Bottom dock';
  const RIGHT_TOP   = 'Inspection';
  const RIGHT_BOTTOM = 'Bottom dock';

  const tabs: RailEditorTab[] = [
    {
      id: 'left',
      label: 'Left',
      hint: 'The rail on the left edge — the tool windows, and the toggles for the bottom dock.',
      sections: [
        sectionOf('leftTop', LEFT_TOP, 'side panels — project, structure, dependencies'),
        sectionOf('leftBottom', LEFT_BOTTOM, 'build, run, problems, TODO, terminal'),
      ],
    },
    {
      id: 'right',
      label: 'Right',
      hint: 'The rail on the right edge — the build tool, the test catalogue, and the panels a framework asked for.',
      sections: [
        sectionOf('rightTop', RIGHT_TOP, 'maven or cargo, tests, trees'),
        sectionOf('rightBottom', RIGHT_BOTTOM, 'forms, and framework catalogues'),
      ],
    },
  ];

  function resetTab(tabId: string): RailEditorSection[] {
    return tabId === 'left'
      ? [
          { id: 'leftTop', label: LEFT_TOP, hint: 'side panels — project, structure, dependencies', items: defaults('leftTop') },
          { id: 'leftBottom', label: LEFT_BOTTOM, hint: 'build, run, problems, TODO, terminal', items: defaults('leftBottom') },
        ]
      : [
          { id: 'rightTop', label: RIGHT_TOP, hint: 'maven or cargo, tests, trees', items: defaults('rightTop') },
          { id: 'rightBottom', label: RIGHT_BOTTOM, hint: 'forms, and framework catalogues', items: defaults('rightBottom') },
        ];
  }

  async function save(edited: RailEditorTab[]): Promise<void> {
    const bySection = new Map<string, RailEditorRow[]>();
    for (const tab of edited) {
      for (const s of tab.sections) bySection.set(s.id, s.items);
    }
    // `railOrderToConfig` carries through the ids this dialog never showed — the Java tools
    // on a Cargo project, a catalogue this project found nothing for. Dropping them would
    // reset their arrangement every time you edit the bar from the other kind of project.
    const next: ActivityBarSections = {
      top_items:          railOrderToConfig(bySection.get('leftTop') ?? [],     bennuRailsStore.saved('leftTop'),     BENNU_MANDATORY),
      bottom_items:       railOrderToConfig(bySection.get('leftBottom') ?? [],  bennuRailsStore.saved('leftBottom'),  BENNU_MANDATORY),
      right_top_items:    railOrderToConfig(bySection.get('rightTop') ?? [],    bennuRailsStore.saved('rightTop'),    BENNU_MANDATORY),
      right_bottom_items: railOrderToConfig(bySection.get('rightBottom') ?? [], bennuRailsStore.saved('rightBottom'), BENNU_MANDATORY),
    };
    await bennuRailsStore.save(next);
  }
</script>

<CustomizeRailsModal
  {tabs}
  title="Customize Activity Bar"
  onSave={save}
  onResetTab={resetTab}
  {onClose}
/>
