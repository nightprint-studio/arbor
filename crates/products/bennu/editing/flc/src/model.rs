//! The line model: two ONNX graphs from the same weights (a prefill without a cache, a decode step that extends
//! one) and a search over tokens, with an optional constraint on which *bytes* may come next — the index knows
//! member names, not tokens, so a token is allowed when every byte of it is.
//!
//! Without the `onnx` feature the types exist (so the host compiles) and [`LineModel::load`] says why it cannot.

use crate::constraint::Constraint;
#[cfg(feature = "onnx")]
use crate::constraint::ByteMask;
use crate::tokenizer::Tokenizer;

/// Most tokens the middle (the rest of the line) can have: the cap training truncated at.
pub const MAX_NEW_TOKENS: usize = 40;

#[derive(Clone, Debug)]
pub struct Candidate {
    /// The proposed rest of the line (never contains a newline).
    pub text: String,
    /// Geometric mean of the probability of the chosen tokens, `EOT` included.
    pub confidence: f32,
    /// Per generated token: the length in bytes of the text so far, and the token's probability.
    pub steps: Vec<(usize, f32)>,
}

#[cfg(not(feature = "onnx"))]
pub struct LineModel {
    pub tk: Tokenizer,
    pub ctx: usize,
}

#[cfg(not(feature = "onnx"))]
impl LineModel {
    pub fn load(_dir: &str) -> Result<Self, String> {
        Err("bennu-flc was built without the `onnx` feature".to_string())
    }

    pub fn complete(&mut self, _prompt: &[u16], _width: usize, _constraint: Option<Constraint>) -> Result<Vec<Candidate>, String> {
        Err("bennu-flc was built without the `onnx` feature".to_string())
    }
}

#[cfg(feature = "onnx")]
mod runtime {
    use super::*;
    use crate::tokenizer::{EOT, MID, NEWLINE, PRE, SUF};
    use ort::ep::CPU;
    use ort::session::{builder::GraphOptimizationLevel, Session};
    use ort::value::Tensor;
    use std::collections::HashMap;

    const LAYERS: usize = 6;
    const HEADS: usize = 6;
    const HEAD_DIM: usize = 64;
    /// Only this many of the likeliest tokens get the byte-by-byte constraint check.
    const CHECKED: usize = 48;
    type Memo = HashMap<Vec<u8>, Option<ByteMask>>;

    #[derive(Clone, Default)]
    struct Cache {
        k: Vec<f32>,
        v: Vec<f32>,
        len: usize,
    }

    impl Cache {
        /// The first `keep` positions of every layer and head (the layout is `[layer, 1, head, position, dim]`).
        fn truncated(&self, keep: usize) -> Cache {
            let cut = |src: &[f32]| -> Vec<f32> {
                let mut out = Vec::with_capacity(LAYERS * HEADS * keep * HEAD_DIM);
                for block in src.chunks(self.len * HEAD_DIM) {
                    out.extend_from_slice(&block[..keep * HEAD_DIM]);
                }
                out
            };
            Cache { k: cut(&self.k), v: cut(&self.v), len: keep }
        }
    }

    struct Beam {
        bytes: Vec<u8>,
        tokens: usize,
        log_sum: f32,
        steps: Vec<(usize, f32)>,
        cache: Cache,
        /// Logits for the token after `bytes`.
        logits: Vec<f32>,
    }

    /// `prefill` takes no cache (ort refuses zero-sized tensors), `decode` extends one.
    pub struct LineModel {
        prefill: Session,
        decode: Session,
        pub tk: Tokenizer,
        /// Positions the model has (prompt + generated must fit).
        pub ctx: usize,
        /// Which header the model was trained with (`header=2` in `flc.meta`; 1 when absent): 2 adds the `vars` line.
        pub header_version: u8,
        /// The last prompt and the state right after reading it. While someone types, the next prompt is the same
        /// or differs only near its end, so most of it is not read again; it goes with the model on a reload.
        warm: Option<Warm>,
    }

    /// What reading a prompt left behind.
    struct Warm {
        ids: Vec<u16>,
        cache: Cache,
        logits: Vec<f32>,
    }

    /// A tail longer than this is cheaper to read from scratch than to append to a truncated cache.
    const MAX_APPEND: usize = 256;

    fn err<E: std::fmt::Display>(e: E) -> String {
        e.to_string()
    }

