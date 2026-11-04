//! The presence indicator: a soft coloured border around the window and a small label, shown while a
//! remote client is attached. Plain UI nodes — no post-process, no shader, a handful of entities — drawn
//! above everything and ignoring the pointer so it never steals a click.
//!
//! The "blur" is four nested rings of falling opacity: cheap, and it reads as a glow.

use bevy::picking::Pickable;
use bevy::prelude::*;

use crate::RemotePresence;
use crate::presence::PresenceState;

const RINGS: usize = 4;
const RING_WIDTH: f32 = 5.0;
const RING_ALPHA: [f32; RINGS] = [0.80, 0.45, 0.2, 0.08];

#[derive(Component)]
pub struct OverlayRoot;
#[derive(Component)]
pub struct Ring(usize);
#[derive(Component)]
pub struct LabelBox;
#[derive(Component)]
pub struct LabelText;

pub fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            OverlayRoot,
            Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            GlobalZIndex(i32::MAX - 1),
            Pickable::IGNORE,
            Visibility::Hidden,
        ))
        .with_children(|root| {
            for i in 0..RINGS {
                let inset = Val::Px(i as f32 * RING_WIDTH);
                root.spawn((
                    Ring(i),
                    Node { position_type: PositionType::Absolute, left: inset, top: inset, right: inset, bottom: inset, border: UiRect::all(Val::Px(RING_WIDTH)), ..default() },
                    BorderColor::all(Color::NONE),
                    Pickable::IGNORE,
                ));
            }
            root.spawn((
                LabelBox,
                Node { position_type: PositionType::Absolute, right: Val::Px(16.0), bottom: Val::Px(12.0), padding: UiRect::axes(Val::Px(8.0), Val::Px(3.0)), ..default() },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
                Pickable::IGNORE,
            ))
            .with_children(|b| {
                b.spawn((LabelText, Text::new(""), TextFont { font_size: 12.0, ..default() }, TextColor(Color::WHITE), Pickable::IGNORE));
            });
        });
}

/// Recolours the rings and rewrites the label — only when what is shown changed.
pub fn update_overlay(
    presence: Res<RemotePresence>,
    time: Res<Time<Real>>,
    mut roots: Query<&mut Visibility, With<OverlayRoot>>,
    mut rings: Query<(&Ring, &mut BorderColor)>,
    mut labels: Query<&mut Text, With<LabelText>>,
    mut shown: Local<Option<(PresenceState, String)>>,
) {
    let visible = presence.0.visible(time.elapsed_secs_f64());
    let now = visible.map(|(state, _)| (state, presence.0.label.clone()));
    if *shown == now {
        return;
    }
    *shown = now;
    for mut v in &mut roots {
        *v = if visible.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    let Some((state, rgba)) = visible else { return };
    for (ring, mut border) in &mut rings {
        *border = BorderColor::all(Color::srgba(rgba[0], rgba[1], rgba[2], RING_ALPHA[ring.0]));
    }
    let word = match state {
        PresenceState::Acting => "acting",
        PresenceState::Paused => "paused",
        _ => "connected",
    };
    let who = if presence.0.label.is_empty() { "remote".to_string() } else { presence.0.label.clone() };
    for mut text in &mut labels {
        text.0 = format!("{who} · {word}");
    }
}
