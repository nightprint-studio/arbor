//! Canonical, machine-readable reference for the nemus DSL.
//!
//! This is the **single source of truth** for "what names the language exposes
//! and what they mean": one [`DslEntry`] per combinator, generator, signal,
//! transform, mini-notation operator, and host keyword. The frontend loads it
//! once (`nemus_lang_reference`) to feed autocomplete, hover docs, and the Docs
//! panel — so the editor's language intelligence and the implementation can
//! never drift.
//!
//! The lists that classify a name (`is_transform` / `is_combinator` /
//! `signal_source`) are **derived** from this catalogue (see [`transform_names`],
//! [`combinator_names`], [`signal_names`], [`generator_names`]); the eval modules
//! keep only the name→closure mapping. A test (`reference::tests`) asserts the
//! catalogue and the implementations cover exactly the same set, both ways, so a
//! new builtin without a doc entry (or vice-versa) fails CI.

/// The category of a [`DslEntry`]. Rendered to a lowercase string (`"combinator"`,
/// `"transform"`, …) via [`DslKind::as_str`] so the frontend can group/filter
/// without a numeric enum. This crate stays serde-free (only `tree-sitter` + `cc`
/// as deps); the Tauri shell maps these to its own serde DTO at the IPC boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DslKind {
    /// Composes patterns (`par`, `seq`, `cat`, `arrange`, `tracks`, …).
    Combinator,
    /// Produces a value, not a pattern transform (`rand`, `choose`, `sample`, …).
    Generator,
    /// A continuous unipolar `0..1` signal source (`sine`, `saw`, …).
    Signal,
    /// A method on a continuous signal (`.range`, `.fast`, `.slow`).
    SignalMethod,
    /// A pattern → pattern transform (`fast`, `gain`, `every`, …).
    Transform,
    /// A method on a range / list (`.map`, `.par`, `.seq`, `.cat`).
    SeqMethod,
    /// A mini-notation island head (`s`, `n`, `sound`, `note`).
    Island,
    /// A host-language statement keyword (`let`, `fn`, `import`, `cps`, `tempo`).
    Keyword,
    /// An eval-time logging function (`trace`, `debug`, `info`, `warn`, `error`).
    Log,
    /// A mini-notation operator (`~`, `*n`, `(n,k)`, …) — documented, not callable.
    Mini,
    /// A note / chord literal form (`c4`, `c4'min7`, scale degrees).
    Note,
}

impl DslKind {
    /// The serialised tag (matches the serde rename) — handy for tests / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            DslKind::Combinator => "combinator",
            DslKind::Generator => "generator",
            DslKind::Signal => "signal",
            DslKind::SignalMethod => "signal_method",
            DslKind::Transform => "transform",
            DslKind::SeqMethod => "seq_method",
            DslKind::Island => "island",
            DslKind::Keyword => "keyword",
            DslKind::Log => "log",
            DslKind::Mini => "mini",
            DslKind::Note => "note",
        }
    }
}

/// One parameter of a DSL entry (for the autocomplete `detail` / hover table).
#[derive(Debug, Clone)]
pub struct DslParam {
    /// Parameter name as written in the signature (`n`, `pat`, `lo`, …).
    pub name: &'static str,
    /// Whether the parameter may be omitted.
    pub optional: bool,
    /// One-line description (type + range + meaning).
    pub summary: &'static str,
    /// Default value when omitted (only meaningful if `optional`).
    pub default: Option<&'static str>,
}

impl DslParam {
    /// A required parameter.
    const fn req(name: &'static str, summary: &'static str) -> Self {
        DslParam { name, optional: false, summary, default: None }
    }
    /// An optional parameter with a default.
    const fn opt(name: &'static str, summary: &'static str, default: &'static str) -> Self {
        DslParam { name, optional: true, summary, default: Some(default) }
    }
}

/// One catalogue entry: a named piece of the language with its signature,
/// human summary, parameters, and a short realistic example.
#[derive(Debug, Clone)]
pub struct DslEntry {
    /// The bare name as typed (`gain`, `par`, `sine`, `~`).
    pub name: &'static str,
    /// What category it belongs to (drives grouping + autocomplete `type`).
    pub kind: DslKind,
    /// One-line signature, e.g. `gain(x, pat) -> pat` or `pat.gain(x)`.
    pub signature: &'static str,
    /// 1–2 sentence description of what it does.
    pub summary: &'static str,
    /// Its parameters in order (empty for nullary forms / operators).
    pub params: Vec<DslParam>,
    /// A short, realistic usage snippet.
    pub example: &'static str,
    /// What the call returns, when not obvious from the signature.
    pub returns: Option<&'static str>,
}

/// Compact builder for a catalogue entry (keeps [`reference`] readable).
fn entry(
    name: &'static str,
    kind: DslKind,
    signature: &'static str,
    summary: &'static str,
    params: Vec<DslParam>,
    example: &'static str,
) -> DslEntry {
    DslEntry { name, kind, signature, summary, params, example, returns: None }
}

