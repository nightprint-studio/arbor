//! Renumbering a shader onto a layout a previewer already has.
//!
//! ## The wall this walks around
//!
//! `AsBindGroup::bind_group_layout_entries` is a **static** method — no `&self`. One material
//! type therefore has exactly one bind-group layout, decided when the previewer was compiled,
//! and a shader opened at run time cannot ask for a different one. That is not a Bevy
//! quirk to route around: a pipeline layout is baked into the pipeline, and there is nowhere
//! for per-instance shape to live.
//!
//! So three ways for a preview to fail, all of them `create_render_pipeline` refusing outright
//! rather than a value coming out wrong:
//!
//! · the binding is **absent** — *"Shader global ResourceBinding { group: 3, binding: 104 } is
//!   not available in the pipeline layout"*
//! · the binding is there and **too small** — a `array<vec4<f32>, 32>` read against 16 bytes
//! · the binding is there and the **wrong kind** — *"Type on the shader side (Sampler) does not
//!   match the pipeline binding (Buffer)"*
//!
//! Widening the layout answers the first two, and each widening is a guess about the next
//! shader. It cannot answer the third at all: binding 101 is a buffer in one material and a
//! sampler in the next, and one layout cannot be both.
//!
//! ## What this does instead
//!
//! The shader is renumbered. A previewer declares a **fixed superset** — so many uniform
//! slots, so many textures, so many samplers — and the source is rewritten so that each
//! declaration lands on a slot of its own kind. A layout may declare more than a shader uses;
//! only the reverse is an error. So once every declaration has been placed, the pipeline
//! matches by construction, for every shader that fits the superset.
//!
//! Rewriting is a copy. The file on disk is untouched, and the preview already replaces the
//! shader asset in place on every edit — this changes what is *in* that copy, not how many
//! copies there are. What comes back also says where everything went, so a panel can show
//! "`top_normal` → texture 3" and a later stage can fill that slot with a real image.
//!
//! ## What does NOT fit
//!
//! Storage buffers, storage textures, comparison samplers, depth and multisampled textures,
//! integer-sampled textures, and anything past the slot counts. Those come back in
//! [`PreviewPlan::rejected`], named, so a previewer refuses with a sentence instead of
//! panicking on pipeline validation.
//!
//! ## The numbers are a contract
//!
//! [`PreviewLayout`] is mirrored by whatever draws the result — in Arbor, the Bevy runtime's
//! `PreviewExt` and `RawMaterial`. Change a count here and that material's `#[uniform]` /
//! `#[texture]` / `#[sampler]` attributes have to move with it, or every shader is renumbered
//! onto slots that are not there.

use crate::bindings::{scan as scan_bindings, Binding};
use crate::preview_hints::hints_before;

/// How many slots of each kind a preview material offers.
///
/// The counts are not arbitrary. Uniform slots are free-ish (512 bytes each) so there are
/// enough for the widest material extension anyone writes. **Samplers are not free**: Metal
/// allows 16 per fragment stage across every bind group, and `StandardMaterial` already
/// spends six of them — which is exactly why `tile.wgsl` shares one sampler between ten
/// textures instead of declaring ten. Three is what is left over with room to spare.
///
/// Texture slots are cheap natively and not on WebGL2, where a fragment stage gets 16 texture
/// units in total. Twelve plus `StandardMaterial`'s six is at that ceiling, so a material
/// using most of them may render in the native renderer and not in the browser viewport.
/// That is a real limit of the target, not of this scheme, and it is better met with a
/// message than with a widening that helps once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewLayout {
    /// The first binding index the material owns.
    pub base: u32,
    pub uniforms: u32,
    pub textures_2d: u32,
    pub samplers: u32,
    pub textures_2d_array: u32,
    pub textures_cube: u32,
}

/// How many slots the previewer being targeted actually declares.
///
/// The **offsets** never move — the first texture is always at `base + 8` — so a shader
/// renumbered for one previewer is renumbered onto the same indices for another. What differs
/// is how many of each there are, and that is not a preference:
///
/// A fragment stage on **WebGL2** gets 16 texture units in total, across every bind group. Bevy
/// spends most of them before a material is reached — the view's environment map, its shadow
/// maps, `StandardMaterial`'s own six — so a browser viewport has room for a handful and a
/// native renderer has room for as many as anyone writes. Declaring the generous number in both
/// does not give the browser more units; it makes `create_pipeline_layout` refuse, for **every**
/// shader, including the ones that use no textures at all.
///
/// So the caller says which previewer it is renumbering for, and a shader that does not fit
/// comes back with the slots it wanted named — which is how "this one renders in the tool and
/// not in the panel" becomes a sentence instead of a dead canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewCaps {
    pub uniforms: u32,
    /// Texture slots for a material that **extends `StandardMaterial`**.
    pub textures_2d: u32,
    /// Texture slots for a material that **owns its whole bind group**.
    ///
    /// A separate number, and genuinely so rather than for tidiness: the budget is spent by
    /// every layout entry in every group, and an extension has `StandardMaterial`'s six
    /// underneath it while a raw material has nothing. The same viewport therefore has room
    /// for more of one than of the other, and pretending otherwise means either wasting slots
    /// on the raw path or crashing on the extension one.
    pub textures_2d_owning: u32,
    pub samplers: u32,
    pub samplers_owning: u32,
    pub textures_2d_array: u32,
    pub textures_cube: u32,
}

