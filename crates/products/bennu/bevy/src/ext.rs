//! The extension the host registers: what Bevy contributes to a project, and nothing else.

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use bennu_ext::prelude::{
    ExtEntry, ExtGutterMark, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, Diagnostic};

use crate::build::build;
use crate::catalog;
use crate::conflict::skipped_schedules;
use crate::editor;
use crate::model::BevyModel;

/// Bevy ECS support: the components, resources and events a project declares, the systems that
/// touch them, and the pairs an access conflict serialises.
pub struct BevyExtension {
    model: Mutex<Arc<BevyModel>>,
    ready: AtomicBool,
}

impl Default for BevyExtension {
    fn default() -> Self {
        Self::new()
    }
}

impl BevyExtension {
    pub fn new() -> Self {
        Self { model: Mutex::new(Arc::new(BevyModel::default())), ready: AtomicBool::new(false) }
    }

    /// The current model. Cloned `Arc` rather than held lock: a catalog query walks it for as long
    /// as it takes, and a reindex racing with one must not be blocked behind the walk.
    pub fn model(&self) -> Arc<BevyModel> {
        self.model.lock().map(|m| Arc::clone(&m)).unwrap_or_default()
    }

    fn store(&self, next: BevyModel) {
        if let Ok(mut slot) = self.model.lock() {
            *slot = Arc::new(next);
        }
        self.ready.store(true, AtomicOrdering::Release);
    }
}

