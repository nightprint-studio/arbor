import { describe, expect, it } from 'vitest';
import { propertiesPath, yamlPath } from './config-path';

const labels = (p: { label: string }[]) => p.map((s) => s.label);

describe('yamlPath', () => {
  const doc = [
    'spring:',
    '  datasource:',
    '    url: jdbc:x',
    '    hikari:',
    '      maximum-pool-size: 5',
    'server:',
    '  port: 8080',
  ];

  it('lists every level above the key under the caret', () => {
    expect(labels(yamlPath(doc, 5))).toEqual(['spring', 'datasource', 'hikari', 'maximum-pool-size']);
    expect(labels(yamlPath(doc, 7))).toEqual(['server', 'port']);
  });

  it('answers for a root key with itself', () => {
    expect(labels(yamlPath(doc, 1))).toEqual(['spring']);
  });

  it('a blank line answers for the line above it', () => {
    expect(labels(yamlPath([...doc, ''], 8))).toEqual(['server', 'port']);
  });

  it('counts a list item as a level', () => {
    const list = ['servers:', '  - name: a', '    port: 1', '  - name: b', '    port: 2'];
    expect(labels(yamlPath(list, 5))).toEqual(['servers', '[1]', 'port']);
    expect(labels(yamlPath(list, 2))).toEqual(['servers', '[0]', 'name']);
  });

  it('follows a sequence written at the same indent as its key', () => {
    const list = ['hosts:', '- a', '- b'];
    expect(labels(yamlPath(list, 3))).toEqual(['hosts', '[1]']);
  });

  it('does not cross a document separator', () => {
    expect(labels(yamlPath(['a:', '  b: 1', '---', 'c:', '  d: 2'], 5))).toEqual(['c', 'd']);
  });
});

describe('propertiesPath', () => {
  it('splits the key on dots', () => {
    expect(labels(propertiesPath(['spring.datasource.url=jdbc:x'], 1))).toEqual(['spring', 'datasource', 'url']);
  });

  it('ignores a comment', () => {
    expect(propertiesPath(['# a.b=c'], 1)).toEqual([]);
  });
});
