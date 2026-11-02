/**
 * What is wrong with a Tomcat configuration — before it is launched.
 *
 * Every rule here exists because the alternative is a stack trace in a console, a long way from
 * the cause: a servlet package that does not match the container, a port that is taken, a JNDI name
 * the application looks up that no one defined. One function, so the form's badges, the field
 * errors and the guard on **Run** can never disagree about what is blocking.
 */

import type { RunConfig } from '$lib/stores/bennu/run-config.svelte';
import type { TomcatInstallation, TomcatProjectScan } from '$lib/ipc/bennu/tomcat';

export type TomcatTab = 'server' | 'deploy' | 'context' | 'launch';

export interface TomcatIssue {
  level: 'error' | 'warning';
  tab: TomcatTab;
  /** Which field to mark, when there is one. */
  field?: 'home' | 'httpPort' | 'shutdownPort' | 'contextPath' | 'docBase';
  message: string;
}

export interface IssueContext {
  /** The installation `home` points at, or `null` when it is not (yet) a readable Tomcat. */
  installation: TomcatInstallation | null;
  /** Why `home` could not be read, when it was asked about. */
  homeError: string;
  scan: TomcatProjectScan | null;
  /** The configured ports something is already listening on. */
  busyPorts: number[];
}

const validPort = (p: number) => Number.isInteger(p) && p >= 1 && p <= 65535;

export function tomcatIssues(cfg: RunConfig, ctx: IssueContext): TomcatIssue[] {
  const t = cfg.tomcat;
  const out: TomcatIssue[] = [];
  const add = (i: TomcatIssue) => out.push(i);

  if (!t.home.trim()) {
    add({ level: 'error', tab: 'server', field: 'home', message: 'Choose a Tomcat installation, or download one.' });
  } else if (!ctx.installation) {
    add({
      level: 'error',
      tab: 'server',
      field: 'home',
      message: ctx.homeError || 'That folder is not a Tomcat installation.',
    });
  } else if (ctx.scan?.namespace && ctx.scan.namespace !== ctx.installation.namespace) {
    add({
      level: 'error',
      tab: 'server',
      field: 'home',
      message:
        `The project uses ${ctx.scan.namespace}.servlet but Tomcat ${ctx.installation.version} ` +
        `speaks ${ctx.installation.namespace}.servlet — the application would not load.`,
    });
  }

  if (!validPort(t.httpPort)) {
    add({ level: 'error', tab: 'server', field: 'httpPort', message: 'Not a port: use a number from 1 to 65535.' });
  } else if (ctx.busyPorts.includes(t.httpPort)) {
    add({ level: 'error', tab: 'server', field: 'httpPort', message: `Port ${t.httpPort} is already in use.` });
  }
  if (!validPort(t.shutdownPort)) {
    add({ level: 'error', tab: 'server', field: 'shutdownPort', message: 'Not a port: use a number from 1 to 65535.' });
  } else if (t.shutdownPort === t.httpPort) {
    add({ level: 'error', tab: 'server', field: 'shutdownPort', message: 'The shutdown port must differ from the HTTP port.' });
  } else if (ctx.busyPorts.includes(t.shutdownPort)) {
    add({ level: 'error', tab: 'server', field: 'shutdownPort', message: `Port ${t.shutdownPort} is already in use.` });
  }

  if (!t.contextPath.startsWith('/')) {
    add({ level: 'error', tab: 'deploy', field: 'contextPath', message: 'The context path must start with “/”.' });
  }
  if (t.deploy === 'external' && !t.docBase.trim()) {
    add({ level: 'error', tab: 'deploy', field: 'docBase', message: 'Name the WAR or the exploded folder to serve.' });
  }
  if (t.deploy === 'live' && ctx.scan && !ctx.scan.webapp_dir) {
    add({
      level: 'error',
      tab: 'deploy',
      message: 'This module has no src/main/webapp, so there is nothing to serve. Pick the web module, or serve a built WAR.',
    });
  }

  if (t.contextMode === 'guided' && ctx.scan) {
    const missing = ctx.scan.requirements.filter((r) => !t.resources.some((x) => x.name === r.name));
    if (missing.length) {
      add({
        level: 'warning',
        tab: 'context',
        message: `The project looks up ${missing.length} JNDI name${missing.length === 1 ? '' : 's'} the context does not define: ${missing.map((m) => m.name).join(', ')}.`,
      });
    }
  }
  for (const r of t.resources) {
    if (r.kind === 'datasource' && r.name.trim() && !r.url.trim()) {
      add({ level: 'warning', tab: 'context', message: `${r.name}: the JDBC URL is empty.` });
    }
  }
  return out;
}

export const blocking = (issues: TomcatIssue[]) => issues.filter((i) => i.level === 'error');