/// The full DSL catalogue. Built fresh on each call (a `Vec` of borrowed-static
/// data — cheap); the command layer can cache it if it ever matters.
pub fn reference() -> Vec<DslEntry> {
    let mut v = Vec::new();
    v.extend(combinators());
    v.extend(generators());
    v.extend(signals());
    v.extend(signal_methods());
    v.extend(seq_methods());
    v.extend(islands());
    v.extend(keywords());
    v.extend(logs());
    v.extend(transforms());
    v.extend(notes());
    v.extend(mini_operators());
    v
}

// ── Derived name sets (the canonical lists the eval layer defers to) ───────────
//
// These are read on the eval hot path (`is_transform` / `is_combinator` run once
// per call/method during evaluation), so the per-kind name lists are computed
// once from the catalogue and cached — no `reference()` rebuild per lookup.

use std::sync::OnceLock;

/// Names of every [`DslKind::Transform`] entry — the canonical replacement for a
/// hand-written `is_transform` match arm.
pub fn transform_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| names_of(DslKind::Transform))
}

/// Names of every [`DslKind::Combinator`] entry.
pub fn combinator_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| names_of(DslKind::Combinator))
}

/// Names of every [`DslKind::Signal`] source.
pub fn signal_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| names_of(DslKind::Signal))
}

/// Names of every [`DslKind::Generator`] entry.
pub fn generator_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| names_of(DslKind::Generator))
}

/// Names of every [`DslKind::Log`] function.
pub fn log_names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES.get_or_init(|| names_of(DslKind::Log))
}

fn names_of(kind: DslKind) -> Vec<&'static str> {
    reference()
        .into_iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.name)
        .collect()
}

// ── A. Combinators ─────────────────────────────────────────────────────────────

fn combinators() -> Vec<DslEntry> {
    vec![
        entry(
            "par", DslKind::Combinator, "par(...pats | list) -> pat",
            "Overlay every pattern so they sound at once (stack / polyphony). The mini-notation equivalent is `&`. Accepts varargs or a single list.",
            vec![DslParam::req("pats", "two or more patterns, or one list of patterns")],
            "par(drums, bass, pad)",
        ),
        entry(
            "stack", DslKind::Combinator, "stack(...pats | list) -> pat",
            "Alias of `par`: overlay every pattern so they play simultaneously.",
            vec![DslParam::req("pats", "two or more patterns, or one list of patterns")],
            "stack(kick, snare, hats)",
        ),
        entry(
            "seq", DslKind::Combinator, "seq(...pats | list) -> pat",
            "Lay the patterns out in equal slots within one cycle (mini-notation: a space). `seq(a, b)` puts a in 0..0.5, b in 0.5..1.",
            vec![DslParam::req("pats", "two or more patterns, or one list of patterns")],
            "seq(s(bd), s(sd), s(hh))",
        ),
        entry(
            "cat", DslKind::Combinator, "cat(...pats | list) -> pat",
            "Play one pattern per cycle, alternating and looping (mini-notation: `< >`). `cat(a, b)` is a at cycle 0, b at cycle 1, a at cycle 2…",
            vec![DslParam::req("pats", "two or more patterns, or one list of patterns")],
            "cat(verseA, verseB)",
        ),
        entry(
            "arrange", DslKind::Combinator, "arrange(...sections) -> pat",
            "Concatenate `cycles(n, x)` sections along the absolute timeline: the first for n1 cycles, then the second for n2, and so on, looping at the end.",
            vec![DslParam::req("sections", "cycles(n, pat) / section(name, n, pat) directives")],
            "arrange(cycles(4, intro), cycles(16, main), cycles(4, outro))",
        ),
        entry(
            "cycles", DslKind::Combinator, "cycles(n, pat) -> section",
            "An arrangement directive: `pat` occupies `n` cycles of the timeline. Only valid inside `arrange(…)`. (With a number as the 2nd arg it is instead a tempo segment for `tempo(…)`.)",
            vec![
                DslParam::req("n", "how many cycles the section spans (integer)"),
                DslParam::req("pat", "the pattern (or, in `tempo`, the cps number) for the span"),
            ],
            "cycles(8, mainGroove)",
        ),
        entry(
            "section", DslKind::Combinator, "section(name, n, pat) -> section",
            "A named arrangement directive (the labelled counterpart of `cycles`): `pat` spans `n` cycles and shows in the arrangement view as a coloured band/chip with `name`.",
            vec![
                DslParam::req("name", "the section label (string), e.g. \"INTRO\""),
                DslParam::req("n", "how many cycles the section spans (integer)"),
                DslParam::req("pat", "the pattern for the span"),
            ],
            "section(\"DROP\", 16, drop)",
        ),
        entry(
            "track", DslKind::Combinator, "track(name, pat) -> track",
            "One named mixer channel: a name plus its pattern. `arrange`/`cat`/`par` build the timeline inside it.",
            vec![
                DslParam::req("name", "the channel/strip name (string)"),
                DslParam::req("pat", "the pattern this track plays"),
            ],
            "track(\"bass\", bassline(c2))",
        ),
        entry(
            "tracks", DslKind::Combinator, "tracks(...tracks | list) -> output",
            "The output of a `.nemus` file: a list of named channels (the mixer strips). Each is a `track(name, pattern)`.",
            vec![DslParam::req("tracks", "the `track(...)` channels, varargs or one list")],
            "tracks(track(\"bass\", bass), track(\"drums\", drums))",
        ),
    ]
}

