//! Dictation by segments: while the user is still talking, the recording is cut at pauses and each
//! finished piece is transcribed in the background, so text appears as they speak. Everything here
//! is pure and driven by the caller's 20 ms frames.
//!
//! * [`SegmentCutter`] decides *where* to cut (a pause of ~600 ms after real speech, or a hard cap).
//! * [`Assembler`] joins the pieces back into one text, keeps the context handed to the next piece
//!   as `initial_prompt`, and remembers which text the user has already seen.

use std::ops::Range;

use crate::speech::{EndpointEvent, Endpointer, EndpointerConfig, FRAME_MS};

/// Samples per millisecond at 16 kHz.
const SAMPLES_PER_MS: usize = 16;
/// Silence that closes a segment.
pub const SEGMENT_PAUSE_MS: u32 = 600;
/// Speech kept before the first loud frame and after the last (whisper does better with a margin).
const MARGIN_MS: usize = 200;
/// A segment never grows beyond this: a monologue without pauses is cut anyway.
pub const MAX_SEGMENT_SECS: usize = 24;
/// Less speech than this in a segment is a click or a breath, not a word.
const MIN_VOICED_MS: u32 = 250;

/// Cuts a live recording into transcribable segments. Feed it one frame RMS at a time with
/// [`push_frame`](Self::push_frame); it answers with the sample range of a finished segment.
#[derive(Debug, Clone)]
pub struct SegmentCutter {
    endpoint: Endpointer,
    start_frames: u32,
    /// Samples consumed so far.
    pos: usize,
    /// Everything before this has been handed out already.
    cut_at: usize,
    /// Where the current utterance began (with margin), when inside one.
    speech_from: Option<usize>,
}

impl Default for SegmentCutter {
    fn default() -> Self {
        Self::new()
    }
}

impl SegmentCutter {
    pub fn new() -> Self {
        let cfg = EndpointerConfig { end_silence_ms: SEGMENT_PAUSE_MS, ..Default::default() };
        Self { endpoint: Endpointer::new(cfg), start_frames: cfg.start_frames, pos: 0, cut_at: 0, speech_from: None }
    }

    /// One frame of `frame_len` samples with the given RMS. `Some(range)` = a segment to transcribe.
    pub fn push_frame(&mut self, rms: f32, frame_len: usize) -> Option<Range<usize>> {
        self.pos += frame_len;
        match self.endpoint.push(rms) {
            EndpointEvent::SpeechStart => {
                let onset = self.pos.saturating_sub(self.start_frames as usize * frame_len);
                self.speech_from = Some(onset.saturating_sub(MARGIN_MS * SAMPLES_PER_MS).max(self.cut_at));
                None
            }
            EndpointEvent::SpeechEnd { voiced_ms } => {
                let from = self.speech_from.take().unwrap_or(self.cut_at);
                let trailing = (SEGMENT_PAUSE_MS as usize).saturating_sub(MARGIN_MS) * SAMPLES_PER_MS;
                let to = self.pos.saturating_sub(trailing).max(from);
                self.cut_at = self.pos;
                (voiced_ms >= MIN_VOICED_MS && to > from).then_some(from..to)
            }
            EndpointEvent::None => {
                let from = self.speech_from?;
                if self.pos - from >= MAX_SEGMENT_SECS * 1000 * SAMPLES_PER_MS {
                    self.speech_from = Some(self.pos);
                    self.cut_at = self.pos;
                    return Some(from..self.pos);
                }
                None
            }
        }
    }

    /// The recording ended after `total` samples: whatever speech has not been handed out yet.
    pub fn finish(&mut self, total: usize) -> Option<Range<usize>> {
        let from = self.speech_from.take()?;
        self.cut_at = total;
        (total > from && (total - from) / SAMPLES_PER_MS >= (MIN_VOICED_MS + FRAME_MS) as usize).then_some(from..total)
    }
}

/// Phrases Whisper invents out of near-silence; a segment that is only one of these is dropped.
const HALLUCINATIONS: &[&str] = &[
    "sottotitoli",
    "amara.org",
    "thanks for watching",
    "thank you for watching",
    "grazie per la visione",
    "grazie a tutti",
    "subtitles by",
    "[blank_audio]",
    "[musica]",
    "[music]",
];

