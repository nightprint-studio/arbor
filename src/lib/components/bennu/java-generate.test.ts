import { describe, expect, it } from 'vitest';
import { buildGetter, buildSetter, buildWith, generateMembers, toCamelCase, type GenerateOptions } from './java-generate';
import { detectAccessors } from './java-accessors';

const camel: GenerateOptions = { className: 'Foo', fluent: false, naming: 'camelCase' };

describe('toCamelCase', () => {
  it('turns snake_case into camelCase', () => {
    expect(toCamelCase('gare_repository')).toBe('gareRepository');
    expect(toCamelCase('bandi_native_repository')).toBe('bandiNativeRepository');
  });

  it('leaves camelCase and single words alone', () => {
    expect(toCamelCase('gareRepository')).toBe('gareRepository');
    expect(toCamelCase('service')).toBe('service');
  });

  it('drops leading underscores and lowers screaming segments', () => {
    expect(toCamelCase('_foo_bar')).toBe('fooBar');
    expect(toCamelCase('MAX_SIZE')).toBe('maxSize');
  });
});

describe('accessors of a snake_case field', () => {
  const field = { name: 'gare_repository', type: 'GareRepository' };

  it('are named in camelCase when camelCase is chosen', () => {
    expect(buildGetter(field, camel)).toContain('public GareRepository getGareRepository()');
    expect(buildGetter(field, camel)).toContain('return gare_repository;');
    expect(buildSetter(field, camel)).toContain('public void setGareRepository(GareRepository gare_repository)');
    expect(buildWith(field, camel)).toContain('withGareRepository(');
  });

  it('keep snake_case when snake_case is chosen', () => {
    const snake: GenerateOptions = { ...camel, naming: 'snake_case' };
    expect(buildGetter(field, snake)).toContain('get_gare_repository()');
  });

  it('use the camel name for fluent accessors', () => {
    expect(buildGetter(field, { ...camel, fluent: true })).toContain('GareRepository gareRepository()');
  });

  it('a boolean field reads with is', () => {
    expect(buildGetter({ name: 'is_active', type: 'boolean' }, camel)).toContain('isIsActive()');
  });

  it('are found when already declared', () => {
    const source = 'class A {\n  public GareRepository getGareRepository() { return gare_repository; }\n}';
    expect(detectAccessors(source, ['gare_repository']).gare_repository.getter).toBe(true);
  });

  it('generate every member once', () => {
    const text = generateMembers('getters-setters', [field], camel);
    expect(text.match(/getGareRepository/g)).toHaveLength(1);
    expect(text.match(/setGareRepository/g)).toHaveLength(1);
  });
});
