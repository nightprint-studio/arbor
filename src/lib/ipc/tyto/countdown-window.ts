/**
 * Shell commands for the pre-recording countdown overlay (`tyto-countdown`).
 *
 * DIRECT Tauri commands (in `src-tauri`, not the tyto-be rpc bridge). The store hides
 * Tyto, opens the overlay, and polls {@link takeCountdownDone} until the self-driven
 * overlay finishes (a pull model — reliable while Tyto is hidden, unlike a pushed
 * event). The overlay reads its second count via {@link getCountdownInit} and calls
 * {@link countdownFinished} when the digits reach zero.
 */
import { invoke } from '@tauri-apps/api/core';

/** Open the opaque countdown overlay counting down from `seconds`. */
export function openCountdownOverlay(seconds: number): Promise<void> {
  return invoke('open_countdown_overlay', { seconds });
}

/** The overlay window reads its second count once on mount (null = nothing armed). */
export function getCountdownInit(): Promise<number | null> {
  return invoke('get_countdown_init');
}

/** The overlay calls this when the digits reach zero (records completion + closes). */
export function countdownFinished(): Promise<void> {
  return invoke('countdown_finished');
}

/** The store polls this: `true` once the countdown has finished, else `false`. */
export function takeCountdownDone(): Promise<boolean> {
  return invoke('take_countdown_done');
}

/** Abort the countdown (error/cancel): close the overlay and restore Tyto. */
export function closeCountdownOverlay(): Promise<void> {
  return invoke('close_countdown_overlay');
}
