import { describe, it, expect } from 'vitest';
import type { ProjectModule } from '$lib/ipc/bennu/modules';
import {
  groupByProject, hasEcosystem, moduleChoice, modulesFromKind, moduleOfFile, moduleRefs, ofKinds, resolveTarget, toolRootOf,
} from './project-modules';

const TAURI = 'C:/w/tauri-app';
const JAVA = 'C:/w/shop';

const byRoot: Record<string, ProjectModule[]> = {
  [TAURI]: [{ kind: 'Cargo', dir: 'src-tauri' }, { kind: 'Node', dir: '.' }],
  [JAVA]: [{ kind: 'Maven', dir: '.' }, { kind: 'Node', dir: 'web' }],
};

const refs = moduleRefs([TAURI, JAVA], byRoot);

describe('toolRootOf', () => {
  it('joins a module folder to the project root, and leaves the root alone for "."', () => {
    expect(toolRootOf(TAURI, 'src-tauri')).toBe('C:/w/tauri-app/src-tauri');
    expect(toolRootOf(`${TAURI}/`, '.')).toBe(TAURI);
    expect(toolRootOf('C:\\w\\x', 'a\\b')).toBe('C:/w/x/a/b');
  });
});

describe('moduleRefs', () => {
  it('keeps workspace order and puts a project root module first', () => {
    expect(refs.map((r) => `${r.kind}@${r.dir}`)).toEqual([
      'Node@.', 'Cargo@src-tauri', 'Maven@.', 'Node@web',
    ]);
    expect(new Set(refs.map((r) => r.id)).size).toBe(refs.length);
  });
});

describe('hasEcosystem / ofKinds', () => {
  it('answers for the whole workspace, not the active project', () => {
    expect(hasEcosystem(refs, 'Cargo')).toBe(true);
    expect(hasEcosystem(refs, 'Gradle')).toBe(false);
    expect(ofKinds(refs, ['Maven', 'Cargo']).map((r) => r.dir)).toEqual(['src-tauri', '.']);
  });
});

describe('groupByProject', () => {
  it('groups consecutive modules of one project', () => {
    const groups = groupByProject(refs);
    expect(groups.map((g) => [g.root, g.modules.length])).toEqual([[TAURI, 2], [JAVA, 2]]);
  });
});

describe('moduleOfFile', () => {
  it('picks the deepest module containing the file', () => {
    expect(moduleOfFile(refs, `${TAURI}/src-tauri/src/main.rs`)?.kind).toBe('Cargo');
    expect(moduleOfFile(refs, `${TAURI}/src/App.svelte`)?.kind).toBe('Node');
    expect(moduleOfFile(refs, 'C:/w/elsewhere/x.rs')).toBeUndefined();
    expect(moduleOfFile(refs, null)).toBeUndefined();
  });
  it('is case-insensitive like the filesystem it runs on, and does not match a sibling prefix', () => {
    expect(moduleOfFile(refs, 'c:/W/Tauri-App/SRC-TAURI/lib.rs')?.kind).toBe('Cargo');
    expect(moduleOfFile(refs, `${TAURI}-extra/src-tauri/x.rs`)).toBeUndefined();
  });
});

describe('resolveTarget', () => {
  const build = ofKinds(refs, ['Maven', 'Cargo']);
  it('prefers the selection, then the active file, then the active project', () => {
    const maven = build.find((r) => r.kind === 'Maven')!;
    expect(resolveTarget({ refs: build, selectedId: maven.id, activeFile: `${TAURI}/src-tauri/a.rs` })).toBe(maven);
    expect(resolveTarget({ refs: build, activeFile: `${TAURI}/src-tauri/a.rs` })?.kind).toBe('Cargo');
    expect(resolveTarget({ refs: build, activeFile: `${JAVA}/web/x.ts`, activeRoot: JAVA })?.kind).toBe('Maven');
    expect(resolveTarget({ refs: build, activeRoot: JAVA })?.kind).toBe('Maven');
  });
  it('ignores a stale selection and falls back to the first module', () => {
    expect(resolveTarget({ refs: build, selectedId: 'gone' })?.kind).toBe('Cargo');
    expect(resolveTarget({ refs: [] })).toBeUndefined();
  });
});

describe('modulesFromKind', () => {
  it('mirrors the primary kind so the rail is steady before the backend answers', () => {
    expect(modulesFromKind('cargo')).toEqual([{ kind: 'Cargo', dir: '.' }]);
    expect(modulesFromKind('maven')).toEqual([{ kind: 'Maven', dir: '.' }]);
    expect(modulesFromKind('plain')).toEqual([]);
  });
});

describe('moduleChoice', () => {
  const build = ofKinds(refs, ['Maven', 'Cargo']);
  it('acts directly with one module, a pin or an active file in a module', () => {
    expect(moduleChoice({ refs: build.slice(0, 1) })).toMatchObject({ ask: false });
    expect(moduleChoice({ refs: build, selectedId: build[1].id })).toEqual({ ask: false, module: build[1] });
    expect(moduleChoice({ refs: build, activeFile: `${TAURI}/src-tauri/a.rs` })).toMatchObject({ ask: false });
  });
  it('asks only when nothing narrows several modules down', () => {
    expect(moduleChoice({ refs: build, activeFile: 'C:/elsewhere/x.rs', selectedId: 'stale' })).toEqual({ ask: true, modules: build });
  });
  it('has nothing to act on without modules', () => {
    expect(moduleChoice({ refs: [] })).toEqual({ ask: false, module: undefined });
  });
});