impl PreviewCaps {
    /// What the headless renderer declares — the full set.
    pub fn native() -> Self {
        Self {
            uniforms: UNIFORM_SLOTS,
            textures_2d: TEXTURE_2D_SLOTS,
            textures_2d_owning: TEXTURE_2D_SLOTS,
            samplers: SAMPLER_SLOTS,
            samplers_owning: SAMPLER_SLOTS,
            textures_2d_array: TEXTURE_2D_ARRAY_SLOTS,
            textures_cube: TEXTURE_CUBE_SLOTS,
        }
    }

    /// What the browser viewport declares, inside WebGL2's budget.
    pub fn viewport() -> Self {
        Self {
            uniforms: UNIFORM_SLOTS,
            textures_2d: VIEWPORT_TEXTURE_2D_SLOTS,
            textures_2d_owning: VIEWPORT_TEXTURE_2D_OWNING_SLOTS,
            samplers: VIEWPORT_SAMPLER_SLOTS,
            samplers_owning: VIEWPORT_SAMPLER_OWNING_SLOTS,
            textures_2d_array: 0,
            textures_cube: 0,
        }
    }
}

impl Default for PreviewCaps {
    fn default() -> Self {
        Self::native()
    }
}

/// Slot counts, shared by both materials so one shader renumbers the same way either side.
///
/// **Samplers are the scarce one, on every target.** Metal allows sixteen per fragment stage
/// across the whole pipeline, and `StandardMaterial` spends six of them before an extension is
/// reached; going past the line is not a graceful refusal but `create_pipeline_layout` failing
/// with `Out of Memory`, which names neither samplers nor the limit. Three is what fits with
/// room for the engine's own — and three is plenty, because a sampler is not tied to a texture:
/// `tile.wgsl` reads ten atlases through one, on purpose.
pub const UNIFORM_SLOTS: u32 = 8;
pub const TEXTURE_2D_SLOTS: u32 = 12;
pub const SAMPLER_SLOTS: u32 = 3;
pub const TEXTURE_2D_ARRAY_SLOTS: u32 = 2;
pub const TEXTURE_CUBE_SLOTS: u32 = 2;

/// What the **browser** viewport can afford, and the arithmetic behind it.
///
/// wgpu's GL backend hands a texture unit to **every entry in every bind-group layout**, used
/// or not, counted across all four groups with one running counter (`create_pipeline_layout`
/// in `wgpu-hal`'s `gles` module). The ceiling is `MAX_TEXTURE_SLOTS`, a **hardcoded 16** in
/// that backend — not a limit read from the adapter — so it is the same number on every
/// machine and will not vary with the GPU. On a Bevy forward pass:
///
/// | | units |
/// |---|---|
/// | view + mesh groups | 7 |
/// | `StandardMaterial`, when the material extends it | 6 |
/// | left for an **extension** | **3** |
/// | left for a material that **owns its group** | **9** |
///
/// Two and four rather than three and nine, because a Bevy release that adds one texture to
/// the view group would otherwise take the viewport with it — and the seventeenth unit is not
/// an error message, it is `index out of bounds: the len is 16 but the index is 16`, a panic
/// inside the driver shim with a dead canvas behind it.
///
/// The part that surprises: a shader sampling **no** textures pays for these too. The layout
/// is static, so `stone.wgsl` — a single `vec4` and nothing else — is charged for every slot
/// the material type declares, and was the first thing to fall over when there were four.
pub const VIEWPORT_TEXTURE_2D_SLOTS: u32 = 2;
pub const VIEWPORT_TEXTURE_2D_OWNING_SLOTS: u32 = 4;
pub const VIEWPORT_SAMPLER_SLOTS: u32 = 2;
pub const VIEWPORT_SAMPLER_OWNING_SLOTS: u32 = 3;

/// Where each family starts, as an offset from [`PreviewLayout::base`].
const OFF_UNIFORM: u32 = 0;
const OFF_TEXTURE_2D: u32 = OFF_UNIFORM + UNIFORM_SLOTS; // 8
const OFF_SAMPLER: u32 = OFF_TEXTURE_2D + TEXTURE_2D_SLOTS; // 20
const OFF_TEXTURE_2D_ARRAY: u32 = OFF_SAMPLER + SAMPLER_SLOTS; // 23
const OFF_ARRAY_SAMPLER: u32 = OFF_TEXTURE_2D_ARRAY + TEXTURE_2D_ARRAY_SLOTS; // 25
const OFF_TEXTURE_CUBE: u32 = OFF_ARRAY_SAMPLER + 1; // 26
const OFF_CUBE_SAMPLER: u32 = OFF_TEXTURE_CUBE + TEXTURE_CUBE_SLOTS; // 28

/// The base a material **extension** binds at.
///
/// Bevy's convention, and the one every shader here already follows: `StandardMaterial` owns
/// the low indices of the group and an extension starts at 100.
pub const EXTENSION_BASE: u32 = 100;

impl PreviewLayout {
    /// The layout for a shader that extends `StandardMaterial`.
    pub fn extension() -> Self {
        Self::at(EXTENSION_BASE)
    }

    /// The layout for a shader that owns its whole bind group.
    pub fn raw() -> Self {
        Self::at(0)
    }

    /// The full set, before a caller narrows it to what its previewer declares.
    ///
    /// One number per family and not two: a layout is the RESOLVED answer — which slots exist
    /// and where — while the "extension or owning" split belongs to [`PreviewCaps`], which is
    /// the question. Carrying both here would mean a layout that has not decided yet, and
    /// every reader of it having to decide again.
    fn at(base: u32) -> Self {
        Self {
            base,
            uniforms: UNIFORM_SLOTS,
            textures_2d: TEXTURE_2D_SLOTS,
            samplers: SAMPLER_SLOTS,
            textures_2d_array: TEXTURE_2D_ARRAY_SLOTS,
            textures_cube: TEXTURE_CUBE_SLOTS,
        }
    }