// ── E. Generators (produce values) + file sources ──────────────────────────────

fn generators() -> Vec<DslEntry> {
    vec![
        entry(
            "rand", DslKind::Generator, "rand(lo, hi) -> pat",
            "A pattern of random floats in [lo, hi], one value per query, seeded by cycle (so it is identical every loop). The range is mandatory.",
            vec![
                DslParam::req("lo", "lower bound (inclusive)"),
                DslParam::req("hi", "upper bound (inclusive)"),
            ],
            ".lpf(rand(200, 2000))",
        ),
        entry(
            "choose", DslKind::Generator, "choose(...options) -> pat",
            "Pick one of the options at random (seeded by cycle) per query. Options are all numbers or all patterns.",
            vec![DslParam::req("options", "two or more numbers, or two or more patterns")],
            "choose(c4, ef4, g4)",
        ),
        entry(
            "sample", DslKind::Generator, "sample(\"path\") -> pat",
            "Load an audio file as a one-shot: a pattern of a single hap that plays the file. For chops/hits/foley; pitch via `.shift`. Path is project-relative.",
            vec![DslParam::req("path", "project-relative path to a WAV / mp3 / ogg / flac file")],
            "sample(\"chops/vox.wav\").shift(7)",
        ),
        entry(
            "audio", DslKind::Generator, "audio(\"path\") -> pat",
            "Load a long file (stem, vocal take, ambience) that plays in full from the start of the track. Path is project-relative.",
            vec![DslParam::req("path", "project-relative path to a long audio file")],
            "track(\"vocals\", audio(\"vox/take.wav\"))",
        ),
    ]
}

// ── Continuous signal sources + their methods ──────────────────────────────────

fn signals() -> Vec<DslEntry> {
    vec![
        entry(
            "sine", DslKind::Signal, "sine -> signal",
            "A unipolar 0..1 sine LFO over one cycle. Use as a patternised control via `.range(lo, hi)`.",
            vec![],
            ".lpf(sine.range(400, 2000))",
        ),
        entry(
            "saw", DslKind::Signal, "saw -> signal",
            "A unipolar 0..1 rising sawtooth ramp over one cycle.",
            vec![],
            ".gain(saw.range(0.3, 1))",
        ),
        entry(
            "isaw", DslKind::Signal, "isaw -> signal",
            "A unipolar 0..1 falling (inverted) sawtooth ramp over one cycle.",
            vec![],
            ".pan(isaw)",
        ),
        entry(
            "tri", DslKind::Signal, "tri -> signal",
            "A unipolar 0..1 triangle LFO over one cycle.",
            vec![],
            ".room(tri.range(0, 0.4))",
        ),
        entry(
            "square", DslKind::Signal, "square -> signal",
            "A unipolar 0..1 square LFO (50% duty) over one cycle.",
            vec![],
            ".gain(square.range(0.5, 1))",
        ),
    ]
}

fn signal_methods() -> Vec<DslEntry> {
    vec![
        entry(
            "range", DslKind::SignalMethod, "signal.range(lo, hi) -> signal",
            "Rescale a unipolar 0..1 signal into [lo, hi]. The result is still a signal, chainable into a patternised control.",
            vec![
                DslParam::req("lo", "low end of the output range"),
                DslParam::req("hi", "high end of the output range"),
            ],
            "sine.range(200, 2000)",
        ),
        entry(
            "fast", DslKind::SignalMethod, "signal.fast(n) -> signal",
            "Speed the signal up by factor `n` (more oscillations per cycle).",
            vec![DslParam::req("n", "rate factor (> 0)")],
            "sine.fast(2).range(0, 1)",
        ),
        entry(
            "slow", DslKind::SignalMethod, "signal.slow(n) -> signal",
            "Slow the signal down by factor `n` (the LFO spans n cycles).",
            vec![DslParam::req("n", "rate divisor (> 0)")],
            "tri.slow(4).range(0, 1)",
        ),
    ]
}

// ── Range / list methods ───────────────────────────────────────────────────────

fn seq_methods() -> Vec<DslEntry> {
    vec![
        entry(
            "map", DslKind::SeqMethod, "(range | list).map(fn) -> list",
            "Apply `fn` to each element, returning a list. Combine it yourself with `par`/`seq`/`cat`, or post-process.",
            vec![DslParam::req("fn", "a one-argument function `i => expr`")],
            "par((0..8).map(i => n($i)))",
        ),
        entry(
            "par", DslKind::SeqMethod, "(range | list).par(fn) -> pat",
            "Shortcut for `par((range).map(fn))`: map each element then overlay the results.",
            vec![DslParam::req("fn", "a one-argument function `i => expr`")],
            "(0..8).par(i => n($i).off(i*0.1, gain(0.5)))",
        ),
        entry(
            "seq", DslKind::SeqMethod, "(range | list).seq(fn) -> pat",
            "Shortcut for `seq((range).map(fn))`: map then lay out in equal slots within a cycle.",
            vec![DslParam::req("fn", "a one-argument function `i => expr`")],
            "(0..4).seq(i => n($i))",
        ),
        entry(
            "cat", DslKind::SeqMethod, "(range | list).cat(fn) -> pat",
            "Shortcut for `cat((range).map(fn))`: map then play one result per cycle.",
            vec![DslParam::req("fn", "a one-argument function `i => expr`")],
            "(0..4).cat(i => n($i))",
        ),
    ]
}

