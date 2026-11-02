/**
 * What the Tomcat run-configuration form knows about the world outside the configuration: which
 * installations exist, what the project declares, which ports are taken, which passwords are stored.
 *
 * One object the tabs share, so they do not each ask the backend the same question, and so the
 * issues the badges show and the ones the fields mark are computed from the same facts. The
 * component that owns the form drives the refreshes from its effects; this holds the answers and the
 * sequence guards that keep a late reply from overwriting a newer one.
 */

import { listConnections, type ConnectionRow } from '$lib/ipc/picus/db';
import {
  tomcatBusyPorts,
  tomcatEnvironment,
  tomcatInspect,
  tomcatScan,
  tomcatSecretIds,
  type TomcatEnvironment,
  type TomcatInstallation,
  type TomcatProjectScan,
} from '$lib/ipc/bennu/tomcat';
import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
import { tomcatIssues, type TomcatIssue } from './tomcat-issues';

const sameFolder = (a: string, b: string) =>
  a.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase() === b.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();

export function createTomcatForm(root: () => string, config: () => RunConfig) {
  let env = $state<TomcatEnvironment | null>(null);
  let scan = $state<TomcatProjectScan | null>(null);
  let busy = $state<number[]>([]);
  let inspected = $state<TomcatInstallation | null>(null);
  let homeError = $state('');
  let stored = $state<string[]>([]);
  /** Picus's connections, or why there are none to offer. */
  let picus = $state<ConnectionRow[]>([]);
  let picusError = $state('');

  // A reply is applied only if no newer question was asked since — the form is edited while the
  // backend is still answering the previous keystroke.
  const seq = { env: 0, scan: 0, inspect: 0, ports: 0, secrets: 0 };

  const installation = $derived.by<TomcatInstallation | null>(() => {
    const home = config().tomcat.home.trim();
    if (!home) return null;
    const listed = env?.installs.find((i) => sameFolder(i.home, home));
    if (listed) return listed;
    return inspected && sameFolder(inspected.home, home) ? inspected : null;
  });

  const issues = $derived<TomcatIssue[]>(
    tomcatIssues(config(), { installation, homeError, scan, busyPorts: busy }),
  );

  return {
    get env() { return env; },
    get scan() { return scan; },
    get busy() { return busy; },
    get installation() { return installation; },
    get homeError() { return homeError; },
    get issues() { return issues; },
    get picus() { return picus; },
    get picusError() { return picusError; },
    /** Read Picus's connections. Not part of the form's first paint: it may have to start Picus. */
    async refreshPicus() {
      try {
        picus = await listConnections();
        picusError = '';
      } catch (e) {
        picus = [];
        picusError = e instanceof Error ? e.message : String(e);
      }
    },
    /** Whether a password is stored for the resource (never its value). */
    hasSecret(resourceId: string) { return stored.includes(resourceId); },
    markSecret(resourceId: string, has: boolean) {
      stored = has ? [...new Set([...stored, resourceId])] : stored.filter((id) => id !== resourceId);
    },

    async refreshEnv() {
      const mine = ++seq.env;
      try {
        const next = await tomcatEnvironment(root(), config().tomcat.installDir, config().tomcat.baseDir);
        if (mine === seq.env) env = next;
      } catch { /* the form still works without the list: it just offers nothing to pick */ }
    },

    async refreshScan() {
      const mine = ++seq.scan;
      try {
        const next = await tomcatScan(root(), config().module);
        if (mine === seq.scan) scan = next;
      } catch { if (mine === seq.scan) scan = null; }
    },

    /** Read the folder `home` names when it is not one of the downloaded installations. */
    async refreshInspect() {
      const mine = ++seq.inspect;
      const home = config().tomcat.home.trim();
      if (!home || env?.installs.some((i) => sameFolder(i.home, home))) {
        homeError = '';
        return;
      }
      try {
        const next = await tomcatInspect(home);
        if (mine !== seq.inspect) return;
        inspected = next;
        homeError = '';
      } catch (e) {
        if (mine !== seq.inspect) return;
        inspected = null;
        homeError = e instanceof Error ? e.message : String(e);
      }
    },

    async refreshPorts() {
      const mine = ++seq.ports;
      const t = config().tomcat;
      try {
        const next = await tomcatBusyPorts([t.httpPort, t.shutdownPort].filter((p) => Number.isInteger(p) && p > 0 && p < 65536));
        if (mine === seq.ports) busy = next;
      } catch { /* no answer is not "busy" */ }
    },

    async refreshSecrets() {
      const mine = ++seq.secrets;
      try {
        const next = await tomcatSecretIds(root(), config().id);
        if (mine === seq.secrets) stored = next;
      } catch { /* none known */ }
    },
  };
}

export type TomcatForm = ReturnType<typeof createTomcatForm>;