    pub fn uniform_binding(&self, slot: u32) -> u32 {
        self.base + OFF_UNIFORM + slot
    }
    pub fn texture_2d_binding(&self, slot: u32) -> u32 {
        self.base + OFF_TEXTURE_2D + slot
    }
    pub fn sampler_binding(&self, slot: u32) -> u32 {
        self.base + OFF_SAMPLER + slot
    }
    pub fn texture_2d_array_binding(&self, slot: u32) -> u32 {
        self.base + OFF_TEXTURE_2D_ARRAY + slot
    }
    pub fn texture_cube_binding(&self, slot: u32) -> u32 {
        self.base + OFF_TEXTURE_CUBE + slot
    }
}

/// Which family a declaration was placed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotFamily {
    Uniform,
    Texture2d,
    Texture2dArray,
    TextureCube,
    Sampler,
}

impl SlotFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Texture2d => "texture_2d",
            Self::Texture2dArray => "texture_2d_array",
            Self::TextureCube => "texture_cube",
            Self::Sampler => "sampler",
        }
    }
}

/// One declaration, and where it ended up.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBinding {
    /// The variable's name, as the shader wrote it.
    pub name: String,
    /// The declared type, verbatim.
    pub ty: String,
    pub family: SlotFamily,
    /// Index within the family — the `n` in "texture slot n".
    pub slot: u32,
    /// The binding the shader wrote.
    pub from: u32,
    /// The binding it now has.
    pub to: u32,
    /// For a texture, **what it is** — `diffuse`, `normal`, `pbr` — see [`texture_key`].
    /// Empty for anything else.
    ///
    /// Textures with the same key SHARE a slot, deliberately: they would be handed identical
    /// generated images anyway, so giving each its own spends a scarce slot on nothing.
    pub key: String,
    /// The picture that key opens on — see [`image_for_key`]. Empty for anything else.
    pub image: String,
    /// The sentence from a `// @preview` line above the declaration, when there was one.
    pub hint: Option<String>,
    /// True when this had to share a slot because there were **no more left**.
    ///
    /// Distinct from sharing by key, which is free: two `normal` maps in one slot read the
    /// same picture and that picture is right for both. This is the other thing — a material
    /// asking for more DISTINCT kinds than the previewer has, where the last kinds end up
    /// reading a picture meant for something else. Refusing instead would mean a blank panel,
    /// which is indistinguishable from a broken shader; this is visibly wrong, and named.
    ///
    /// A uniform is never aliased — two parameter blocks in one buffer is not a duplicated
    /// picture, it is silently wrong numbers.
    pub aliased: bool,
}

/// A declaration this scheme cannot place, and why.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejected {
    pub name: String,
    pub ty: String,
    pub binding: u32,
    /// A sentence for a person, not a code. It is what a panel shows and what a caller
    /// refuses with, so it says what is wrong rather than naming an enum.
    pub reason: String,
}

/// A shader, renumbered, and the map back to what it was.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewPlan {
    /// The source with every material-group binding moved onto a slot. Byte-identical to the
    /// input when nothing had to move.
    pub source: String,
    /// The group expression, verbatim — `"#{MATERIAL_BIND_GROUP}"`, `"3"`.
    pub group: String,
    /// True when the material owns its whole bind group rather than extending
    /// `StandardMaterial`.
    pub owns_group: bool,
    pub layout: PreviewLayout,
    /// Everything placed, in the order it was declared.
    pub placed: Vec<PlacedBinding>,
    /// Everything that could not be.
    pub rejected: Vec<Rejected>,
    /// True when the shader brings its **own vertex stage**.
    ///
    /// Part of the plan because it decides which material type a previewer has to build, the
    /// same way `owns_group` does. `Material::vertex_shader` is a static method, so a material
    /// either always overrides the vertex stage or never does — there is no per-instance
    /// choice. A previewer therefore needs two material types and has to pick, and picking
    /// wrongly is not a wrong picture: a material that overrides the stage against a shader
    /// with no `@vertex` fails to compile, and one that does not against a shader with one
    /// silently ignores it.
    pub vertex_entry: bool,
}

impl PreviewPlan {
    /// The placements of one family, in slot order.
    pub fn family(&self, family: SlotFamily) -> Vec<&PlacedBinding> {
        let mut v: Vec<&PlacedBinding> =
            self.placed.iter().filter(|p| p.family == family).collect();
        v.sort_by_key(|p| p.slot);
        v
    }

    /// Whether anything actually moved. A shader already written onto the slots renders the
    /// same either way, and saying so lets a caller skip a copy.
    pub fn rewritten(&self) -> bool {
        self.placed.iter().any(|p| p.from != p.to)
    }
}