// ── Mini-notation island heads ─────────────────────────────────────────────────

fn islands() -> Vec<DslEntry> {
    vec![
        entry(
            "s", DslKind::Island, "s(<mini-notation>) -> pat",
            "A sound/sample island: its leaves are sample names (`bd`, `sd`, `hh`). The content is mini-notation (space-separated, not commas).",
            vec![DslParam::req("mini", "mini-notation of sample names")],
            "s(bd ~ sd ~)",
        ),
        entry(
            "sound", DslKind::Island, "sound(<mini-notation>) -> pat",
            "Alias of `s`: a sound/sample island.",
            vec![DslParam::req("mini", "mini-notation of sample names")],
            "sound(bd [hh hh] sd)",
        ),
        entry(
            "n", DslKind::Island, "n(<mini-notation>) -> pat",
            "A note island: its leaves are pitches (`c4`), scale degrees (`0 2 4`) or chords (`c4'min7`), played by `.inst(…)`.",
            vec![DslParam::req("mini", "mini-notation of pitches / degrees / chords")],
            "n(c2 g1).inst(\"synth.bass\")",
        ),
        entry(
            "note", DslKind::Island, "note(<mini-notation>) -> pat",
            "Alias of `n`: a note island.",
            vec![DslParam::req("mini", "mini-notation of pitches / degrees / chords")],
            "note(<c4'min7 af3'maj7>)",
        ),
    ]
}

// ── Host keywords / statements ─────────────────────────────────────────────────

fn keywords() -> Vec<DslEntry> {
    vec![
        entry(
            "let", DslKind::Keyword, "let IDENT = expr",
            "Bind a value to a name (it does not sound on its own).",
            vec![],
            "let bass = n(c2 g1).inst(\"synth.bass\")",
        ),
        entry(
            "fn", DslKind::Keyword, "fn IDENT(params) = expr",
            "Define an expression-bodied function. No recursion (the language stays total).",
            vec![],
            "fn bassline(root) = n($root ~ $root g1).lpf(800)",
        ),
        entry(
            "import", DslKind::Keyword, "import { name, … } from \"path\"",
            "Bring selected top-level `fn`/`let` declarations from another `.nemus` file into scope. The imported file's `tracks(…)` output is ignored.",
            vec![],
            "import { kick, snare } from \"lib/drums.nemus\"",
        ),
        entry(
            "cps", DslKind::Keyword, "cps(n)",
            "Set a constant clock in cycles-per-second. `cps(0.5)` is one cycle every two seconds. Overridden by `tempo(…)` if present.",
            vec![DslParam::req("n", "cycles per second")],
            "cps(0.5)",
        ),
        entry(
            "tempo", DslKind::Keyword, "tempo(cycles(n, cps), …)",
            "A piecewise-constant tempo map: each `cycles(n, cps)` plays `n` cycles at that cps; the tempo steps on cycle boundaries and the map loops over the total. Wins over `cps(…)`.",
            vec![DslParam::req("segments", "one or more `cycles(n, cps)` tempo segments")],
            "tempo(cycles(8, 0.5), cycles(16, 0.6))",
        ),
    ]
}

// ── Eval-time logging ──────────────────────────────────────────────────────────

fn logs() -> Vec<DslEntry> {
    let mk = |name: &'static str, level: &'static str, example: &'static str| {
        entry(
            name, DslKind::Log,
            // signature spelled per-entry to keep &'static str
            "",
            "",
            vec![
                DslParam::req("msg", "the message (or, with two args, a label)"),
                DslParam { name: "x", optional: true, summary: "a value to log and return unchanged (pass-through)", default: None },
            ],
            example,
        )
        .with_log(level)
    };
    vec![
        mk("trace", "trace", "trace(\"per-event detail\")"),
        mk("debug", "debug", "debug(\"bass root\", c2)"),
        mk("info", "info", "info(\"loaded\")"),
        mk("warn", "warn", "warn(\"clipping risk\")"),
        mk("error", "error", "error(\"bad input\")"),
    ]
}

