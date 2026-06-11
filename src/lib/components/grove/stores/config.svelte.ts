/**
 * grove config store — reactive mirror of the typed `[grove]` settings
 * (`~/.config/arbor/config.toml`), routed through the backend (never
 * localStorage; Arbor hard rule #11). Mirrors the appearance-store shape:
 * defaults render immediately, `loadConfig()` overwrites from disk (called from
 * the grove window boot), setters persist.
 */

import { getGroveConfig, setGroveConfig, type GroveConfig, type GroveRenderConfig } from '$lib/ipc/grove';

export const LOG_LEVELS = ['trace', 'debug', 'info', 'warn', 'error'] as const;
export type GroveLogThreshold = (typeof LOG_LEVELS)[number];

const DEFAULT_RENDER: GroveRenderConfig = { sample_rate: 48_000, bit_depth: 'int24', tail_max_secs: 4.0 };

function createConfigStore() {
  let defaultOctave = $state(4);
  let defaultCps    = $state(0.5);
  let logThreshold  = $state<string>('info');
  let render        = $state<GroveRenderConfig>({ ...DEFAULT_RENDER });
  let vscoDir       = $state<string | null>(null);
  let packsDir      = $state<string | null>(null);
  let loaded        = $state(false);

  function snapshot(): GroveConfig {
    return {
      default_octave: defaultOctave,
      default_cps:    defaultCps,
      log_threshold:  logThreshold,
      render:         { ...render },
      vsco_dir:       vscoDir,
      packs_dir:      packsDir,
    };
  }

  function persist() { void setGroveConfig(snapshot()).catch(() => {}); }

  return {
    get defaultOctave() { return defaultOctave; },
    get defaultCps()    { return defaultCps; },
    get logThreshold()  { return logThreshold; },
    get render()        { return render; },
    get vscoDir()       { return vscoDir; },
    get packsDir()      { return packsDir; },
    get loaded()        { return loaded; },

    async loadConfig() {
      try {
        const cfg = await getGroveConfig();
        defaultOctave = cfg.default_octave;
        defaultCps    = cfg.default_cps;
        logThreshold  = cfg.log_threshold;
        render        = { ...cfg.render };
        vscoDir       = cfg.vsco_dir;
        packsDir      = cfg.packs_dir;
        loaded = true;
      } catch {
        // First-run / backend not ready — keep defaults; next call retries.
      }
    },

    setLogThreshold(level: string) {
      if (logThreshold === level) return;
      logThreshold = level;
      persist();
    },
    setDefaultOctave(n: number) {
      if (defaultOctave === n) return;
      defaultOctave = n;
      persist();
    },
    setDefaultCps(cps: number) {
      if (defaultCps === cps) return;
      defaultCps = cps;
      persist();
    },
    setVscoDir(dir: string | null) {
      if (vscoDir === dir) return;
      vscoDir = dir;
      persist();
    },
    setRender(next: GroveRenderConfig) {
      render = { ...next };
      persist();
    },
  };
}

export const configStore = createConfigStore();
