/**
 * Singleton store for host-wide commit preferences (persisted in
 * `~/.config/arbor/config.toml`). Currently only holds the global
 * commit-message template fallback; the per-repo native `commit.template`
 * still takes priority and is read directly from git via `getGitCommitTemplate`.
 */
import { getCommitConfig, setCommitConfig } from '$lib/ipc/config';

function createCommitConfigStore() {
  let templateGlobal = $state<string>('');
  let loaded         = $state(false);

  async function loadConfig() {
    try {
      const cfg = await getCommitConfig();
      templateGlobal = typeof cfg.template_global === 'string' ? cfg.template_global : '';
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults; next call will retry.
    }
  }

  function persist() {
    void setCommitConfig({ template_global: templateGlobal }).catch(() => {});
  }

  function setTemplateGlobal(v: string) {
    if (templateGlobal === v) return;
    templateGlobal = v;
    persist();
  }

  return {
    get templateGlobal() { return templateGlobal; },
    get loaded()         { return loaded; },
    loadConfig,
    setTemplateGlobal,
  };
}

export const commitConfigStore = createCommitConfigStore();