impl DslEntry {
    /// Fill in the signature + summary of a logging function (they share a shape).
    fn with_log(mut self, level: &'static str) -> Self {
        self.signature = match self.name {
            "trace" => "trace(msg) -> unit  ·  trace(label, x) -> x",
            "debug" => "debug(msg) -> unit  ·  debug(label, x) -> x",
            "info" => "info(msg) -> unit  ·  info(label, x) -> x",
            "warn" => "warn(msg) -> unit  ·  warn(label, x) -> x",
            "error" => "error(msg) -> unit  ·  error(label, x) -> x",
            _ => self.signature,
        };
        self.summary = match level {
            "trace" => "Log a message at the `trace` level (eval-time). `trace(label, x)` logs and returns `x` unchanged (pass-through, for inspection). Gated by the log threshold.",
            "debug" => "Log a message at the `debug` level (eval-time). `debug(label, x)` logs and returns `x` unchanged (pass-through, for inspection). Gated by the log threshold.",
            "info" => "Log a message at the `info` level (eval-time). `info(label, x)` logs and returns `x` unchanged. Gated by the log threshold.",
            "warn" => "Log a message at the `warn` level (eval-time). `warn(label, x)` logs and returns `x` unchanged. Gated by the log threshold.",
            "error" => "Log a message at the `error` level (eval-time). `error(label, x)` logs and returns `x` unchanged. Gated by the log threshold.",
            _ => self.summary,
        };
        self
    }
}

// ── Transforms (pattern → pattern) ─────────────────────────────────────────────

