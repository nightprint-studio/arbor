# merula-transcribe

WAV → MIDI transcription for [merula](../../../design/merula), behind a swappable
`Transcriber` trait (D1's "implementazione sostituibile"). It depends on
`merula-audio` (for decoding) and `midly` (for the MIDI output), and **not**
on the language or pattern crates — it ends at MIDI, which the deterministic
`merula-import` converter then turns into `.merula`.

```text
DecodedAudio ──Transcriber──▶ midly::Smf ──▶ in memory → merula-import
                                          └─▶ on disk   → .mid
```

## Backends

| Backend | Notes |
|---|---|
| `DspTranscriber` | **Built-in, zero extra deps.** Monophonic pitch via YIN (`dsp::pitch`), drums via energy-onset detection + a zero-crossing-rate timbre cue mapped to GM keys (`dsp::onset`). Time-domain, model-free, runs on the mix → fast and rough. The fallback that always works. |
| `OnnxTranscriber` (feature `onnx`) | **basic-pitch** polyphonic pitch via ONNX (`onnx::basic_pitch`) + the DSP onset detector for drums. Far better than the DSP pitch on real, polyphonic audio. With a **Demucs** model installed and `split_stems` set, it first separates the mix (`onnx::demucs`, segmented overlap-add — the HT-Demucs FT drums specialist): drums are detected on the isolated kit and pitch on the drum-free signal (`mix − drums`). Inference runs on the **GPU via DirectML** when available, falling back to CPU automatically (a transparent speed-up — Demucs especially). onnxruntime is linked in (`ort` with `download-binaries` + `directml`); models download on demand. The shell selects this backend when basic-pitch is installed, else the DSP one. |

## Design notes

- **Interchange is `midly::Smf`** (owned / `'static`). The transient WAV → `.merula`
  path keeps it in memory and hands it straight to `merula-import`; the
  "Convert WAV to MIDI" command writes it to disk. No track-name metas (they'd
  borrow runtime data); the converter names parts itself.
- **Drums on channel 9** (GM "channel 10") so the converter splits them out.
- **Never panics on bad input** — returns `TranscribeError` (e.g. `NoContent` for
  silence), and reports progress through a callback for the shell's job UI.

Part of the merula crate stack: `merula-audio` → `merula-transcribe`.
See [`design/merula/architecture.md`](../../../design/merula/architecture.md).