impl FrameworkExtension for BevyExtension {
    fn id(&self) -> &'static str {
        "bevy"
    }

    fn display_name(&self) -> &'static str {
        "Bevy"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.bevy
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        self.store(build(scan.rust, scan.shaders));
    }

    fn is_ready(&self) -> bool {
        self.ready.load(AtomicOrdering::Acquire)
    }

    /// Warnings on the systems in this buffer that nothing orders against a system they contend
    /// with. Held to the seam's standard — under-report rather than risk a false positive — which
    /// here means only the pairs [`warnable`](crate::conflict::warnable) will stand behind.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if !self.is_ready() {
            return Vec::new();
        }
        let model = self.model();
        match ctx.extension().as_str() {
            // A `.wgsl` gets the half of the material check that belongs in it — a layout the
            // shader declares differently from the Rust that fills it. Nothing about the ECS.
            "wgsl" => editor::shader_diagnostics(&model, ctx.path, ctx.source),
            "rs" => {
                let mut out = editor::diagnostics(&model, ctx.source);
                out.extend(editor::shader_diagnostics(&model, ctx.path, ctx.source));
                out
            }
            _ => Vec::new(),
        }
    }

    /// A mark beside every ECS declaration, pointing at the systems that touch it.
    ///
    /// The affordance the whole catalog exists to make unnecessary: the answer to "who writes this"
    /// belongs next to the declaration, not in a panel you have to think to open.
    fn gutter(&self, ctx: &FileCtx<'_>) -> Vec<ExtGutterMark> {
        // Before the first scan there is no model to answer from, and a mark saying "nothing
        // touches it" would be wrong rather than empty.
        if ctx.extension() != "rs" || !self.is_ready() {
            return Vec::new();
        }
        let model = self.model();
        let mut marks = editor::gutter(&model, ctx.source);
        marks.extend(editor::shader_gutter(&model, ctx.source));
        marks
    }

    /// Across the seam, in whichever direction the caret points: a shader path in a `.rs` answers
    /// with the shader, a `.wgsl` answers with the materials that run it.
    fn navigate(&self, ctx: &FileCtx<'_>, offset: usize) -> Vec<ExtTarget> {
        if !self.is_ready() {
            return Vec::new();
        }
        editor::shader_navigate(&self.model(), ctx.path, ctx.source, offset)
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        let model = self.model();
        match kind {
            "components" => catalog::components(&model),
            "systems" => catalog::systems(&model),
            "conflicts" => catalog::conflicts(&model),
            "shaders" => catalog::shaders(&model),
            _ => Vec::new(),
        }
    }

    fn stats(&self) -> Vec<ExtStat> {
        let model = self.model();
        let mut stats = vec![
            ExtStat {
                label: "Components".to_string(),
                value: model.types.len(),
                catalog: Some("components".to_string()),
            },
            ExtStat {
                label: "Systems".to_string(),
                value: model.systems.len(),
                catalog: Some("systems".to_string()),
            },
            ExtStat {
                label: "Access conflicts".to_string(),
                value: model.conflicts.len(),
                catalog: Some("conflicts".to_string()),
            },
        ];
        // Shaders, only when the project has any. A row reading "Shaders 0" on a game with no
        // materials is a panel offering to open an empty list, which is what the `rail` gate on
        // a catalog exists to avoid.
        if !model.shaders.is_empty() {
            stats.push(ExtStat {
                label: "Shaders".to_string(),
                value: model.shaders.len(),
                catalog: Some("shaders".to_string()),
            });
        }
        // A schedule too big to pair is reported rather than left to look like a clean one — the
        // count above would otherwise be an under-count nobody could see.
        let skipped = skipped_schedules(&model.systems);
        if !skipped.is_empty() {
            stats.push(ExtStat {
                label: format!("Schedules too large to pair ({})", skipped.join(", ")),
                value: skipped.len(),
                catalog: None,
            });
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bennu_ext::prelude::{ExtEntry, FileCtx, FrameworkExtension, ProjectScan, ScannedFile};
    use bennu_proto::prelude::CapabilitySet;

    use super::BevyExtension;

    fn indexed(sources: &[(&str, &str)]) -> BevyExtension {
        let rust: Vec<ScannedFile> = sources
            .iter()
            .map(|(p, t)| ScannedFile { path: PathBuf::from(p), text: (*t).to_string() })
            .collect();
        let ext = BevyExtension::new();
        ext.reindex(&ProjectScan {
            rust: &rust,
            ..ProjectScan::empty(std::path::Path::new("/p"))
        });
        ext
    }

    const GAME: &str = r#"
use bevy::prelude::*;

#[derive(Component)]
pub struct Health(pub f32);

#[derive(Component)]
struct Player;

#[derive(Resource, Default)]
struct Score { points: u32 }

#[derive(Bundle)]
struct PlayerBundle { health: Health, marker: Player }

fn damage(mut q: Query<&mut Health>, mut score: ResMut<Score>) {}

fn draw_health(q: Query<&Health, With<Player>>, score: Res<Score>) {}

fn tick(mut score: ResMut<Score>) {}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (damage, draw_health).chain());
    app.add_systems(FixedUpdate, tick.after(damage));
}
"#;

    #[test]
    fn it_applies_only_where_bevy_is_a_dependency() {
        let ext = BevyExtension::new();
        assert!(!ext.applies(&CapabilitySet::default()));
        assert!(ext.applies(&CapabilitySet { bevy: true, ..CapabilitySet::default() }));
    }

    #[test]
    fn an_unindexed_extension_answers_nothing_rather_than_panicking() {
        let ext = BevyExtension::new();
        assert!(ext.catalog("components").is_empty());
        assert!(ext.catalog("systems").is_empty());
        assert!(!ext.is_ready());
    }

    #[test]
    fn declarations_are_found_with_their_roles() {
        let model = indexed(&[("/p/src/main.rs", GAME)]).model();
        let names: Vec<&str> = model.types.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["Health", "Player", "PlayerBundle", "Score"]);
        let bundle = model.types.iter().find(|t| t.name == "PlayerBundle").unwrap();
        assert_eq!(bundle.fields, vec!["Health", "Player"]);
    }

    #[test]
    fn a_system_carries_the_schedule_it_was_registered_in() {
        let model = indexed(&[("/p/src/main.rs", GAME)]).model();
        let damage = model.systems.iter().find(|s| s.name == "damage").unwrap();
        assert_eq!(damage.schedules, vec!["Update"]);
        let tick = model.systems.iter().find(|s| s.name == "tick").unwrap();
        assert_eq!(tick.schedules, vec!["FixedUpdate"]);
        // `plugin(app: &mut App)` is not a system: no system parameter, no registration.
        assert!(model.systems.iter().all(|s| s.name != "plugin"));
    }

    #[test]
    fn the_conflicting_pair_is_the_one_that_shares_a_resource() {
        let model = indexed(&[("/p/src/main.rs", GAME)]).model();
        // `damage` and `draw_health` are both in Update and both touch Health and Score.
        let names: Vec<(String, String)> = model
            .conflicts
            .iter()
            .map(|c| (model.systems[c.a].name.clone(), model.systems[c.b].name.clone()))
            .collect();
        assert_eq!(names, vec![("damage".to_string(), "draw_health".to_string())]);
        // …and the chain that registered them is recorded, so the row is not read as a bug.
        assert_eq!(model.conflicts[0].ordering, crate::conflict::Ordering::Explicit);
    }

    /// Two systems in one schedule wanting the same resource, registered as a plain tuple: nothing
    /// says which goes first.
    const LOOSE: &str = r#"