fn transforms() -> Vec<DslEntry> {
    let pat = || DslParam::req("pat", "the pattern (the method receiver, or last arg)");
    vec![
        // Time & structure
        entry(
            "fast", DslKind::Transform, "fast(n, pat) -> pat  ·  pat.fast(n)",
            "Compress time: `n`× more repetitions per cycle (mini-notation `*n`). `fast(0.5)` halves the speed.",
            vec![DslParam::req("n", "rate factor (> 0)"), pat()],
            "s(bd sd).fast(2)",
        ),
        entry(
            "slow", DslKind::Transform, "slow(n, pat) -> pat  ·  pat.slow(n)",
            "Stretch time (mini-notation `/n`). Equivalent to `fast(1/n)`; `slow(2)` makes the pattern last two cycles.",
            vec![DslParam::req("n", "rate divisor (> 0)"), pat()],
            "arp.slow(2)",
        ),
        entry(
            "rev", DslKind::Transform, "rev(pat) -> pat  ·  pat.rev()  ·  rev",
            "Reverse the order of events within each cycle. A nullary transform — bare `rev` is already a transform value.",
            vec![pat()],
            "n(c e g).rev()",
        ),
        entry(
            "palindrome", DslKind::Transform, "palindrome(pat) -> pat  ·  pat.palindrome()  ·  palindrome",
            "Alternate forwards / reversed each cycle (forwards on even cycles, reversed on odd). Nullary.",
            vec![pat()],
            "arp.palindrome()",
        ),
        entry(
            "iter", DslKind::Transform, "iter(n, pat) -> pat  ·  pat.iter(n)",
            "Rotate the pattern by one nth each cycle, cycling through all `n` rotations.",
            vec![DslParam::req("n", "number of rotation steps (integer)"), pat()],
            "n(c e g b).iter(4)",
        ),
        entry(
            "chunk", DslKind::Transform, "chunk(n, tf, pat) -> pat  ·  pat.chunk(n, tf)",
            "Split each cycle into `n` chunks and apply `tf` to a different chunk each cycle (the affected chunk advances).",
            vec![
                DslParam::req("n", "number of chunks (integer)"),
                DslParam::req("tf", "transform value applied to one chunk per cycle"),
                pat(),
            ],
            "s(bd sd hh cp).chunk(4, fast(2))",
        ),
        entry(
            "swingBy", DslKind::Transform, "swingBy(amount, n, pat) -> pat  ·  pat.swingBy(amount, n)",
            "Add swing: delay every other subdivision (of `n` per cycle) by `amount` of a slot.",
            vec![
                DslParam::req("amount", "delay as a fraction of a slot (e.g. 1/3)"),
                DslParam::req("n", "subdivisions per cycle (integer)"),
                pat(),
            ],
            "s(hh*8).swingBy(0.1, 8)",
        ),
        // Periodic / echo / probability
        entry(
            "every", DslKind::Transform, "every(n, tf, pat) -> pat  ·  pat.every(n, tf)",
            "Apply `tf` on cycles 0, n, 2n, … and leave the others unchanged.",
            vec![
                DslParam::req("n", "apply every nth cycle (integer)"),
                DslParam::req("tf", "transform value to apply"),
                pat(),
            ],
            "arp.every(4, rev)",
        ),
        entry(
            "off", DslKind::Transform, "off(t, tf, pat) -> pat  ·  pat.off(t, tf)",
            "Overlay a copy shifted forward by `t` cycles with `tf` applied — typical for echoes / layers.",
            vec![
                DslParam::req("t", "shift in cycles (e.g. 0.125 = 1/8)"),
                DslParam::req("tf", "transform value applied to the copy"),
                pat(),
            ],
            "lead.off(0.125, gain(0.4))",
        ),
        entry(
            "degrade", DslKind::Transform, "degrade(pat) -> pat  ·  pat.degrade()  ·  degrade",
            "Randomly drop ~50% of events (seeded by cycle, so stable every loop). Nullary.",
            vec![pat()],
            "s(hh*16).degrade()",
        ),
        entry(
            "degradeBy", DslKind::Transform, "degradeBy(p, pat) -> pat  ·  pat.degradeBy(p)",
            "Randomly drop a fraction `p` of events (seeded). `degradeBy(0.3)` drops ~30%.",
            vec![DslParam::req("p", "drop probability 0..1"), pat()],
            "s(hh*16).degradeBy(0.3)",
        ),
        entry(
            "sometimes", DslKind::Transform, "sometimes(tf, pat) -> pat  ·  pat.sometimes(tf)",
            "Apply `tf` to ~50% of events (seeded); the rest pass through.",
            vec![DslParam::req("tf", "transform value to sometimes apply"), pat()],
            "lead.sometimes(degrade)",
        ),
        entry(
            "sometimesBy", DslKind::Transform, "sometimesBy(p, tf, pat) -> pat  ·  pat.sometimesBy(p, tf)",
            "Apply `tf` to a fraction `p` of events (seeded). `sometimesBy(0.2, fast(2))` affects ~20%.",
            vec![
                DslParam::req("p", "probability 0..1"),
                DslParam::req("tf", "transform value to apply"),
                pat(),
            ],
            "lead.sometimesBy(0.2, fast(2))",
        ),
        entry(
            "jux", DslKind::Transform, "jux(tf, pat) -> pat  ·  pat.jux(tf)",
            "Split into stereo: the original panned left, a copy with `tf` applied panned right. Deterministic.",
            vec![DslParam::req("tf", "transform value applied to the right copy"), pat()],
            "arp.jux(rev)",
        ),
        // Voice & mix (constant number OR a numeric pattern/signal)
        entry(
            "gain", DslKind::Transform, "gain(x, pat) -> pat  ·  pat.gain(x)",
            "Multiplicative amplitude (typical 0..1, default 1). Accepts a constant or a numeric pattern/signal.",
            vec![DslParam::req("x", "amplitude (number or signal), typical 0..1"), pat()],
            "drums.gain(0.6)",
        ),
        entry(
            "pan", DslKind::Transform, "pan(x, pat) -> pat  ·  pat.pan(x)",
            "Stereo position: 0 = left, 1 = right, 0.5 = centre. Patternisable.",
            vec![DslParam::req("x", "pan position 0..1 (number or signal)"), pat()],
            "hats.pan(rand(0, 1))",
        ),
        entry(
            "room", DslKind::Transform, "room(x, pat) -> pat  ·  pat.room(x)",
            "Reverb send amount 0..1. Patternisable.",
            vec![DslParam::req("x", "reverb send 0..1 (number or signal)"), pat()],
            "pad.room(0.4)",
        ),
        entry(
            "lpf", DslKind::Transform, "lpf(hz, pat) -> pat  ·  pat.lpf(hz)",
            "Low-pass filter cutoff in Hz (≈ 20..20000) — `lpf(800)` darkens the sound. Patternisable.",
            vec![DslParam::req("hz", "cutoff frequency in Hz (number or signal)"), pat()],
            "lead.lpf(sine.range(400, 2000))",
        ),
        entry(
            "hpf", DslKind::Transform, "hpf(hz, pat) -> pat  ·  pat.hpf(hz)",
            "High-pass filter cutoff in Hz (complements `lpf`) — `hpf(200)` removes the lows. Patternisable.",
            vec![DslParam::req("hz", "cutoff frequency in Hz (number or signal)"), pat()],
            "bass.hpf(120)",
        ),
        entry(
            "delay", DslKind::Transform, "delay(t, fb?, mix?, pat) -> pat  ·  pat.delay(t, fb?, mix?)",
            "A feedback audio echo (distinct from `.off`, which re-triggers musically). `t` is in fractions of a cycle.",
            vec![
                DslParam::req("t", "delay time in fractions of a cycle"),
                DslParam::opt("fb", "feedback 0..1", "0.3"),
                DslParam::opt("mix", "wet mix 0..1", "0.5"),
                pat(),
            ],
            "lead.delay(0.1875, 0.4, 0.5)",
        ),
        entry(
            "crush", DslKind::Transform, "crush(bits, pat) -> pat  ·  pat.crush(bits)",
            "Bitcrush: reduce resolution to `bits` for a lo-fi / digital sound. Patternisable.",
            vec![DslParam::req("bits", "bit depth (number or signal)"), pat()],
            "drums.crush(6)",
        ),
        entry(
            "shape", DslKind::Transform, "shape(amount, pat) -> pat  ·  pat.shape(amount)",
            "Waveshaper distortion, `amount` 0..1. Patternisable.",
            vec![DslParam::req("amount", "distortion 0..1 (number or signal)"), pat()],
            "bass.shape(0.3)",
        ),
        entry(
            "shift", DslKind::Transform, "shift(semitones, pat) -> pat  ·  pat.shift(semitones)",
            "Pitch shift by `semitones` via resampling (pitch + speed are coupled). `shift(7)` = +7 semitones.",
            vec![DslParam::req("semitones", "semitone offset (number or signal)"), pat()],
            "sample(\"vox.wav\").shift(7)",
        ),
        entry(
            "speed", DslKind::Transform, "speed(x, pat) -> pat  ·  pat.speed(x)",
            "Playback speed via resampling (pitch + speed coupled). `speed(2)` doubles speed (and raises an octave).",
            vec![DslParam::req("x", "speed factor (number or signal)"), pat()],
            "chop.speed(0.5)",
        ),
        entry(
            "vel", DslKind::Transform, "vel(x, pat) -> pat  ·  pat.vel(x)",
            "Velocity 0..1 — selects the sampled velocity layer (changes timbre, not just volume). Patternisable; `.vel(rand(0.5, 0.9))` humanises.",
            vec![DslParam::req("x", "velocity 0..1 (number or signal)"), pat()],
            "strings.vel(rand(0.5, 0.9))",
        ),
        entry(
            "inst", DslKind::Transform, "inst(name, pat) -> pat  ·  pat.inst(name)",
            "Choose the voice: a built-in synth preset (`\"synth.bass\"`, …) or an installed sampler (`\"strings.violin\"`). Unknown names fall back to the synth.",
            vec![DslParam::req("name", "instrument name (string)"), pat()],
            "n(c2 g1).inst(\"synth.bass\")",
        ),
        entry(
            "art", DslKind::Transform, "art(name, pat) -> pat  ·  pat.art(name)",
            "Articulation: a constant string (`\"legato\"`, `\"staccato\"`, `\"pizzicato\"`, …) mapping `inst + art` to a sample region / keyswitch.",
            vec![DslParam::req("name", "articulation name (string)"), pat()],
            "strings.art(\"pizzicato\")",
        ),
        entry(
            "scale", DslKind::Transform, "scale(name, pat) -> pat  ·  pat.scale(name)",
            "Interpret numeric DEGREE leaves against a `\"root:mode\"` scale (e.g. \"c:minor\"). Required when the pattern uses degrees; no effect on note-name leaves.",
            vec![DslParam::req("name", "\"root:mode\" string, e.g. \"c:minor\""), pat()],
            "n(0 2 4 7).scale(\"c:minor\")",
        ),
        entry(
            "add", DslKind::Transform, "add(n, pat) -> pat  ·  pat.add(n)",
            "Transpose every pitch by `n` semitones.",
            vec![DslParam::req("n", "semitone offset (number)"), pat()],
            "lead.add(12)",
        ),
        entry(
            "addDeg", DslKind::Transform, "addDeg(n, pat) -> pat  ·  pat.addDeg(n)",
            "Transpose by `n` scale degrees (requires a scale on the pattern).",
            vec![DslParam::req("n", "degree offset (integer)"), pat()],
            "n(0 2 4).scale(\"c:minor\").addDeg(2)",
        ),
        entry(
            "log", DslKind::Transform, "log(level?, pat) -> pat  ·  pat.log(level?)",
            "Transparent per-hap logging: log every event at `level` (default `debug`) while passing the pattern through unchanged.",
            vec![DslParam::opt("level", "log level (trace..error)", "debug"), pat()],
            "lead.log(trace)",
        ),
    ]
}