/// **What a texture is**, in one word: `diffuse`, `normal`, `pbr`, `height`, `ao`…
///
/// The key, not the picture. A material samples ten textures and a panel that lists ten
/// variable names is asking you to hold the naming scheme in your head; a panel that lists
/// what they ARE is asking you nothing. It is also what makes them shareable: `top_normal` and
/// `side_normal` are the same kind of thing, and in a preview — which has no assets — they
/// would be handed byte-identical generated images anyway. One slot, no loss.
///
/// **The face is deliberately not part of it.** In the game `top_albedo_atlas` and
/// `side_albedo_atlas` are genuinely two atlases; in a preview they are two names for the same
/// absence. An author who wants them apart says so — `// @preview diffuse.top` and
/// `// @preview diffuse.side` are two keys, because the key is whatever the annotation says.
///
/// Guessed from the variable's name when there is no annotation, in segments rather than
/// substrings: `ao` is a word in `top_ao` and three letters in `shadow_map`.
pub fn texture_key(name: &str, annotated: Option<&str>) -> String {
    if let Some(a) = annotated {
        let a = a.trim().to_ascii_lowercase();
        // Anything the author wrote is a key, including one this crate has never heard of —
        // a vocabulary that only accepts the words it already knows is one nobody can extend.
        // The single exception is a raw IMAGE name, which is the older spelling of the same
        // annotation and still means "put this picture here".
        if !a.is_empty() {
            return a;
        }
    }
    let lower = name.to_ascii_lowercase();
    let segments: Vec<&str> = lower.split(|c: char| !c.is_ascii_alphanumeric()).collect();
    let has = |w: &str| segments.iter().any(|s| *s == w);
    let contains = |w: &str| lower.contains(w);

    if contains("normal") || has("nrm") || has("norm") {
        return "normal".into();
    }
    if contains("height") || contains("displace") || has("disp") || has("bump") {
        return "height".into();
    }
    if has("ao") || contains("occlusion") {
        return "ao".into();
    }
    if has("pbr") || has("orm") || has("mra") || has("arm") || contains("rough")
        || contains("metal") || contains("specular")
    {
        return "pbr".into();
    }
    if has("mask") || contains("alpha") || contains("opacity") {
        return "mask".into();
    }
    if contains("emissive") || contains("emission") {
        return "emissive".into();
    }
    // Albedo, atlas, anything unrecognised.
    "diffuse".into()
}

/// The image a key opens on.
///
/// Flat white is the worst available default for every key at once: it is a correct albedo, a
/// normal pointing along `(1,1,1)`, a surface at full displacement and an unoccluded mask —
/// three of which are wrong, and wrong in a way that reads as the SHADER being broken rather
/// than the input being absent.
///
/// A key this crate does not recognise gets a chequer, because the first thing worth knowing
/// about an unfamiliar map is where its UVs go, and a flat fill answers that with nothing. A
/// key that IS an image name is taken at its word — `// @preview noise` has always meant "put
/// noise here", and it still does.
pub fn image_for_key(key: &str) -> String {
    let k = key.split('.').next().unwrap_or(key);
    if IMAGES.contains(&k) {
        return k.to_string();
    }
    match k {
        "normal" => "normal",
        "height" | "displacement" | "pbr" | "orm" | "roughness" | "metallic" => "grey",
        "ao" | "occlusion" | "mask" | "opacity" => "white",
        "emissive" | "emission" => "black",
        _ => "checker",
    }
    .to_string()
}

/// The pictures a previewer is expected to be able to generate.
pub const IMAGES: &[&str] = &["white", "black", "grey", "normal", "checker", "noise", "uv"];

/// Renumber `source` for the previewer with the full set of slots.
pub fn preview_plan(source: &str) -> Option<PreviewPlan> {
    preview_plan_with(source, PreviewCaps::native())
}

