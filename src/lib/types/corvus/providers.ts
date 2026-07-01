// Mirror of the Rust crate `corvus-provider-descriptor` (serde camelCase).
//
// The single, domain-agnostic provider-connection contract. The frontend
// renders + drives ANY provider (issue tracker or git host) from these types
// alone — no per-provider (`linear`/`jira`/`github`/`gitlab`) knowledge.

/** Which settings section a descriptor belongs to. */
export type ProviderDomain = 'issue_tracker' | 'git_host';

export type FieldWidget = 'text' | 'secret' | 'url';

export type OAuthFlow = 'device' | 'redirect';

/** A predicate over another field's current value, evaluated client-side
 *  (`corvus_provider_descriptor::FieldMatch`, serde `tag="op", content="value"`). */
export type FieldMatch =
  | { op: 'nonEmpty' }
  | { op: 'endsWith'; value: string }
  | { op: 'equals';   value: string }
  | { op: 'contains'; value: string };

export interface FieldRule {
  /** Key of the field whose current value is tested. */
  field:   string;
  matches: FieldMatch;
}

export interface AuthField {
  key:          string;
  label:        string;
  widget:       FieldWidget;
  required:     boolean;
  /** Required *additionally* only when this rule matches the current values. */
  requiredWhen?: FieldRule;
  placeholder?:  string;
}

/** A hint under a fields form. `when` absent ⇒ default/fallback; otherwise the
 *  FE shows the first hint whose rule matches the current values. */
export interface FieldHint {
  text:  string;
  when?: FieldRule;
}

/** Tagged union (`type`): an OAuth button or a credential form. */
export type AuthMethodKind =
  | { type: 'oauth'; flow: OAuthFlow }
  | { type: 'fields'; fields: AuthField[]; hints: FieldHint[] };

export interface AuthMethod {
  id:    string;
  label: string;
  kind:  AuthMethodKind;
}

export interface ProviderDescriptor {
  id:           string;
  domain:       ProviderDomain;
  displayName:  string;
  description?: string;
  /** Brand icon id the FE's `BrandTile`/`BrandIcon` resolves. */
  icon:         string;
  /** CSS color/var for the connect CTA, e.g. `"var(--brand-linear)"`. */
  brandColor?:  string;
  /** Display order — `authMethods[0]` is the recommended/default action. */
  authMethods:  AuthMethod[];
}

export interface ProviderUserInfo {
  displayName: string;
  email?:      string | null;
  avatarUrl?:  string | null;
}

/** FE-facing auth status for any provider. */
export interface AuthStatus {
  authenticated: boolean;
  user?:         ProviderUserInfo | null;
  /** Connected-account sub-label (Jira tenant / self-hosted git host). */
  accountLabel?: string | null;
  /** Active auth method id (`"oauth"`/`"pat"`/`"basic"`/…). */
  method?:       string | null;
}

/** What `*_provider_start_oauth` returns — how the FE should proceed.
 *  Completion arrives via the `arbor://provider-oauth-done` event (by id). */
export type OAuthStart =
  | { type: 'redirect'; url: string }
  | { type: 'device'; userCode: string; verificationUri: string; expiresIn: number; interval: number };

/** Payload of the unified `arbor://provider-oauth-done` Tauri event. */
export interface ProviderOAuthDone {
  id:    string;
  ok:    boolean;
  error: string | null;
}
