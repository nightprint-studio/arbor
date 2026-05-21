import { invoke } from '@tauri-apps/api/core';
import type { ContainerDef } from '$lib/types/contribution';

/** Every container registered via `arbor.ui.container.register`. */
export async function listContainers(): Promise<ContainerDef[]> {
  return invoke<ContainerDef[]>('list_containers');
}

/** Single container by canonical key `"<plugin>::<id>"`, or null. */
export async function getContainer(key: string): Promise<ContainerDef | null> {
  return invoke<ContainerDef | null>('get_container', { key });
}
