use bevy::prelude::*;

/// Marks the entity the player controls.
#[derive(Component)]
pub struct Player;

/// Spins an entity around the Z axis, in radians per second.
#[derive(Component)]
pub struct Spin {
    pub speed: f32,
}

/// Everything the game adds to the app, in one place: the systems, the resources and the
/// startup work. `main` only has to know about this plugin.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, spin);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands.spawn((
        Player,
        Spin { speed: 1.5 },
        Sprite {
            color: Color::srgb(0.9, 0.45, 0.2),
            custom_size: Some(Vec2::splat(160.0)),
            ..default()
        },
    ));
}

fn spin(time: Res<Time>, mut query: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut query {
        transform.rotate_z(spin.speed * time.delta_secs());
    }
}