    impl LineModel {
        /// `dir` holds `flc_prefill.onnx`, `flc.onnx`, `vocab.txt` and `flc.meta` (`ctx=768`).
        pub fn load(dir: &str) -> Result<Self, String> {
            let open = |name: &str| -> Result<Session, String> {
                Session::builder()
                    .map_err(err)?
                    .with_optimization_level(GraphOptimizationLevel::Level3)
                    .map_err(err)?
                    .with_intra_threads(2)
                    .map_err(err)?
                    // Memory, not speed: the runtime's arena keeps every block it has ever needed, so a model of
                    // 56 MB grew to ~580 MB after a few suggestions and stayed there (measured: 270 MB without,
                    // with the same answers and no slower). Allocation per call is cheap at this size.
                    .with_memory_pattern(false)
                    .map_err(err)?
                    .with_execution_providers([CPU::default().with_arena_allocator(false).build()])
                    .map_err(err)?
                    .commit_from_file(format!("{dir}/{name}"))
                    .map_err(err)
            };
            let meta = std::fs::read_to_string(format!("{dir}/flc.meta")).unwrap_or_default();
            let get = |k: &str| meta.split_whitespace().find_map(|kv| kv.strip_prefix(&format!("{k}=")).map(str::to_string));
            Ok(Self {
                prefill: open("flc_prefill.onnx")?,
                decode: open("flc.onnx")?,
                tk: Tokenizer::load(&format!("{dir}/vocab.txt"))?,
                ctx: get("ctx").and_then(|v| v.parse().ok()).unwrap_or(768),
                header_version: get("header").and_then(|v| v.parse().ok()).unwrap_or(1),
                warm: None,
            })
        }

        /// The state after reading `prompt` — from the last call's when it shares a start with it, else from scratch.
        /// Identical to a fresh read: the model is causal, so what comes before the first difference is unchanged.
        fn prefilled(&mut self, prompt: &[u16]) -> Result<(Cache, Vec<f32>), String> {
            let reusable = self.warm.take().and_then(|w| {
                let common = w.ids.iter().zip(prompt).take_while(|(a, b)| a == b).count();
                // at least one token is always read, because the logits belong to the last one
                let keep = common.min(prompt.len().saturating_sub(1));
                (keep > 0 && prompt.len() - keep <= MAX_APPEND).then_some((w, keep))
            });
            let (cache, logits) = match reusable {
                Some((w, keep)) if keep == w.ids.len() && keep == prompt.len() => (w.cache, w.logits),
                Some((w, keep)) => {
                    let mut cache = w.cache.truncated(keep);
                    let logits = self.step(&prompt[keep..], &mut cache)?;
                    (cache, logits)
                }
                None => {
                    let mut cache = Cache::default();
                    let logits = self.step(prompt, &mut cache)?;
                    (cache, logits)
                }
            };
            self.warm = Some(Warm { ids: prompt.to_vec(), cache: cache.clone(), logits: logits.clone() });
            Ok((cache, logits))
        }

        fn step(&mut self, ids: &[u16], cache: &mut Cache) -> Result<Vec<f32>, String> {
            let t = ids.len();
            let shape = [LAYERS, 1, HEADS, cache.len, HEAD_DIM];
            let ids_in = Tensor::from_array(([1usize, t], ids.iter().map(|&i| i as i64).collect::<Vec<_>>())).map_err(err)?;
            let outputs = if cache.len == 0 {
                self.prefill.run(ort::inputs!["ids" => ids_in]).map_err(err)?
            } else {
                self.decode
                    .run(ort::inputs![
                        "ids" => ids_in,
                        "past_k" => Tensor::from_array((shape, cache.k.clone())).map_err(err)?,
                        "past_v" => Tensor::from_array((shape, cache.v.clone())).map_err(err)?,
                    ])
                    .map_err(err)?
            };
            let logits = outputs["logits"].try_extract_tensor::<f32>().map_err(err)?.1.to_vec();
            cache.k = outputs["present_k"].try_extract_tensor::<f32>().map_err(err)?.1.to_vec();
            cache.v = outputs["present_v"].try_extract_tensor::<f32>().map_err(err)?.1.to_vec();
            cache.len += t;
            Ok(logits)
        }

