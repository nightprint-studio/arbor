<script lang="ts">
  /**
   * Voice: dictation into Imber, the voice overlay, the speech models and the wake word.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
</script>

<span class="eyebrow">Imber</span>
<h1>Voice</h1>

<p class="doc-lead">
  Talk to Imber instead of typing. Speech is turned into text by a model that runs on this computer: the audio is never
  recorded to disk and never sent anywhere.
</p>

<h2>Three ways to speak</h2>
<table>
  <thead><tr><th>How</th><th>What happens</th></tr></thead>
  <tbody>
    <tr><td><strong>Microphone button</strong> in the message box</td><td>Hold it to talk and release to finish, or tap it once to start and again to stop. The text is inserted where the caret is, and with a model that keeps up it appears <em>while you are still talking</em> (see below). <kbd>Esc</kbd>, or a click on the spinner, cancels a recording or a transcription in progress.</td></tr>
    <tr><td><Kbd action="voice_push_to_talk" size="sm" /> (hold)</td><td>Opens the voice overlay, records while the keys are down and sends the sentence to Imber when you let go. The file and the selected lines are attached exactly as they are when you type.</td></tr>
    <tr><td><Kbd action="voice_push_to_talk" size="sm" /> (tap)</td><td>A quick tap leaves the microphone open instead; tap again to stop. The microphone button works the same way.</td></tr>
    <tr><td><strong>Wake word</strong></td><td>Say “Ehi Imber” or “Hey Imber”, then your request. See below.</td></tr>
  </tbody>
</table>

<h2>The voice overlay</h2>
<p>
  The window dims and Imber appears in the middle as a glowing orb that swells and sends out ripples with your voice, above a thin
  waveform. <em>Listening…</em> gives way to <em>Transcribing…</em> and <em>Thinking…</em>; what was understood is shown large as you
  speak, the words that have just arrived a little dimmer until the next piece settles them, and the file and the selected lines
  that go with the request appear as chips. When the sentence ends it is sent to Imber: the orb shrinks into a header and the answer
  streams into a card below it, with code and diagrams rendered as in the panel. Typing a follow-up, or choosing
  <strong>Continue in the panel</strong>, closes the overlay and opens the Imber panel on the same conversation. It can be turned off in
  Settings › Voice; the answer then simply appears in the panel.
</p>
<ul>
  <li><kbd>Enter</kbd> while listening stops the recording and sends what was said at once. <kbd>Esc</kbd>, a click on the dimmed area or the × cancels the recording or the transcription, and closes the overlay once there is an answer.</li>
  <li><kbd>Ctrl</kbd>+<kbd>Enter</kbd> in the follow-up field sends it to the panel.</li>
  <li>When nothing was heard the overlay offers <strong>Try again</strong>; when there is no speech engine, no model or no microphone access it explains what is missing and offers <strong>Open Voice settings</strong>.</li>
  <li>With animations switched off, or reduced motion requested by the system, the orb, the ripples and the waveform stay still; only fades remain.</li>
</ul>

<h2>Choosing a speech model</h2>
<p>
  Settings › Voice lists the models with their size on disk, how fast each will feel on this machine, how accurate it is and which
  languages it covers, and the seconds of work it needs per second of speech (estimated from your physical cores, memory and GPU,
  and replaced by the real figure once <em>Test it</em> or a dictation has measured it). One is marked <strong>Recommended for your PC</strong> (hover the badge for the reason): the most accurate one that keeps up with speech, worked out from the cores, the memory and
  whether a GPU is used, and from the language you chose: Italian and the other non-English languages never get the two smallest
  models, which are poor there. Without a GPU that means Small (or Base for English on a small CPU): the Medium, Large and Turbo
  models need many seconds of computing per phrase on a laptop CPU, so they are only recommended with GPU acceleration or when a measurement shows they keep up. The model in use is shown as <strong>In use</strong>; the first model you download is selected automatically, and choosing another with <em>Use</em> takes effect immediately.
</p>
<ul>
  <li><strong>Download</strong> asks first, showing the name, the size, where it comes from and the free disk space. The file is checked against its published SHA-256 before it is used, and appears in the Jobs panel while it downloads.</li>
  <li><strong>Test it</strong> records five seconds with that model (the button counts down, then shows <em>Transcribing…</em>; <strong>Stop</strong> cancels either phase), shows what it understood and the measured speed. The measurement is remembered and corrects the estimates for every model.</li>
  <li><strong>Use my own model file…</strong> picks a ggml Whisper file you already have.</li>
</ul>

<h2>Text while you speak</h2>
<p>
  With <strong>Live text while you speak</strong> on (Settings › Voice), a pause of about half a second finishes a piece of speech, which is
  transcribed in the background and inserted at once while you keep talking; when you stop, only the last piece is left to transcribe.
  Each piece is given the previous text as context. A model that would fall behind your speech on this PC (more than 0.7 s of work per second of
  speech) does not do this: the overlay says so and the text appears when you finish. Nothing can hang: every transcription has a time limit
  derived from the length of the recording and can be cancelled.
</p>

<h2>Vocabulary hints</h2>
<p>
  With <strong>Vocabulary hints</strong> on, the project name, the class being edited, the files open next to it and the words in your
  selection are given to the recogniser, so names come out spelled right.
</p>

<h2>Memory and CPU</h2>
<p>
  The speech engine is a separate process that starts the first time you use voice and exits after the idle time set in the settings
  (five minutes by default); at rest it uses no memory and no CPU. It appears in the processes list with its memory, whether it is
  idle or transcribing, and a way to stop it, and each download or transcription is listed as a job.
</p>

<h2>Wake word</h2>
<Callout variant="warning" title="Armed means the microphone is open">
  While the wake word is armed Arbor listens continuously and uses some CPU. Nothing is recorded to disk and no audio leaves your
  computer. A microphone mark in the status bar is shown for as long as it is on; click it to turn it off.
</Callout>
<ul>
  <li>It is <strong>off by default</strong> and never arms by itself after a restart. Arm and disarm it from Settings › Voice or the command palette (<em>Voice: arm wake word</em>).</li>
  <li>It listens only while Arbor is the focused application, unless <em>Also when the window is in the background</em> is on, and disarms itself after the number of minutes you set without being used.</li>
  <li>Loudness is checked all the time, which is nearly free. Only when someone speaks does a small speech model look at the first seconds for the phrase, and no more than a few times a minute.</li>
  <li>The phrase is matched loosely, so “hey amber” or “ei imbar” work; it must open the sentence. The list of phrases is yours to change.</li>
  <li>A pause (1.2 seconds by default) ends the request, which then opens the voice overlay like the shortcut does.</li>
</ul>

<h2>If it does not work</h2>
<ul>
  <li><strong>No microphone / access denied</strong>: Windows — Settings › Privacy &amp; security › Microphone; macOS — System Settings › Privacy &amp; Security › Microphone; Linux — check that PulseAudio or PipeWire exposes an input device to your user.</li>
  <li><strong>“No speech was heard”</strong>: nothing loud enough was said; check the microphone in Settings › Voice.</li>
  <li><strong>Slow</strong>: choose a smaller model; <em>Test it</em> tells you what to expect. The model marked <em>Live text</em> keeps up with your speech.</li>
  <li><strong>“No model selected” although one is installed</strong>: press <em>Use</em> on it; the notice disappears as soon as a model is in use.</li>
</ul>