use bevy::prelude::*;

#[derive(Resource, Default)]
struct Score { points: u32 }

fn add_points(mut score: ResMut<Score>) {}

fn show_points(score: Res<Score>) {}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (add_points, show_points));
}
"#;

    /// A buffered message, and the two systems on either side of its queue — the shape that used
    /// to report every message in a project as touched by nothing, because a reader is a read of
    /// `Messages<T>` and the row was looked up under `T`.
    const POSTBOX: &str = r#"
use bevy::prelude::*;

#[derive(Message, Clone, Copy, Debug)]
pub enum HudCommand { Pause, Stop }

fn send_commands(mut out: MessageWriter<HudCommand>) {}

fn apply_commands(mut incoming: MessageReader<HudCommand>) {}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (send_commands, apply_commands));
}
"#;

    /// The per-domain parameter layer of an engine built on Bevy: nothing in the project mentions
    /// `Res` or `MessageReader`, and every declaration still has readers and writers.
    const DOMAINED: &str = r#"
use fulcrum_domain::prelude::*;

#[derive(DomainResource, Default)]
struct Board { width: u32 }

#[derive(DomainMessage, Debug)]
enum BoardCommand { Clear }

fn resize(mut board: DomainResMutParam<Board>, mut cmds: DomainMessageReader<BoardCommand>) {}

