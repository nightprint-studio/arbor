import { describe, it, expect } from 'vitest';
import type { TreeNode } from '$lib/types/bennu';
import type { MavenModel, MavenModule, MavenPlugin } from '$lib/ipc/bennu/maven-build';
import {
  absoluteId,
  decodeExpansion,
  directoryKeys,
  encodeExpansion,
  mavenKeys,
  mavenLiveKeys,
  relativeKey,
  scopedKey,
  staleKeys,
} from './tree-expansion';

function dir(path: string, children: TreeNode[] = []): TreeNode {
  return { path, name: path.split('/').pop() ?? path, is_dir: true, children } as unknown as TreeNode;
}
function file(path: string): TreeNode {
  return { path, name: path.split('/').pop() ?? path, is_dir: false, children: [] } as unknown as TreeNode;
}
function plugin(group_id: string, artifact_id: string, goals: string[] = []): MavenPlugin {
  return { group_id, artifact_id, version: '', prefix: '', goals, managed: false, offset: 0, line: 1 };
}
function module(artifact_id: string, dirName: string, plugins: MavenPlugin[] = []): MavenModule {
  return { artifact_id, dir: dirName, name: '', packaging: '', pom: `C:/p/${dirName}/pom.xml`, plugins };
}

describe('relativeKey / absoluteId', () => {
  it('keys a folder by its path under the root', () => {
    expect(relativeKey('C:/p', 'C:/p/core/src')).toBe('core/src');
    expect(absoluteId('C:/p', 'core/src')).toBe('C:/p/core/src');
  });

  it('survives native separators, a trailing slash and a different drive-letter case', () => {
    expect(relativeKey('C:\\p\\', 'c:\\p\\core\\')).toBe('core');
    expect(absoluteId('C:\\p\\', 'core')).toBe('C:/p/core');
  });

  it('has no key for the root itself or for anything outside it', () => {
    expect(relativeKey('C:/p', 'C:/p')).toBeNull();
    expect(relativeKey('C:/p', 'C:/p/')).toBeNull();
    expect(relativeKey('C:/p', 'C:/px/core')).toBeNull();
    expect(relativeKey('C:/p', 'D:/other')).toBeNull();
    expect(relativeKey('', 'C:/p/core')).toBeNull();
  });
});

describe('encode / decode', () => {
  it('round-trips, sorted', () => {
    const state = new Map<string, boolean>([
      ['project:b', true],
      ['maven:module/app/lifecycle', false],
      ['project:a', true],
    ]);
    const persisted = encodeExpansion(state);
    expect(persisted).toEqual({
      expanded: ['project:a', 'project:b'],
      collapsed: ['maven:module/app/lifecycle'],
    });
    expect(decodeExpansion(persisted)).toEqual(state);
  });

  it('reads an older session with no lists as nothing remembered', () => {
    expect(decodeExpansion(undefined).size).toBe(0);
    expect(decodeExpansion({}).size).toBe(0);
  });

  it('drops malformed entries, keeps unknown scopes, and lets collapsed win a contradiction', () => {
    const state = decodeExpansion({
      expanded: ['project:a', 'no-scope', 'project:', 'future:x', 42 as unknown as string],
      collapsed: ['project:a'],
    });
    expect([...state]).toEqual([
      ['project:a', false],
      ['future:x', true],
    ]);
  });
});

describe('staleKeys', () => {
  it('drops only the keys of the scope that finished loading', () => {
    const keys = ['project:gone', 'project:kept', 'maven:module/x', 'future:y'];
    expect(staleKeys(keys, 'project', new Set(['kept']))).toEqual(['project:gone']);
    expect(staleKeys(keys, 'maven', new Set())).toEqual(['maven:module/x']);
  });
});

describe('directoryKeys', () => {
  it('lists every directory under the root, at any depth, and no files', () => {
    const tree = dir('C:/p', [
      dir('C:/p/core', [dir('C:/p/core/src', [file('C:/p/core/src/A.java')])]),
      file('C:/p/pom.xml'),
    ]);
    expect([...directoryKeys('C:/p', tree)].sort()).toEqual(['core', 'core/src']);
    expect(directoryKeys('C:/p', null).size).toBe(0);
  });
});

describe('Maven keys', () => {
  it('names rows by module identity, not position', () => {
    const app = module('app', 'modules/app');
    expect(mavenKeys.module(app)).toBe('module/app');
    expect(mavenKeys.lifecycle(app)).toBe('module/app/lifecycle');
    expect(mavenKeys.plugins(app)).toBe('module/app/plugins');
    expect(mavenKeys.plugin(app, plugin('org.apache.maven.plugins', 'maven-surefire-plugin'))).toBe(
      'module/app/plugin/org.apache.maven.plugins:maven-surefire-plugin',
    );
  });

  it('falls back to the directory for a module with no artifactId', () => {
    expect(mavenKeys.module(module('', 'web'))).toBe('module/web');
    expect(mavenKeys.module(module('', ''))).toBe('module/.');
  });

  it('keeps every collapsible row a model draws, and nothing it does not', () => {
    const model: Pick<MavenModel, 'modules' | 'profiles'> = {
      profiles: [],
      modules: [
        module('root', '', [plugin('g', 'bound', ['run']), plugin('g', 'config-only')]),
        module('core', 'core'),
      ],
    };
    const live = mavenLiveKeys(model);
    expect(live.has('profiles')).toBe(false);
    expect(live.has('module/root')).toBe(true);
    expect(live.has('module/core/lifecycle')).toBe(true);
    expect(live.has('module/root/plugin/g:bound')).toBe(true);
    expect(live.has('module/root/plugin/g:config-only')).toBe(false);

    const withProfiles = mavenLiveKeys({ ...model, profiles: [{ id: 'dev', active_by_default: false, module: '' }] });
    expect(withProfiles.has('profiles')).toBe(true);
  });

  it('scopes a key under its tree', () => {
    expect(scopedKey('maven', mavenKeys.profiles)).toBe('maven:profiles');
  });
});
