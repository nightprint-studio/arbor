import { describe, it, expect } from 'vitest';
import { isDockerfile, isStrutsConfig } from '$lib/utils/file-names';

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

describe('isStrutsConfig', () => {
  it('recognises the configuration a Struts project is actually made of', () => {
    for (const name of [
      'struts.xml',
      'struts-security.xml',
      'struts-plugin.xml',
      'struts-default.xml',
      'STRUTS.XML',
      'struts.properties',
    ]) {
      expect(isStrutsConfig(name), name).toBe(true);
    }
  });

  it('reads the basename out of a path', () => {
    expect(isStrutsConfig('/p/src/main/resources/struts.xml')).toBe(true);
    expect(isStrutsConfig('C:\\p\\src\\main\\resources\\struts-orders.xml')).toBe(true);
  });

  it('leaves alone the files that could belong to something else', () => {
    // `validation.xml` IS Struts's — and it is also what Bean Validation calls its config, so an
    // icon claiming the wrong framework is worse than the generic one. `strutsy.xml` is the
    // prefix-match trap: `struts-` and not `struts`.
    for (const name of [
      'validation.xml',
      'strutsy.xml',
      'struts.txt',
      'web.xml',
      'my-struts.xml',
      'application.properties',
    ]) {
      expect(isStrutsConfig(name), name).toBe(false);
    }
  });
});