fn draw(board: DomainResParam<Board>) {}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (resize, draw));
}
"#;

    /// Rows of `kind`, by primary label.
    fn row<'a>(rows: &'a [ExtEntry], primary: &str) -> &'a ExtEntry {
        rows.iter().find(|r| r.primary == primary).unwrap_or_else(|| panic!("no row {primary}"))
    }

    #[test]
    fn a_message_finds_the_systems_on_either_side_of_its_queue() {
        let rows = indexed(&[("/p/src/hud.rs", POSTBOX)]).catalog("components");
        let hud = row(&rows, "HudCommand");
        assert_eq!(hud.kind, "Message", "a buffered message is not an observer event");
        assert_eq!(hud.secondary, "read by 1 · written by 1", "{:?}", hud.secondary);
        let named: Vec<&str> = hud.children.iter().map(|c| c.primary.as_str()).collect();
        assert!(named.contains(&"send_commands") && named.contains(&"apply_commands"), "{named:?}");
    }

    #[test]
    fn a_declaration_behind_an_engine_wrapper_is_still_touched_by_its_systems() {
        let rows = indexed(&[("/p/src/board.rs", DOMAINED)]).catalog("components");
        let board = row(&rows, "Board");
        assert_eq!(board.kind, "Resource");
        assert_eq!(board.secondary, "read by 1 · written by 1", "{:?}", board.secondary);
        let command = row(&rows, "BoardCommand");
        assert_eq!(command.kind, "Message");
        assert_eq!(command.secondary, "read by 1", "{:?}", command.secondary);
    }

    #[test]
    fn a_marker_component_is_counted_by_what_filters_on_it() {
        const MARKER: &str = r#"
#[derive(Component)]
struct Player;

fn move_player(mut q: Query<&mut Transform, With<Player>>) {}
fn watch_others(q: Query<&Transform, Without<Player>>) {}

pub fn plugin(app: &mut App) { app.add_systems(Update, (move_player, watch_others)); }
"#;
        let ext = indexed(&[("/p/src/player.rs", MARKER)]);
        let player = ext.catalog("components").into_iter().find(|r| r.primary == "Player").unwrap();
        // Nothing reads `Player`'s data — and two systems could not work without it.
        assert_eq!(player.secondary, "filtered on by 2", "{:?}", player.secondary);
        assert!(player.children.iter().all(|c| c.kind == "filter"), "{:?}", player.children);
        // A filter is still not an access: the two systems do not contend over `Player`.
        assert!(
            ext.model().conflicts.iter().all(|c| c.reasons.iter().all(|r| r.target != "Player")),
            "a With/Without pair was read as contention"
        );
    }

    #[test]
    fn a_wrapped_write_and_a_wrapped_read_of_one_resource_contend() {
        let model = indexed(&[("/p/src/board.rs", DOMAINED)]).model();
        assert_eq!(model.conflicts.len(), 1, "{:?}", model.conflicts);
        assert_eq!(model.conflicts[0].reasons[0].target, "Board");
    }

    #[test]
    fn a_generic_resource_keeps_the_argument_that_tells_two_of_them_apart() {
        const STATES: &str = r#"
fn to_menu(mut next: ResMut<NextState<GameState>>) {}
fn to_page(mut next: ResMut<NextState<MenuPage>>) {}
pub fn plugin(app: &mut App) { app.add_systems(Update, (to_menu, to_page)); }
"#;
        let model = indexed(&[("/p/src/nav.rs", STATES)]).model();
        // Two different states: not a conflict, which dropping the type argument would have made
        // one.
        assert!(model.conflicts.is_empty(), "{:?}", model.conflicts);
        let targets: Vec<&str> =
            model.systems.iter().flat_map(|s| &s.accesses).map(|a| a.target.as_str()).collect();
        assert!(targets.contains(&"NextState<GameState>"), "{targets:?}");
    }

    #[test]
    fn an_unordered_pair_is_warned_about_on_both_of_its_systems() {
        let ext = indexed(&[("/p/src/score.rs", LOOSE)]);
        let path = PathBuf::from("/p/src/score.rs");
        let diags = ext.diagnostics(&FileCtx { path: &path, source: LOOSE });
        assert_eq!(diags.len(), 2, "{diags:?}");
        assert!(diags.iter().all(|d| d.code == "bevy.unordered-conflict"));
        assert!(diags.iter().all(|d| d.severity == "warning"));
        // The span is the system's own name, taken from the buffer in front of the user.
        for d in &diags {
            let named = &LOOSE[d.start..d.end];
            assert!(named == "add_points" || named == "show_points", "{named}");
        }
    }

    #[test]
    fn an_ordered_pair_is_not_warned_about() {
        // `damage` and `draw_health` contend, and the `.chain()` that registered them says which
        // goes first — a row in the panel, never a squiggle.
        let ext = indexed(&[("/p/src/main.rs", GAME)]);
        let path = PathBuf::from("/p/src/main.rs");
        assert!(ext.diagnostics(&FileCtx { path: &path, source: GAME }).is_empty());
    }

    #[test]
    fn a_diagnostic_follows_the_buffer_rather_than_the_indexed_copy() {
        // Indexed from one text, asked about another: the offsets must come from what is on
        // screen, or every squiggle lands where the function used to be.
        let ext = indexed(&[("/p/src/score.rs", LOOSE)]);
        let edited = format!("// a line added above\n{}", LOOSE);
        let path = PathBuf::from("/p/src/score.rs");
        let diags = ext.diagnostics(&FileCtx { path: &path, source: &edited });
        assert_eq!(diags.len(), 2);
        for d in &diags {
            let named = &edited[d.start..d.end];
            assert!(named == "add_points" || named == "show_points", "{named}");
        }
    }

    #[test]
    fn nothing_is_answered_for_a_buffer_before_the_first_scan() {
        let ext = BevyExtension::new();
        let path = PathBuf::from("/p/src/score.rs");
        assert!(ext.diagnostics(&FileCtx { path: &path, source: LOOSE }).is_empty());
        assert!(ext.gutter(&FileCtx { path: &path, source: LOOSE }).is_empty());
    }

    #[test]
    fn a_declaration_gets_a_gutter_mark_pointing_at_what_touches_it() {
        let ext = indexed(&[("/p/src/main.rs", GAME)]);
        let path = PathBuf::from("/p/src/main.rs");
        let marks = ext.gutter(&FileCtx { path: &path, source: GAME });
        let health = marks.iter().find(|m| m.tooltip.starts_with("Component")).unwrap();
        assert!(!health.targets.is_empty());
        // Never for a file it has nothing to do with.
        let other = PathBuf::from("/p/src/App.java");
        assert!(ext.gutter(&FileCtx { path: &other, source: GAME }).is_empty());
    }
}