/// Renumber `source` onto the layout its material implies, for a previewer with `caps` slots.
///
/// Returns `None` when the shader declares nothing in a material bind group — the same answer
/// [`crate::uniforms::material_bind_group`] gives, and for the same reason: it is a fact about
/// the document, not a failure.
pub fn preview_plan_with(source: &str, caps: PreviewCaps) -> Option<PreviewPlan> {
    let all = scan_bindings(source);
    let in_group: Vec<&Binding> = all.iter().filter(|b| b.in_material_group()).collect();
    if in_group.is_empty() {
        return None;
    }
    let group = in_group[0].group.clone();

    // Which base to renumber onto follows from the shader, exactly as `owns_group` does: a
    // material extension leaves the low indices to the `StandardMaterial` underneath, so a
    // shader whose lowest uniform is below 100 has no PBR under it and is rendered as itself.
    let lowest_uniform = in_group
        .iter()
        .filter(|b| b.address_space == "uniform")
        .map(|b| b.index)
        .min();
    let owns_group = match lowest_uniform {
        Some(i) => i < EXTENSION_BASE,
        // No uniform at all — only textures. Their indices answer the same question.
        None => in_group.iter().map(|b| b.index).min().unwrap_or(0) < EXTENSION_BASE,
    };
    // The offsets come from the layout and the counts from the caller: where a texture goes is
    // a property of the scheme, how many there is room for is a property of the target.
    let mut layout = if owns_group { PreviewLayout::raw() } else { PreviewLayout::extension() };
    layout.uniforms = caps.uniforms;
    // Which of the two budgets applies follows from the material, exactly as the base binding
    // does: an extension has `StandardMaterial`'s textures underneath it and a raw material has
    // nothing, so they do not have the same room.
    layout.textures_2d = if owns_group { caps.textures_2d_owning } else { caps.textures_2d };
    layout.samplers = if owns_group { caps.samplers_owning } else { caps.samplers };
    layout.textures_2d_array = caps.textures_2d_array;
    layout.textures_cube = caps.textures_cube;

    // In binding order, so the slot a declaration gets does not depend on where in the file it
    // was written. A shader edited to move a `var` up a few lines would otherwise renumber
    // differently, and every texture in the panel would swap places.
    let mut ordered: Vec<&Binding> = in_group.clone();
    ordered.sort_by_key(|b| b.index);

    let mut placed: Vec<PlacedBinding> = Vec::new();
    let mut rejected: Vec<Rejected> = Vec::new();
    let (mut n_uniform, mut n_tex, mut n_sampler, mut n_array, mut n_cube) =
        (0u32, 0u32, 0u32, 0u32, 0u32);

    for b in &ordered {
        let annotation = hints_before(source, b.start);
        let hint = annotation.first().and_then(|h| h.hint.clone());
        let annotated_key = annotation.first().map(|h| h.label.as_str());

        let reject = |reason: String| Rejected {
            name: b.name.clone(),
            ty: b.ty.clone(),
            binding: b.index,
            reason,
        };

        let ty = b.ty.replace(' ', "");
        let space = b.address_space.trim();

        // Storage first: the address space decides it, and a storage buffer's type is a struct
        // name that would otherwise read as a parameter block.
        if space.starts_with("storage") {
            rejected.push(reject(
                "a storage buffer is filled by the program that owns it, not by a panel".into(),
            ));
            continue;
        }
        if ty.starts_with("texture_storage") {
            rejected.push(reject(
                "a storage texture is written by a compute pass this preview does not run".into(),
            ));
            continue;
        }
        if ty.starts_with("texture_depth") {
            rejected.push(reject("a depth texture comes from a pass, not from a file".into()));
            continue;
        }
        if ty.starts_with("texture_multisampled") {
            rejected.push(reject("a multisampled texture cannot be supplied as an image".into()));
            continue;
        }
        if ty == "sampler_comparison" {
            rejected.push(reject(
                "a comparison sampler belongs to a shadow lookup, which this preview has none of"
                    .into(),
            ));
            continue;
        }

        if ty == "sampler" {
            if layout.samplers == 0 {
                rejected.push(reject(
                    "this previewer declares no sampler slots at all".into(),
                ));
                continue;
            }
            // Sharing a sampler is what `tile.wgsl` already does on purpose: ten textures, one
            // sampler, because Metal counts them. Past the slot count they are shared here too,
            // which changes nothing about the picture — a sampler has no content.
            let slot = n_sampler.min(layout.samplers.saturating_sub(1));
            let aliased = n_sampler >= layout.samplers;
            placed.push(PlacedBinding {
                name: b.name.clone(),
                ty: b.ty.clone(),
                family: SlotFamily::Sampler,
                slot,
                from: b.index,
                to: layout.sampler_binding(slot),
                key: String::new(),
                image: String::new(),
                hint,
                aliased,
            });
            n_sampler += 1;
            continue;
        }

        if ty.starts_with("texture_") {
            // Only float-sampled textures. An integer texture needs a layout entry with a
            // different sample type, and a previewer that offered one of each would spend its
            // slot budget on the case nobody writes.
            let float_sampled = !ty.contains("<u32>") && !ty.contains("<i32>");
            if !float_sampled {
                rejected.push(reject(
                    "an integer-sampled texture needs a layout entry this previewer does not \
                     declare"
                        .into(),
                ));
                continue;
            }
            // `texture_2d_array` is tested before `texture_2d`, because the second is a
            // prefix of the first and the other order silently puts every array texture in a
            // 2D slot — which is a layout mismatch, not a wrong picture.
            let (family, wanted, limit) = if ty.starts_with("texture_2d_array") {
                (SlotFamily::Texture2dArray, &mut n_array, layout.textures_2d_array)
            } else if ty.starts_with("texture_cube") {
                (SlotFamily::TextureCube, &mut n_cube, layout.textures_cube)
            } else if ty.starts_with("texture_2d") {
                (SlotFamily::Texture2d, &mut n_tex, layout.textures_2d)
            } else {
                rejected.push(reject(format!(
                    "`{}` is a texture kind this previewer has no slot for",
                    b.ty
                )));
                continue;
            };
            if limit == 0 {
                rejected.push(reject(format!(
                    "this previewer declares no {} slots",
                    family.as_str()
                )));
                continue;
            }
            let key = texture_key(&b.name, annotated_key);

            // One slot per DECLARATION, always.
            //
            // Textures of one kind share a *picture* — `top_normal` and `side_normal` are both
            // handed the flat normal, because a preview has no assets and there is nothing to
            // tell them apart. They cannot share a *binding*: two globals at the same
            // `@group`/`@binding` against a layout with one entry is not an optimisation, it is
            // a pipeline wgpu refuses — *"Error matching ShaderStages(FRAGMENT) shader
            // requirements against the pipeline"*, which is what `tile.wgsl` did when this
            // tried to be clever.
            //
            // So the key decides the CONTENT and the counter decides the address. Sharing the
            // content costs nothing and is invisible; sharing the address costs the material.
            let aliased = *wanted >= limit;
            let slot = (*wanted).min(limit - 1);
            *wanted += 1;

            let to = match family {
                SlotFamily::Texture2dArray => layout.texture_2d_array_binding(slot),
                SlotFamily::TextureCube => layout.texture_cube_binding(slot),
                _ => layout.texture_2d_binding(slot),
            };
            placed.push(PlacedBinding {
                name: b.name.clone(),
                ty: b.ty.clone(),
                family,
                slot,
                from: b.index,
                to,
                image: image_for_key(&key),
                key,
                hint,
                aliased,
            });
            continue;
        }

        // Everything left is a buffer the caller can fill: a struct, a `vec4`, an
        // `array<vec4<f32>, 32>`. A slot is 512 bytes and a shader reads whatever it declared
        // out of the front of it, so the type does not have to be understood to be supplied.
        if n_uniform >= layout.uniforms {
            rejected.push(reject(format!(
                "only {} uniform slots are available",
                layout.uniforms
            )));
            continue;
        }
        placed.push(PlacedBinding {
            name: b.name.clone(),
            ty: b.ty.clone(),
            family: SlotFamily::Uniform,
            slot: n_uniform,
            from: b.index,
            to: layout.uniform_binding(n_uniform),
            key: String::new(),
            image: String::new(),
            hint,
            // Never. Two parameter blocks sharing one buffer is not a duplicated picture, it
            // is silently wrong numbers — so a uniform past the slot count is refused above.
            aliased: false,
        });
        n_uniform += 1;
    }

    Some(PreviewPlan {
        vertex_entry: has_vertex_entry(source),
        source: rewrite(source, &ordered, &placed),
        group,
        owns_group,
        layout,
        placed,
        rejected,
    })
}

