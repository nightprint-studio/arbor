import { describe, expect, it } from 'vitest';
import { attachPortOf, DEFAULT_ATTACH_PORT, emptyConfig } from './run-config.svelte';

describe('attachPortOf', () => {
  it('reads a port from 1 to 65535', () => {
    expect(attachPortOf({ attachPort: '1' })).toBe(1);
    expect(attachPortOf({ attachPort: ' 5005 ' })).toBe(5005);
    expect(attachPortOf({ attachPort: '65535' })).toBe(65535);
  });

  it('reads an empty field as the default, anything else out of range as no port', () => {
    expect(attachPortOf({ attachPort: '' })).toBe(DEFAULT_ATTACH_PORT);
    for (const bad of ['0', '65536', '-1', '50.5', 'abc']) {
      expect(attachPortOf({ attachPort: bad })).toBeNull();
    }
  });

  it('starts a new Attach configuration on 5005 and leaves other kinds without a port', () => {
    expect(emptyConfig('Remote JVM', 'attach').attachPort).toBe('5005');
    expect(emptyConfig('App', 'application').attachPort).toBe('');
  });
});
