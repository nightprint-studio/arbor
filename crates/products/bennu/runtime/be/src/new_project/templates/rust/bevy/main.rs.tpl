mod game;
{{#if ron}}
mod settings;
{{/if}}

use bevy::prelude::*;

use game::GamePlugin;

fn main() {
{{#if ron}}
    let settings = settings::Settings::load();
{{/if}}
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
{{#if ron}}
                title: settings.title.clone(),
{{/if}}
{{#unless ron}}
                title: "{{title}}".into(),
{{/unless}}
                ..default()
            }),
            ..default()
        }))
{{#if ron}}
        .insert_resource(ClearColor(settings.background()))
        .insert_resource(settings)
{{/if}}
        .add_plugins(GamePlugin)
        .run();
}
