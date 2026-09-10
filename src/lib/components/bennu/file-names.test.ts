import { describe, it, expect } from 'vitest';
import { isDockerfile } from '$lib/utils/file-names';

describe('isDockerfile', () => {
  it('recognises the three spellings and Podman’s name for the same file', () => {
    for (const name of [
      'Dockerfile',
      'dockerfile',
      'Dockerfile.dev',
      'Dockerfile.jvm',
      'api.dockerfile',
      'Containerfile',
    ]) {
      expect(isDockerfile(name), name).toBe(true);
    }
  });

  it('reads the basename out of a path', () => {
    expect(isDockerfile('/srv/app/docker/Dockerfile')).toBe(true);
    expect(isDockerfile('C:\\src\\app\\Dockerfile.prod')).toBe(true);
  });

  it('leaves the neighbours alone', () => {
    // `.dev` is geode's playtest format and `docker-compose.yml` is YAML: both sit next to a
    // Dockerfile in the same folder, and both were the reason to match on the whole name.
    for (const name of ['scenario.dev', 'docker-compose.yml', 'dockerignore', '.dockerignore']) {
      expect(isDockerfile(name), name).toBe(false);
    }
  });
});
