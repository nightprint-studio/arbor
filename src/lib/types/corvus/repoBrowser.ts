// TypeScript mirrors of src-tauri/src/remote_browser/mod.rs

export interface RemoteAccount {
  provider:     string;      // "github" | "gitlab"
  username:     string;
  display_name: string | null;
  avatar_url:   string | null;
}

export interface RemoteRepo {
  id:               string;
  name:             string;
  namespace:        string;   // org or user login
  full_name:        string;   // "namespace/name"
  description:      string | null;
  private:          boolean;
  default_branch:   string;
  language:         string | null;
  stars:            number;
  updated_at:       string;
  clone_url_https:  string;
  clone_url_ssh:    string | null;
  web_url:          string;
  provider:         string;
  is_fork:          boolean;
  is_archived:      boolean;
  size_kb:          number | null;
}

export interface RemoteTreeEntry {
  name:       string;
  path:       string;
  entry_type: 'file' | 'dir' | 'submodule' | 'symlink';
  size:       number | null;
}

export interface RemoteFileContent {
  path:       string;
  content:    string;          // UTF-8 text
  image_data: string | null;   // data:<mime>;base64,<data>
  size:       number;
  is_binary:  boolean;
  is_image:   boolean;
  mime_type:  string | null;
}

/** One node in the hierarchical namespace tree. */
export interface NamespaceTreeNode {
  segment:  string;             // last path segment, e.g. "X"
  fullPath: string;             // full path, e.g. "ORGANIZATION/X"
  repos:    RemoteRepo[];       // repos whose namespace == fullPath
  children: NamespaceTreeNode[];
  expanded: boolean;
}
