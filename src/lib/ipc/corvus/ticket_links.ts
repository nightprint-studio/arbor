import type {
  CommitQueryItem,
  LinkedCommitRef,
  TicketLink,
  TicketLinkConfig,
  TicketLinksRepoConfig,
} from '$lib/types/corvus/git';
import { corvus } from '../rpc';

export function getCommitTicketLinks(
  tabId:   string,
  commits: CommitQueryItem[],
): Promise<Record<string, TicketLink[]>> {
  return corvus('get_commit_ticket_links', { tab_id: tabId, commits });
}

export function addTicketLink(
  tabId:    string,
  sha:      string,
  ticketId: string,
  tracker:  string,
): Promise<void> {
  return corvus('add_ticket_link', { tab_id: tabId, sha, ticket_id: ticketId, tracker });
}

export function removeTicketLink(
  tabId:    string,
  sha:      string,
  ticketId: string,
): Promise<void> {
  return corvus('remove_ticket_link', { tab_id: tabId, sha, ticket_id: ticketId });
}

export function getTicketLinkConfig(tabId: string): Promise<TicketLinkConfig> {
  return corvus('get_ticket_link_config', { tab_id: tabId });
}

export function setTicketLinkRepoConfig(
  tabId:  string,
  config: TicketLinksRepoConfig,
): Promise<void> {
  return corvus('set_ticket_link_repo_config', { tab_id: tabId, config });
}

/** Returns '' when valid, or an error message when the pattern is invalid or has no capture group. */
export function validateTicketRegex(pattern: string): Promise<string> {
  return corvus('validate_ticket_regex', { pattern });
}

export function checkNotesPushConfig(tabId: string): Promise<boolean> {
  return corvus('check_notes_push_config', { tab_id: tabId });
}

export function findCommitsForTicket(
  tabId:    string,
  ticketId: string,
): Promise<LinkedCommitRef[]> {
  return corvus('find_commits_for_ticket', { tab_id: tabId, ticket_id: ticketId });
}
