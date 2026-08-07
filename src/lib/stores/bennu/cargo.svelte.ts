/**
 * Cargo state: the crate graph, the command catalogue, and the toolchain.
 *
 * Three things read by four surfaces — the Cargo tool window, the run-configuration editor, the
 * command palette and the ▶ button — which is why they live in a store rather than being fetched
 * where they are needed. The graph in particular is asked for by the panel on open, by the editor
 * every time it renders a crate picker, and by ▶ when it has to find the sole binary; reading it
 * once per project is the difference between a click and a filesystem walk per keystroke.
 *
 * ## What is cached and what is not
 *
 * The **command catalogue** and the **toolchain** are per session: the first is a constant table and
 * the second changes only when you install a component (hence {@link refreshToolchain}). The
 * **workspace** is per project and re-read on demand — a manifest is edited while the editor is
 * open, and a crate added to `members` should appear without reopening the window.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store pattern).
 */

import {
  cargoCommands, cargoToolchain, cargoWorkspace,
  type CargoCommandDef, type CargoCrate, type CargoToolchain, type CargoWorkspace,
} from '$lib/ipc/bennu/cargo';

function createCargoStore() {
  let workspace = $state<CargoWorkspace | null>(null);
  let commands = $state<CargoCommandDef[]>([]);
  let toolchain = $state<CargoToolchain | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  /** The root the loaded workspace belongs to, so a project switch is not mistaken for a reload. */
  let loadedRoot = '';
  /** The catalogue and the toolchain are per session; read once. */
  let sessionLoaded = false;

  return {
    get workspace() { return workspace; },
    get commands() { return commands; },
    get toolchain() { return toolchain; },
    get loading() { return loading; },
    get error() { return error; },

    /** The commands worth putting in the panel's front row. */
    get commonCommands() { return commands.filter((c) => c.common); },
    /** The rest — one click further in, so the panel is a tool rather than a reference card. */
    get otherCommands() { return commands.filter((c) => !c.common); },

    /** Every binary in the workspace, as `(crate, target)` pairs — what ▶ and the editor pick from. */
    get binaries() {
      return (workspace?.crates ?? []).flatMap((c) =>
        c.targets.filter((t) => t.kind === 'bin').map((t) => ({ crate: c, target: t })),
      );
    },

    /** The crate called `name`. */
    crateNamed(name: string): CargoCrate | null {
      return workspace?.crates.find((c) => c.name === name) ?? null;
    },

    /**
     * Read `root`'s crate graph, plus the session-wide catalogue and toolchain on the first call.
     *
     * A second call for the same root is a no-op unless `force` — the panel's effect re-runs on
     * things that are not "a different project", and re-walking the workspace each time would make
     * the panel the most expensive thing on screen.
     */
    async load(root: string, force = false): Promise<void> {
      if (!root) {
        this.reset();
        return;
      }
      if (root === loadedRoot && !force && workspace) return;
      loading = true;
      error = null;
      try {
        // The catalogue and the toolchain in parallel with the graph: none of the three depends on
        // the others, and the panel needs all of them before it can draw a row.
        const [ws, cmds, tc] = await Promise.all([
          cargoWorkspace(root),
          sessionLoaded ? Promise.resolve(commands) : cargoCommands(),
          sessionLoaded ? Promise.resolve(toolchain) : cargoToolchain(),
        ]);
        workspace = ws;
        commands = cmds;
        toolchain = tc;
        loadedRoot = root;
        sessionLoaded = true;
      } catch (e) {
        error = e instanceof Error ? e.message : String(e);
        // The root is NOT recorded on failure, so the next open retries rather than showing an
        // empty panel for the rest of the session.
      } finally {
        loading = false;
      }
    },

    /** Re-probe the toolchain — what to call after telling the user to install a component. */
    async refreshToolchain(): Promise<void> {
      toolchain = await cargoToolchain(true).catch(() => toolchain);
    },

    /** Forget everything about the open project (a switch to a non-Cargo one). */
    reset() {
      workspace = null;
      loadedRoot = '';
      error = null;
    },
  };
}

export const bennuCargoStore = createCargoStore();