// ── Note / chord literals + scales (documentation entries) ─────────────────────

fn notes() -> Vec<DslEntry> {
    vec![
        entry(
            "c4", DslKind::Note, "<letter><sharp/flat?><octave?>",
            "A note-name literal. Letters c d e f g a b; `s` = sharp, `f` = flat (so `bf3` is B-flat, never ambiguous with B). A bare letter uses the default octave (4).",
            vec![],
            "n(c4 ef4 g4)",
        ),
        entry(
            "c4'min7", DslKind::Note, "<note>'<chord>",
            "A chord literal: a note plus a chord name expands to a stack. Chords: maj min dim aug sus2 sus4 5 6 min6 add9 7 maj7 min7 dim7 m7b5 minMaj7 aug7 9 maj9 min9 11 13 min11 maj13 min13 (alias m = min).",
            vec![],
            "n(<c4'min7 af3'maj7>)",
        ),
        entry(
            "degree", DslKind::Note, "<integer> (scale degree)",
            "A numeric leaf in an `n(…)` island is a scale degree (0 = root) resolved by `.scale(\"root:mode\")`. Scales: major/ionian minor/aeolian dorian phrygian lydian mixolydian locrian harmonicminor melodicminor majpent minpent chromatic.",
            vec![],
            "n(0 2 4 7).scale(\"c:dorian\")",
        ),
    ]
}

// ── Mini-notation operators (documentation only) ───────────────────────────────

