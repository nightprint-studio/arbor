/** What the shared workspace manager is given: workspaces and their projects, in the shape every product can produce. */
export interface WorkspaceManagerProject {
  /** The project's path: its identity. */
  root: string;
  /** Shown instead of the folder name, when the product has a better one. */
  name?: string;
}

export interface WorkspaceManagerItem {
  id: string;
  name: string;
  /** Index into the product's workspace palette (`colorVar`). */
  colorIdx: number;
  projects: WorkspaceManagerProject[];
}