/// A segment's text without Whisper's non-speech markers; `None` when nothing real is left.
pub fn clean_segment(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_marker = (trimmed.starts_with('[') && trimmed.ends_with(']')) || (trimmed.starts_with('(') && trimmed.ends_with(')'));
    let lower = trimmed.to_lowercase();
    if is_marker || HALLUCINATIONS.iter().any(|h| lower.contains(h)) {
        return None;
    }
    Some(trimmed.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// The text of a dictation as its segments arrive.
#[derive(Debug, Clone, Default)]
pub struct Assembler {
    parts: Vec<String>,
    /// How many parts the user has already been shown as live text.
    shown: usize,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a transcribed segment; returns its cleaned text, `None` when it held nothing.
    pub fn push(&mut self, raw: &str) -> Option<String> {
        let clean = clean_segment(raw)?;
        self.parts.push(clean.clone());
        Some(clean)
    }

    /// Everything so far.
    pub fn text(&self) -> String {
        self.parts.join(" ")
    }

    /// Mark everything so far as already delivered to the user.
    pub fn mark_shown(&mut self) {
        self.shown = self.parts.len();
    }

    /// The text that was not delivered live (the final tail).
    pub fn undelivered(&self) -> String {
        self.parts[self.shown.min(self.parts.len())..].join(" ")
    }

    /// Whether any live text went out (so the final result is a tail, not the whole).
    pub fn streamed(&self) -> bool {
        self.shown > 0
    }

    /// The last `max_chars` characters (whole words), the context for the next segment.
    pub fn prompt_tail(&self, max_chars: usize) -> String {
        let all = self.text();
        if all.chars().count() <= max_chars {
            return all;
        }
        let skip = all.chars().count() - max_chars;
        let tail: String = all.chars().skip(skip).collect();
        match tail.find(' ') {
            Some(i) => tail[i + 1..].to_string(),
            None => tail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 320;

    /// Feed `frames` frames of constant RMS, collecting the segments produced.
    fn feed(c: &mut SegmentCutter, rms: f32, frames: usize) -> Vec<Range<usize>> {
        (0..frames).filter_map(|_| c.push_frame(rms, FRAME)).collect()
    }

    #[test]
    fn a_pause_after_speech_closes_a_segment() {
        let mut c = SegmentCutter::new();
        assert!(feed(&mut c, 0.001, 30).is_empty());
        assert!(feed(&mut c, 0.1, 100).is_empty()); // 2 s of speech
        let cut = feed(&mut c, 0.001, 31); // 620 ms of silence
        assert_eq!(cut.len(), 1);
        let seg = &cut[0];
        // Starts a margin before the speech began (frame 30), ends a margin after it stopped.
        assert!(seg.start <= 30 * FRAME && seg.start >= 30 * FRAME - 4000, "{seg:?}");
        assert!(seg.end >= 130 * FRAME && seg.end <= 131 * FRAME + 3200, "{seg:?}");
    }

    #[test]
    fn a_short_pause_does_not_split() {
        let mut c = SegmentCutter::new();
        feed(&mut c, 0.1, 60);
        assert!(feed(&mut c, 0.001, 20).is_empty()); // 400 ms
        assert!(feed(&mut c, 0.1, 60).is_empty());
    }

    #[test]
    fn two_sentences_make_two_segments_and_a_tail() {
        let mut c = SegmentCutter::new();
        let mut segs = feed(&mut c, 0.1, 80);
        segs.extend(feed(&mut c, 0.001, 40));
        segs.extend(feed(&mut c, 0.1, 80));
        segs.extend(feed(&mut c, 0.001, 40));
        segs.extend(feed(&mut c, 0.1, 50)); // still talking when the key is released
        assert_eq!(segs.len(), 2);
        assert!(segs[1].start >= segs[0].end, "segments must not overlap: {segs:?}");
        let total = 80 * FRAME + 40 * FRAME + 80 * FRAME + 40 * FRAME + 50 * FRAME;
        let tail = c.finish(total).expect("unfinished speech is the tail");
        assert!(tail.start >= segs[1].end && tail.end == total);
    }

    #[test]
    fn silence_and_clicks_produce_nothing() {
        let mut c = SegmentCutter::new();
        assert!(feed(&mut c, 0.001, 400).is_empty());
        assert!(feed(&mut c, 0.3, 2).is_empty());
        assert!(feed(&mut c, 0.001, 100).is_empty());
        assert_eq!(c.finish(504 * FRAME), None);
    }

    #[test]
    fn a_monologue_without_pauses_is_cut_at_the_cap() {
        let mut c = SegmentCutter::new();
        let frames = MAX_SEGMENT_SECS * 50 + 10;
        let segs = feed(&mut c, 0.1, frames);
        assert_eq!(segs.len(), 1);
        assert!(segs[0].len() >= MAX_SEGMENT_SECS * 16_000);
    }

    #[test]
    fn markers_and_hallucinations_are_dropped() {
        assert_eq!(clean_segment("  Ciao   mondo \n"), Some("Ciao mondo".into()));
        assert_eq!(clean_segment("[BLANK_AUDIO]"), None);
        assert_eq!(clean_segment("(musica)"), None);
        assert_eq!(clean_segment("Sottotitoli creati dalla comunità Amara.org"), None);
        assert_eq!(clean_segment("   "), None);
    }

    #[test]
    fn assembler_joins_and_tracks_what_was_shown() {
        let mut a = Assembler::new();
        assert_eq!(a.push("Apri il file."), Some("Apri il file.".into()));
        assert_eq!(a.push("[BLANK_AUDIO]"), None);
        a.mark_shown();
        a.push(" Aggiungi un metodo ");
        assert_eq!(a.text(), "Apri il file. Aggiungi un metodo");
        assert_eq!(a.undelivered(), "Aggiungi un metodo");
        assert!(a.streamed());
        assert_eq!(Assembler::new().undelivered(), "");
        assert!(!Assembler::new().streamed());
    }

    #[test]
    fn prompt_tail_keeps_whole_words() {
        let mut a = Assembler::new();
        a.push("uno due tre quattro cinque sei sette");
        assert_eq!(a.prompt_tail(100), "uno due tre quattro cinque sei sette");
        let tail = a.prompt_tail(14);
        assert!(tail.len() <= 14 && "uno due tre quattro cinque sei sette".ends_with(&tail), "{tail}");
        assert!(!tail.starts_with("ei"), "must not start mid-word: {tail}");
    }
}