        /// The `n` likeliest next tokens with their log-probabilities. The softmax runs over the tokens the
        /// constraint lets through (by first byte), so a refused token never wins and the probabilities stay
        /// honest; the deeper byte check only decides among the likeliest.
        fn top_tokens(&self, logits: &[f32], gen: &[u8], constraint: Option<Constraint>, n: usize, memo: &mut Memo) -> Vec<(u16, f32)> {
            let mask0 = constraint.and_then(|c| *memo.entry(gen.to_vec()).or_insert_with(|| c(gen)));
            let allowed: Vec<u16> = (0..logits.len() as u16)
                .filter(|&id| {
                    let free = id != 0 && id != NEWLINE && id != PRE && id != SUF && id != MID;
                    if !free {
                        return false;
                    }
                    match &mask0 {
                        None => true,
                        // ending the line counts as the newline byte
                        Some(m) if id == EOT => m[b'\n' as usize],
                        Some(m) => self.tk.token_bytes(id).first().is_some_and(|&b| m[b as usize]),
                    }
                })
                .collect();
            let max = allowed.iter().map(|&i| logits[i as usize]).fold(f32::MIN, f32::max);
            let norm = allowed.iter().map(|&i| (logits[i as usize] - max).exp()).sum::<f32>().ln();
            let mut ranked = allowed;
            ranked.sort_by(|&a, &b| logits[b as usize].total_cmp(&logits[a as usize]));
            let mut out = Vec::new();
            for id in ranked.into_iter().take(if mask0.is_some() { CHECKED } else { n }) {
                if let (Some(c), true) = (constraint, mask0.is_some() && id != EOT) {
                    if !self.token_fits(c, gen, self.tk.token_bytes(id), memo) {
                        continue;
                    }
                }
                out.push((id, logits[id as usize] - max - norm));
                if out.len() == n {
                    break;
                }
            }
            out
        }

        /// Whether every byte of the token is allowed where it lands.
        fn token_fits(&self, constraint: Constraint, gen: &[u8], token: &[u8], memo: &mut Memo) -> bool {
            let mut cur = gen.to_vec();
            for &b in token {
                match memo.entry(cur.clone()).or_insert_with(|| constraint(&cur)) {
                    Some(m) if !m[b as usize] => return false,
                    None => return true, // left the constrained stretch: no opinion from here on
                    _ => {}
                }
                cur.push(b);
            }
            true
        }

        /// Beam search for the rest of the line; `width` 1 is plain greedy decoding. Returns finished candidates
        /// best first (by mean log-probability).
        pub fn complete(&mut self, prompt: &[u16], width: usize, constraint: Option<Constraint>) -> Result<Vec<Candidate>, String> {
            let room = self.ctx.saturating_sub(prompt.len()).min(MAX_NEW_TOKENS);
            let (cache, logits) = self.prefilled(prompt)?;
            let mut live = vec![Beam { bytes: Vec::new(), tokens: 0, log_sum: 0.0, steps: Vec::new(), cache, logits }];
            let mut done: Vec<(f32, usize, Vec<u8>, Vec<(usize, f32)>)> = Vec::new();
            let mut memo = Memo::new();
            for _ in 0..room {
                // every live beam proposes its `width` best next tokens; the best `width` overall survive
                let mut pool: Vec<(f32, usize, u16)> = Vec::new();
                for (bi, b) in live.iter().enumerate() {
                    for (id, lp) in self.top_tokens(&b.logits, &b.bytes, constraint, width, &mut memo) {
                        pool.push((b.log_sum + lp, bi, id));
                    }
                }
                pool.sort_by(|a, b| b.0.total_cmp(&a.0));
                pool.truncate(width);
                let mut next = Vec::new();
                for (score, bi, id) in pool {
                    if id == EOT {
                        done.push((score, live[bi].tokens + 1, live[bi].bytes.clone(), live[bi].steps.clone()));
                        continue;
                    }
                    let mut cache = live[bi].cache.clone();
                    let logits = self.step(&[id], &mut cache)?;
                    let mut bytes = live[bi].bytes.clone();
                    bytes.extend_from_slice(self.tk.token_bytes(id));
                    let mut steps = live[bi].steps.clone();
                    steps.push((bytes.len(), (score - live[bi].log_sum).exp()));
                    next.push(Beam { bytes, tokens: live[bi].tokens + 1, log_sum: score, steps, cache, logits });
                }
                live = next;
                if live.is_empty() || (width == 1 && !done.is_empty()) {
                    break;
                }
            }
            done.extend(live.into_iter().map(|b| (b.log_sum, b.tokens, b.bytes, b.steps)));
            let mut out: Vec<(f32, Candidate)> = done
                .into_iter()
                .map(|(sum, tokens, bytes, steps)| {
                    let mean = sum / tokens.max(1) as f32;
                    (mean, Candidate { text: String::from_utf8_lossy(&bytes).into_owned(), confidence: if bytes.is_empty() { 0.0 } else { mean.exp() }, steps })
                })
                .collect();
            out.sort_by(|a, b| b.0.total_cmp(&a.0));
            Ok(out.into_iter().map(|(_, c)| c).collect())
        }
    }
}

#[cfg(feature = "onnx")]
pub use runtime::LineModel;
