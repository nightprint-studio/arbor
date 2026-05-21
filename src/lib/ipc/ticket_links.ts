import { invoke } from '@tauri-apps/api/core';
import type {
  CommitQueryItem,
  LinkedCommitRef,
  TicketLink,
  TicketLinkConfig,
  TicketLinksRepoConfig,
} from '$lib/types/git';

export function getCommitTicketLinks(
  tabId:   string,
  commits: CommitQueryItem[],
): Promise<Record<string, TicketLink[]>> {
  return invoke('get_commit_ticket_links', { tabId, commits });
}

export function addTicketLink(
  tabId:    string,
  sha:      string,
  ticketId: string,
  tracker:  string,
): Promise<void> {
  return invoke('add_ticket_link', { tabId, sha, ticketId, tracker });
}

export function removeTicketLink(
  tabId:    string,
  sha:      string,
  ticketId: string,
): Promise<void> {
  return invoke('remove_ticket_link', { tabId, sha, ticketId });
}

export function getTicketLinkConfig(tabId: string): Promise<TicketLinkConfig> {
  return invoke('get_ticket_link_config', { tabId });
}

export function setTicketLinkRepoConfig(
  tabId:  string,
  config: TicketLinksRepoConfig,
): Promise<void> {
  return invoke('set_ticket_link_repo_config', { tabId, config });
}

/** Returns '' when valid, or an error message when the pattern is invalid or has no capture group. */
export function validateTicketRegex(pattern: string): Promise<string> {
  return invoke('validate_ticket_regex', { pattern });
}

export function checkNotesPushConfig(tabId: string): Promise<boolean> {
  return invoke('check_notes_push_config', { tabId });
}

export function findCommitsForTicket(
  tabId:    string,
  ticketId: string,
): Promise<LinkedCommitRef[]> {
  return invoke('find_commits_for_ticket', { tabId, ticketId });
}
