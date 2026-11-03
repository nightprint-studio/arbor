/**
 * The Tomcat half of a run configuration, in the store's camelCase shape, and the one conversion
 * to and from the wire's snake_case.
 *
 * Kept out of `run-config.svelte.ts` for the reason the cargo form is out of the modal: a Tomcat
 * configuration shares a name, a module and an environment with every other kind and brings a
 * sizeable model of its own — a server, a deployment, a list of JNDI resources.
 */

import type { TomcatResourceDto, TomcatRunConfigDto } from '$lib/types/bennu';

export type ResourceKind = 'datasource' | 'env' | 'param';
export type PasswordMode = 'secret' | 'clear';

/** One entry of the generated `context.xml`. */
export interface TomcatResource {
  /** Stable id — what a stored password is filed under. Never shown. */
  id: string;
  kind: ResourceKind;
  /** JNDI name relative to `java:comp/env/` (`jdbc/Gestionale`), or a parameter's name. */
  name: string;
  driver: string;
  url: string;
  user: string;
  passwordMode: PasswordMode;
  /** `clear` mode only. In `secret` mode the value is held by the backend and this stays empty. */
  password: string;
  maxTotal: number;
  maxIdle: number;
  maxWaitMs: number;
  validationQuery: string;
  testOnBorrow: boolean;
  /** The Picus connection it was taken from, or is checked with; empty = none. */
  picusConnection: string;
  valueType: string;
  value: string;
  allowOverride: boolean;
}

export interface TomcatSettings {
  home: string;
  httpPort: number;
  shutdownPort: number;
  contextPath: string;
  deploy: 'live' | 'external';
  docBase: string;
  reloadable: boolean;
  jvmArgs: string;
  openBrowser: boolean;
  baseDir: string;
  installDir: string;
  contextMode: 'guided' | 'raw';
  contextRaw: string;
  resources: TomcatResource[];
  /** Jars, and folders of jars, copied into the server's `lib/` at launch. */
  libPaths: string[];
}

let seq = 0;
/** A resource id, unique within a session and stable once persisted. */
export function nextResourceId(): string {
  seq += 1;
  return `tr-${Date.now().toString(36)}-${seq}`;
}

/** A blank resource of `kind`, with the defaults a data source is usually wanted with. */
export function newResource(kind: ResourceKind, over: Partial<TomcatResource> = {}): TomcatResource {
  return {
    id: nextResourceId(),
    kind,
    name: kind === 'datasource' ? 'jdbc/Nuovo' : kind === 'env' ? 'app/valore' : 'parametro',
    driver: kind === 'datasource' ? 'org.postgresql.Driver' : '',
    url: '',
    user: '',
    passwordMode: 'secret',
    password: '',
    maxTotal: 20,
    maxIdle: 5,
    maxWaitMs: 10000,
    validationQuery: kind === 'datasource' ? 'SELECT 1' : '',
    testOnBorrow: true,
    picusConnection: '',
    valueType: 'java.lang.String',
    value: '',
    allowOverride: false,
    ...over,
  };
}

export function emptyTomcat(): TomcatSettings {
  return {
    home: '',
    httpPort: 8080,
    shutdownPort: 8005,
    contextPath: '/',
    deploy: 'live',
    docBase: '',
    reloadable: false,
    jvmArgs: '',
    openBrowser: true,
    baseDir: '',
    installDir: '',
    contextMode: 'guided',
    contextRaw: '',
    resources: [],
    libPaths: [],
  };
}

function resourceToDto(r: TomcatResource): TomcatResourceDto {
  return {
    id: r.id,
    kind: r.kind,
    name: r.name,
    driver: r.driver,
    url: r.url,
    user: r.user,
    password_mode: r.passwordMode,
    // A secret never goes to the repository's file — see the Rust contract.
    password: r.passwordMode === 'clear' ? r.password : '',
    max_total: r.maxTotal,
    max_idle: r.maxIdle,
    max_wait_ms: r.maxWaitMs,
    validation_query: r.validationQuery,
    test_on_borrow: r.testOnBorrow,
    picus_connection: r.picusConnection,
    value_type: r.valueType,
    value: r.value,
    allow_override: r.allowOverride,
  };
}

function resourceFromDto(d: Partial<TomcatResourceDto>): TomcatResource {
  const kind: ResourceKind = d.kind === 'env' || d.kind === 'param' ? d.kind : 'datasource';
  return newResource(kind, {
    id: d.id || nextResourceId(),
    name: d.name ?? '',
    driver: d.driver ?? '',
    url: d.url ?? '',
    user: d.user ?? '',
    passwordMode: d.password_mode === 'clear' ? 'clear' : 'secret',
    password: d.password ?? '',
    maxTotal: d.max_total ?? 20,
    maxIdle: d.max_idle ?? 5,
    maxWaitMs: d.max_wait_ms ?? 10000,
    validationQuery: d.validation_query ?? '',
    testOnBorrow: d.test_on_borrow ?? true,
    picusConnection: d.picus_connection ?? '',
    valueType: d.value_type || 'java.lang.String',
    value: d.value ?? '',
    allowOverride: d.allow_override ?? false,
  });
}

export function tomcatToDto(t: TomcatSettings): TomcatRunConfigDto {
  return {
    home: t.home,
    http_port: t.httpPort,
    shutdown_port: t.shutdownPort,
    context_path: t.contextPath,
    deploy: t.deploy,
    doc_base: t.docBase,
    reloadable: t.reloadable,
    jvm_args: t.jvmArgs,
    open_browser: t.openBrowser,
    base_dir: t.baseDir,
    install_dir: t.installDir,
    context_mode: t.contextMode,
    context_raw: t.contextRaw,
    resources: t.resources.map(resourceToDto),
    lib_paths: t.libPaths,
  };
}

/** Tolerant of a hand-edited file with keys missing: absent means the default, not a crash. */
export function tomcatFromDto(d: Partial<TomcatRunConfigDto> | undefined): TomcatSettings {
  const base = emptyTomcat();
  if (!d) return base;
  return {
    home: d.home ?? base.home,
    httpPort: d.http_port ?? base.httpPort,
    shutdownPort: d.shutdown_port ?? base.shutdownPort,
    contextPath: d.context_path ?? base.contextPath,
    deploy: d.deploy === 'external' ? 'external' : 'live',
    docBase: d.doc_base ?? '',
    reloadable: d.reloadable ?? false,
    jvmArgs: d.jvm_args ?? '',
    openBrowser: d.open_browser ?? true,
    baseDir: d.base_dir ?? '',
    installDir: d.install_dir ?? '',
    contextMode: d.context_mode === 'raw' ? 'raw' : 'guided',
    contextRaw: d.context_raw ?? '',
    resources: (d.resources ?? []).map(resourceFromDto),
    libPaths: d.lib_paths ?? [],
  };
}

/** The Tomcat settings of a whole configuration after a partial edit of the Tomcat half. */
export function withTomcat(current: TomcatSettings, patch: Partial<TomcatSettings>): TomcatSettings {
  return { ...current, ...patch };
}
