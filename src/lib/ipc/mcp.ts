/**
 * MCP (AI tool surface) IPC — thin wrappers over the keep-shell commands.
 *
 * All of these stay Tauri commands rather than routing through the generic `rpc`
 * bridge: they start and stop a listener, answer an in-process consent prompt, and
 * read an in-memory log. None of it belongs to a product backend.
 */

import { invoke } from '@tauri-apps/api/core';
import type {
  McpActivityLog, McpClients, McpConfig, McpProgramTools, McpStatus,
} from '$lib/types/mcp';

/** Current settings. */
export const getMcpConfig = () => invoke<McpConfig>('get_mcp_config');

/** Persist settings and reconcile the endpoint immediately. Returns the new status. */
export const setMcpConfig = (config: McpConfig) => invoke<McpStatus>('set_mcp_config', { config });

/** Whether the endpoint is up, on which port, with which token. */
export const getMcpStatus = () => invoke<McpStatus>('get_mcp_status');

/**
 * Answer a pending consent prompt. `remember` grants the tool for the rest of this
 * run only — it is never written to disk.
 *
 * Resolves `false` when nothing was waiting (the prompt already timed out).
 */
export const respondMcpConsent = (id: string, tool: string, allow: boolean, remember: boolean) =>
  invoke<boolean>('mcp_consent_respond', { id, tool, allow, remember });

/** Mint a new bearer token; every client on the old one stops working. */
export const regenerateMcpToken = () => invoke<McpStatus>('mcp_regenerate_token');

/** Drop every "allow for this session" grant. */
export const revokeMcpSessionGrants = () => invoke<void>('mcp_revoke_session_grants');

/**
 * Every tool Arbor can expose, program by program.
 *
 * Slow on first call: reading a backend's inventory means starting it.
 */
export const getMcpTools = () => invoke<McpProgramTools[]>('get_mcp_tools');

/** Who has connected this run, and whether anything is listening now. */
export const getMcpClients = () => invoke<McpClients>('get_mcp_clients');

/** The call log, newest first, with the run that is reading it. */
export const getMcpAudit = () => invoke<McpActivityLog>('get_mcp_audit');

/** Forget the call log. */
export const clearMcpAudit = () => invoke<void>('clear_mcp_audit');