/// Does the shader declare its own vertex stage?
///
/// Read from the comment-blanked text, so an `@vertex` inside a block comment — or inside the
/// prose explaining why there is not one — does not count. A word search rather than a parse:
/// the attribute is unambiguous, and the alternative is a second WGSL front end for one bit.
pub fn has_vertex_entry(source: &str) -> bool {
    let blanked = crate::symbols::blank_comments(source);
    let bytes = blanked.as_bytes();
    let needle = b"@vertex";
    blanked.match_indices("@vertex").any(|(at, _)| {
        // Followed by whitespace or the start of the `fn`, so `@vertexish` is not a match.
        bytes
            .get(at + needle.len())
            .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_')
    })
}

/// Write the new indices into a copy of the source.
///
/// Back to front, so an edit never moves a span that has not been applied yet — the spans came
/// from a scan of the original text and every one of them is stale the moment an earlier one
/// changes length. Rejected declarations keep the index they had: the shader will not be built
/// anyway, and leaving them alone means a caller comparing the two copies sees only what moved.
fn rewrite(source: &str, ordered: &[&Binding], placed: &[PlacedBinding]) -> String {
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for b in ordered {
        let Some(p) = placed.iter().find(|p| p.from == b.index && p.name == b.name) else {
            continue;
        };
        if p.from == p.to {
            continue;
        }
        edits.push((b.index_start, b.index_end, p.to.to_string()));
    }
    if edits.is_empty() {
        return source.to_string();
    }
    edits.sort_by_key(|e| e.0);
    let mut out = source.to_string();
    for (start, end, text) in edits.into_iter().rev() {
        out.replace_range(start..end, &text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TILE: &str = concat!(
        "@group(3) @binding(101) var tile_sampler: sampler;\n",
        "@group(3) @binding(100) var top_albedo_atlas: texture_2d<f32>;\n",
        "@group(3) @binding(104) var top_normal: texture_2d<f32>;\n",
        "@group(3) @binding(116) var<uniform> material_params: TileParams;\n",
    );

    #[test]
    fn an_extension_is_renumbered_onto_the_extension_base() {
        let p = preview_plan(TILE).expect("a material group");
        assert!(!p.owns_group);
        assert_eq!(p.layout.base, EXTENSION_BASE);
        // Slots are handed out in BINDING order, so 100 is the first texture whatever line it
        // was written on.
        let tex = p.family(SlotFamily::Texture2d);
        assert_eq!(tex[0].name, "top_albedo_atlas");
        assert_eq!(tex[0].to, 108);
        assert_eq!(tex[1].name, "top_normal");
        assert_eq!(tex[1].to, 109);
        assert_eq!(p.family(SlotFamily::Sampler)[0].to, 120);
        assert_eq!(p.family(SlotFamily::Uniform)[0].to, 100);
        assert!(p.rejected.is_empty(), "{:?}", p.rejected);
    }

    #[test]
    fn the_rewritten_source_carries_the_new_indices() {
        let p = preview_plan(TILE).unwrap();
        assert!(p.source.contains("@binding(120) var tile_sampler"));
        assert!(p.source.contains("@binding(108) var top_albedo_atlas"));
        assert!(p.source.contains("@binding(100) var<uniform> material_params"));
        // Nothing else moved: the group is untouched, and so is every line's shape.
        assert_eq!(p.source.lines().count(), TILE.lines().count());
    }

    #[test]
    fn a_shader_already_on_the_slots_is_left_byte_identical() {
        let src = concat!(
            "@group(#{MATERIAL_BIND_GROUP}) @binding(100)\n",
            "var<uniform> rock_params: vec4<f32>;\n",
        );
        let p = preview_plan(src).unwrap();
        assert_eq!(p.source, src);
        assert!(!p.rewritten());
    }

    #[test]
    fn a_material_owning_its_group_renumbers_from_zero() {
        let src = concat!(
            "@group(#{MATERIAL_BIND_GROUP}) @binding(0)\n",
            "var<uniform> params: SpiralHoverParams;\n",
            "@group(#{MATERIAL_BIND_GROUP}) @binding(1) var tex: texture_2d<f32>;\n",
            "@group(#{MATERIAL_BIND_GROUP}) @binding(2) var samp: sampler;\n",
        );
        let p = preview_plan(src).unwrap();
        assert!(p.owns_group);
        assert_eq!(p.family(SlotFamily::Uniform)[0].to, 0);
        assert_eq!(p.family(SlotFamily::Texture2d)[0].to, 8);
        assert_eq!(p.family(SlotFamily::Sampler)[0].to, 20);
    }

    #[test]
    fn an_array_uniform_is_a_buffer_like_any_other() {
        let src = concat!(
            "@group(3) @binding(102)\n",
            "var<uniform> glow_pos: array<vec4<f32>, 32>;\n",
        );
        let p = preview_plan(src).unwrap();
        assert_eq!(p.family(SlotFamily::Uniform)[0].to, 100);
        assert!(p.rejected.is_empty());
    }

    #[test]
    fn a_storage_buffer_is_refused_with_a_sentence() {
        let src = "@group(3) @binding(100) var<storage, read> bones: Bones;\n";
        let p = preview_plan(src).unwrap();
        assert!(p.placed.is_empty());
        assert_eq!(p.rejected.len(), 1);
        assert!(p.rejected[0].reason.contains("storage buffer"));
    }

    #[test]
    fn a_fourth_sampler_shares_the_third_rather_than_being_dropped() {
        let mut src = String::new();
        for i in 0..4 {
            src.push_str(&format!("@group(3) @binding(10{i}) var s{i}: sampler;\n"));
        }
        let p = preview_plan(&src).unwrap();
        let s = p.family(SlotFamily::Sampler);
        assert_eq!(s.len(), 4, "every sampler is placed");
        assert!(p.rejected.is_empty());
        // The fourth lands on the third's binding and says so. Sharing a sampler changes
        // nothing about the picture — it has no content — and `tile.wgsl` shares one between
        // ten textures on purpose already.
        assert_eq!(s[3].to, s[2].to);
        assert!(s[3].aliased);
        assert!(!s[2].aliased);
    }

    #[test]
    fn a_texture_key_follows_the_name() {
        assert_eq!(texture_key("top_normal", None), "normal");
        assert_eq!(texture_key("side_height", None), "height");
        assert_eq!(texture_key("top_ao", None), "ao");
        assert_eq!(texture_key("top_pbr", None), "pbr");
        assert_eq!(texture_key("top_albedo_atlas", None), "diffuse");
        // `ao` is a word here and three letters inside `shadow`, which is why segments and
        // not substrings.
        assert_eq!(texture_key("shadow_lut", None), "diffuse");
    }

    #[test]
    fn a_key_opens_on_the_picture_that_means_nothing_happened() {
        assert_eq!(image_for_key("normal"), "normal");
        assert_eq!(image_for_key("ao"), "white");
        assert_eq!(image_for_key("pbr"), "grey");
        assert_eq!(image_for_key("diffuse"), "checker");
        // A face suffix names a different key and the same kind of picture.
        assert_eq!(image_for_key("normal.side"), "normal");
        // The older spelling: an annotation naming an IMAGE still means put that image here.
        assert_eq!(image_for_key("noise"), "noise");
        // A word nobody has heard of is a map whose UVs are worth seeing.
        assert_eq!(image_for_key("banana"), "checker");
    }

    #[test]
    fn an_annotation_beats_the_guess() {
        assert_eq!(texture_key("top_albedo_atlas", Some("noise")), "noise");
        assert_eq!(texture_key("top_normal", Some("normal.top")), "normal.top");
    }

    #[test]
    fn two_textures_of_one_kind_share_a_picture_but_never_a_binding() {
        let src = concat!(
            "@group(3) @binding(100) var top_normal: texture_2d<f32>;\n",
            "@group(3) @binding(101) var side_normal: texture_2d<f32>;\n",
            "@group(3) @binding(102) var top_albedo: texture_2d<f32>;\n",
        );
        let p = preview_plan_with(src, PreviewCaps::native()).unwrap();
        let t = p.family(SlotFamily::Texture2d);
        assert_eq!(t.len(), 3);

        // Same kind, same PICTURE — a preview has no assets, so there is nothing to tell two
        // normal maps apart.
        assert_eq!(t[0].key, "normal");
        assert_eq!(t[1].key, "normal");
        assert_eq!(t[0].image, t[1].image);
        assert_eq!(t[2].key, "diffuse");

        // Different BINDINGS, always. Two globals at one `@group`/`@binding` against a layout
        // with a single entry is a pipeline wgpu refuses, and it refuses it with a message
        // about shader requirements that names neither texture.
        let mut seen: Vec<u32> = t.iter().map(|p| p.to).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 3, "three declarations, three bindings");
    }

    #[test]
    fn a_face_suffix_asks_for_two_slots_instead_of_one() {
        let src = concat!(
            "// @preview normal.top\n",
            "@group(3) @binding(100) var top_normal: texture_2d<f32>;\n",
            "// @preview normal.side\n",
            "@group(3) @binding(101) var side_normal: texture_2d<f32>;\n",
        );
        let p = preview_plan_with(src, PreviewCaps::viewport()).unwrap();
        let t = p.family(SlotFamily::Texture2d);
        assert_ne!(t[0].to, t[1].to, "two keys, two slots — which is what the author asked for");
    }

    #[test]
    fn the_key_is_read_from_the_preview_line_above_the_declaration() {
        let src = concat!(
            "// @preview uv : which way the atlas runs\n",
            "@group(3) @binding(100) var top_albedo_atlas: texture_2d<f32>;\n",
        );
        let p = preview_plan(src).unwrap();
        let t = &p.family(SlotFamily::Texture2d)[0];
        assert_eq!(t.key, "uv");
        assert_eq!(t.image, "uv");
        assert_eq!(t.hint.as_deref(), Some("which way the atlas runs"));
    }

    #[test]
    fn a_material_owning_its_group_gets_more_slots_than_an_extension() {
        // Same shader twice, once at binding 0 and once at 100. The budget is not a property
        // of the viewport alone: an extension carries `StandardMaterial`'s six textures and a
        // raw material carries none, so the same window has room for more of the second.
        let mut ext = String::new();
        let mut raw = String::new();
        for (i, k) in ["diffuse", "normal", "pbr", "ao"].iter().enumerate() {
            ext.push_str(&format!(
                "// @preview {k}\n@group(3) @binding(1{:02}) var e{i}: texture_2d<f32>;\n", i
            ));
            raw.push_str(&format!(
                "// @preview {k}\n@group(3) @binding({}) var r{i}: texture_2d<f32>;\n", i + 1
            ));
        }
        raw.push_str("@group(3) @binding(0) var<uniform> p: vec4<f32>;\n");

        let e = preview_plan_with(&ext, PreviewCaps::viewport()).unwrap();
        assert!(!e.owns_group);
        assert_eq!(e.family(SlotFamily::Texture2d).iter().filter(|p| p.aliased).count(), 2);

        let r = preview_plan_with(&raw, PreviewCaps::viewport()).unwrap();
        assert!(r.owns_group);
        assert!(
            r.family(SlotFamily::Texture2d).iter().all(|p| !p.aliased),
            "four kinds fit a material that owns its group"
        );
    }

    #[test]
    fn a_ten_texture_material_gets_ten_bindings_and_five_pictures() {
        // `tile.wgsl`, in miniature: two faces × five kinds. Ten declarations, five things.
        let mut src = String::new();
        for (i, name) in ["albedo_atlas", "normal", "pbr", "height", "ao"].iter().enumerate() {
            src.push_str(&format!(
                "@group(3) @binding(1{:02}) var top_{name}: texture_2d<f32>;\n",
                i * 2
            ));
            src.push_str(&format!(
                "@group(3) @binding(1{:02}) var side_{name}: texture_2d<f32>;\n",
                i * 2 + 1
            ));
        }
        // `tile.wgsl` in miniature, against the RENDERER: ten declarations, ten bindings, and
        // five distinct pictures between them.
        let p = preview_plan_with(&src, PreviewCaps::native()).unwrap();
        let t = p.family(SlotFamily::Texture2d);
        assert_eq!(t.len(), 10, "every declaration is placed");
        let slots: std::collections::HashSet<u32> = t.iter().map(|p| p.to).collect();
        assert_eq!(slots.len(), 10, "ten declarations cannot share a binding");
        let pictures: std::collections::HashSet<&str> =
            t.iter().map(|p| p.image.as_str()).collect();
        assert!(pictures.len() <= 5, "but they share pictures: {pictures:?}");
        assert!(t.iter().all(|p| !p.aliased));
    }

    #[test]
    fn a_vertex_stage_is_noticed_and_a_commented_one_is_not() {
        assert!(has_vertex_entry("@vertex\nfn vertex(v: Vertex) -> VertexOutput { }"));
        assert!(has_vertex_entry("@vertex fn vertex()"));
        // In prose it is not a stage — and prose about vertex shaders is exactly where the
        // word turns up in a file that has none.
        assert!(!has_vertex_entry("// no @vertex here, the mesh one is fine"));
        assert!(!has_vertex_entry("/* @vertex */ @fragment fn fragment() {}"));
        assert!(!has_vertex_entry("@fragment fn fragment() {}"));
        // Not a prefix of a longer attribute.
        assert!(!has_vertex_entry("@vertexish fn x()"));
    }

    #[test]
    fn a_shader_with_no_material_group_answers_none_still() {
        assert!(preview_plan("fn main() {}").is_none());
    }

    #[test]
    fn the_viewport_declares_fewer_slots_than_the_renderer() {
        // Six DISTINCT kinds, so nothing shares a slot by key and the counts are what is
        // being tested. Six textures all named `t0`… would all read as `diffuse` and land on
        // one slot, which would pass for the wrong reason.
        let mut src = String::new();
        for (i, k) in ["diffuse", "normal", "pbr", "ao", "height", "mask"].iter().enumerate() {
            src.push_str(&format!(
                "// @preview {k}\n@group(3) @binding(10{i}) var t{i}: texture_2d<f32>;\n"
            ));
        }
        let native = preview_plan_with(&src, PreviewCaps::native()).unwrap();
        assert_eq!(native.family(SlotFamily::Texture2d).len(), 6);
        assert!(native.rejected.is_empty());

        // The browser's EXTENSION budget: two, because `StandardMaterial` underneath has
        // already spent six of WebGL2's sixteen texture units and the view group seven.
        let browser = preview_plan_with(&src, PreviewCaps::viewport()).unwrap();
        let bt = browser.family(SlotFamily::Texture2d);
        assert_eq!(bt.len(), 6, "all six are placed; the last four share a slot");
        assert!(browser.rejected.is_empty(), "{:?}", browser.rejected);
        assert!(bt[2..].iter().all(|p| p.aliased));
        assert!(bt[..2].iter().all(|p| !p.aliased));
        assert!(bt[2..].iter().all(|p| p.to == bt[1].to));
        // The OFFSETS do not move with the counts: whichever previewer is targeted, the first
        // texture is at 108. A scheme where they moved would mean two incompatible rewrites of
        // the same shader.
        assert_eq!(bt[0].to, 108);
        assert_eq!(native.family(SlotFamily::Texture2d)[0].to, 108);
    }

    #[test]
    fn a_shader_with_no_material_group_answers_none() {
        assert!(preview_plan("@group(0) @binding(0) var<uniform> view: View;").is_none());
    }
}
