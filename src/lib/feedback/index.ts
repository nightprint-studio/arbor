// Public mounting surface of the feedback subsystem (toasts, notifications,
// operations, jobs). Windows mount <FeedbackHost id="…" [main] /> once; the
// per-system stores keep their own import paths for direct consumers
// (StatusBar, JobsOverlay, OperationsOverlay, …).
export { default as FeedbackHost } from './FeedbackHost.svelte';
export { makeAccepts, acceptAll, type TargetAccepts } from './routing';
