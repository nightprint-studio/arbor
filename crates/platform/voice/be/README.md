# voice-be

Headless backend (Model D) for local speech-to-text. Platform-level: any product window borrows it
through `ensure_product_backend("voice")` and calls `rpc("voice", method, params)`.

- Starts on first use; exits after `idle_minutes` without activity (models are dropped first).
  Armed wake word, a recording, a transcription or a download keep it alive.
- `capture` - cpal microphone input, mono, resampled to 16 kHz, on its own thread.
- `session` - dictation (push-to-talk / toggle / wake / test) and the always-on wake detector.
  Transcription runs on short-lived below-normal-priority threads, never a runtime worker.
- `engine` - whisper.cpp through `whisper-rs`, behind the **`whisper` cargo feature**.
- `models_cmds` - catalogue, hardware, verified download (job in the Jobs panel), removal.

## Building the speech engine

`whisper-rs` compiles whisper.cpp from source, which needs **CMake, a C++ toolchain and libclang**
(MSVC Build Tools on Windows, Xcode command line tools on macOS, `build-essential` + `cmake` + `libclang-dev` on Linux).
`bindgen` (used by `whisper-rs-sys`) must find the `libclang` shared library: on Windows set `LIBCLANG_PATH`
to a folder containing `libclang.dll`, otherwise the build stops with "Unable to find libclang".
Without the feature the crate still builds and runs (devices, meter, models, config); transcription
answers "built without the speech engine".

```
cargo build -p voice-be --features whisper          # CPU
cargo build -p voice-be --features metal            # macOS (GPU)
cargo build -p voice-be --features vulkan           # needs the Vulkan SDK (not default)
cargo build -p voice-be --features cuda             # needs the CUDA toolkit (not default)
```

`yarn backends:release` builds it with `whisper`. Linux also needs the ALSA development headers for cpal
(`libasound2-dev`).

## Events (`voice://…`)

`state`, `level`, `transcript`, `wake`, `disarmed`, `error`, `install`.

## Never stuck, always cancellable

Every transcription runs under a deadline (`plan::transcription_timeout`), a cancel flag (`voice_cancel`)
and `catch_unwind`; whisper.cpp polls both between compute steps through a raw abort callback (the
`set_abort_callback_safe` wrapper of whisper-rs 0.16 reads its closure back as the wrong type, so it is not used).
A final `voice://transcript` always follows, with `error` set (`"cancelled"` for a user stop).

## Live text

`live` runs one worker per push-to-talk / toggle dictation: the pump cuts the recording at pauses and the worker
transcribes the pieces in order, announcing `voice://partial {id, text}` (everything so far); the final transcript
carries `tail` (what was not announced) and `streamed`. It is on only when the model's real-time factor
(measured, else estimated) is under 0.7; otherwise `voice://state.note` says why and the text comes on release.

## Diagnostics (hidden RPCs)

`voice_transcribe_file {path, model, language?, threads?, short_context?, max_tokens?, cancel_after_ms?, timeout_ms?}`,
`voice_system_info` (CPU features whisper.cpp was built with), `voice_bench_matmul`, `voice_bench_encode`.
Events added: `config`, `partial`, `test`.
