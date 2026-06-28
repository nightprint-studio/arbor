import { corvus } from './rpc';
import type { ContainerDef } from '$lib/types/contribution';

/** Every container registered via `arbor.ui.container.register`. */
export async function listContainers(): Promise<ContainerDef[]> {
  return corvus<ContainerDef[]>('list_containers');
}

/** Single container by canonical key `"<plugin>::<id>"`, or null. */
export async function getContainer(key: string): Promise<ContainerDef | null> {
  return corvus<ContainerDef | null>('get_container', { key });
}
