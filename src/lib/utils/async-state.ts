/**
 * Runs an async operation while managing loading/error state.
 *
 * Usage inside a store:
 *   const result = await withLoading(
 *     v => { loading = v; },
 *     v => { error = v; },
 *     () => fetchSomething(tabId),
 *   );
 *   data = result ?? fallback;
 */
export async function withLoading<T>(
  setLoading: (v: boolean) => void,
  setError: (v: string | null) => void,
  fn: () => Promise<T>,
): Promise<T | null> {
  setLoading(true);
  setError(null);
  try {
    return await fn();
  } catch (e: unknown) {
    setError(String(e));
    return null;
  } finally {
    setLoading(false);
  }
}