fn mini_operators() -> Vec<DslEntry> {
    let op = |name: &'static str, sig: &'static str, summary: &'static str, example: &'static str| {
        entry(name, DslKind::Mini, sig, summary, vec![], example)
    };
    vec![
        op("~", "~", "A silent slot (rest).", "s(bd ~ sd ~)"),
        op("_", "_", "Extend the previous term by one more slot.", "s(bd _ sd)"),
        op("[ ]", "[ … ]", "Group several events into one slot (nestable).", "s(bd [hh hh] sd)"),
        op("< >", "< … >", "Alternation: play one element per cycle, rotating.", "s(bd <sd cp>)"),
        op("&", "a & b", "Parallel (stack) — the loosest-precedence operator; each `&` opens a lane.", "s(bd sd & hh*8)"),
        op("*n", "x*n", "Fast: repeat the token n times inside its slot.", "s(hh*4)"),
        op("/n", "x/n", "Slow: play the token once every n cycles.", "s(bd/2)"),
        op("!n", "x!n", "Replicate as n separate slots (≠ `*n`).", "s(bd!3)"),
        op("@n", "x@n", "Weight: give the token n× the duration of its siblings.", "s(bd@3 sd)"),
        op("(n,k)", "x(n,k) / x(n,k,rot)", "Euclidean: distribute n hits over k steps (optional rotation).", "s(bd(3,8))"),
        op(":n", "x:n", "Sample variant: pick the nth sample (s islands only).", "s(bd:3)"),
        op("'chord", "note'chord", "Expand a note into a chord (n islands only).", "n(c4'min7)"),
        op("$ident", "$ident", "Splice the variable `ident` as a leaf (name only, no expressions).", "n(c5 $motif g4)"),
    ]
}

// ── Completeness test ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::combinators::{is_combinator, signal_source};
    use crate::eval::transforms::is_transform;
    use std::collections::HashSet;

    /// Every name the implementation classifies as a transform must have a
    /// `Transform` entry, and every `Transform` entry must classify back — so the
    /// catalogue and `is_transform` can never drift.
    #[test]
    fn transforms_round_trip() {
        let cataloged: HashSet<&str> = transform_names().iter().copied().collect();
        for name in &cataloged {
            assert!(is_transform(name), "catalogue lists transform `{name}` but is_transform() rejects it");
        }
        // The implemented transform names (kept here as the authority for the
        // reverse direction; mirrors the make_transform match arms).
        for name in IMPLEMENTED_TRANSFORMS {
            assert!(cataloged.contains(name), "transform `{name}` is implemented but missing from reference()");
        }
    }

    /// `is_combinator` (the implementation's set of constructors/combinators)
    /// covers exactly the `Combinator`/`Generator`/`Keyword`(cps,tempo)/`Log`
    /// names that route through `eval_builtin_call`.
    #[test]
    fn combinators_round_trip() {
        // Names that the evaluator dispatches via `eval_builtin_call`.
        let builtin: HashSet<&str> = combinator_names()
            .iter()
            .copied()
            .chain(generator_names().iter().copied())
            .chain(log_names().iter().copied())
            .chain(["cps", "tempo"]) // the two builtin keywords (not free identifiers)
            .collect();
        // `stack` is an alias of `par` and `tracks`/`track`/`section` are
        // constructors — all flow through is_combinator.
        for name in &builtin {
            assert!(
                is_combinator(name),
                "catalogue lists builtin `{name}` but is_combinator() rejects it"
            );
        }
        for name in IMPLEMENTED_COMBINATORS {
            assert!(
                builtin.contains(name),
                "builtin `{name}` is dispatched but missing from reference()"
            );
        }
    }

    /// Every catalogued `Signal` must be a real signal source, and vice-versa.
    #[test]
    fn signals_round_trip() {
        let cataloged: HashSet<&str> = signal_names().iter().copied().collect();
        for name in &cataloged {
            assert!(signal_source(name).is_some(), "catalogue lists signal `{name}` but signal_source() returns None");
        }
        for name in IMPLEMENTED_SIGNALS {
            assert!(cataloged.contains(name), "signal `{name}` exists but is missing from reference()");
        }
    }

    /// No duplicate (name, kind) pairs in the catalogue (aliases share a name but
    /// not a kind — `par` is both a Combinator and a SeqMethod, which is fine).
    #[test]
    fn no_duplicate_name_kind() {
        let mut seen = HashSet::new();
        for e in reference() {
            assert!(
                seen.insert((e.name, e.kind.as_str())),
                "duplicate catalogue entry: {} ({})",
                e.name,
                e.kind.as_str()
            );
        }
    }

    // The authoritative implemented-name lists for the reverse direction of the
    // round-trip tests. These mirror the `match` arms in `combinators.rs` /
    // `transforms.rs`; adding a builtin there without updating these (and the
    // catalogue) fails the test.
    const IMPLEMENTED_TRANSFORMS: &[&str] = &[
        "rev", "degrade", "palindrome", "fast", "slow", "gain", "pan", "room",
        "lpf", "hpf", "shift", "speed", "crush", "shape", "vel", "inst", "art",
        "scale", "add", "addDeg", "degradeBy", "sometimesBy", "chunk", "iter",
        "swingBy", "delay", "every", "off", "sometimes", "jux", "log",
    ];
    const IMPLEMENTED_COMBINATORS: &[&str] = &[
        "par", "stack", "seq", "cat", "arrange", "cycles", "section", "track",
        "tracks", "rand", "choose", "sample", "audio", "cps", "tempo", "trace",
        "debug", "info", "warn", "error",
    ];
    const IMPLEMENTED_SIGNALS: &[&str] = &["sine", "saw", "isaw", "tri", "square"];
}
