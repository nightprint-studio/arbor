import { describe, expect, it } from 'vitest';
import { ICONS } from './icon-set';
import { componentIcon, entityIcon } from './icon-kinds';

describe('entityIcon', () => {
  it('uses the kind the backend found', () => {
    expect(entityIcon('camera', 0)).toBe('camera');
    expect(entityIcon('light', 3)).toBe('light');
  });

  it('falls back to a cube, a bigger one with children', () => {
    expect(entityIcon(null, 0)).toBe('entity');
    expect(entityIcon(undefined, 2)).toBe('group');
    expect(entityIcon('nonsense', 0)).toBe('entity');
  });
});

describe('componentIcon', () => {
  it('picks by the type name', () => {
    expect(componentIcon('bevy_transform::components::transform::Transform')).toBe('transform');
    expect(componentIcon('bevy_transform::components::GlobalTransform')).toBe('transform');
    expect(componentIcon('bevy_camera::components::Camera3d')).toBe('camera');
    expect(componentIcon('bevy_light::point_light::PointLight')).toBe('light');
    expect(componentIcon('bevy_ui::ui_node::Node')).toBe('ui');
    expect(componentIcon('bevy_time::time::Timer')).toBe('timer');
    expect(componentIcon('bevy_asset::handle::Handle<bevy_image::image::Image>')).toBe('handle');
    expect(componentIcon('bevy_state::state::State<game::AppState>')).toBe('state');
    expect(componentIcon('game::player::Health')).toBe('component');
  });

  it('only ever names an icon that exists', () => {
    for (const t of ['a::Transform', 'a::Camera', 'a::SpotLight', 'a::Mesh3d', 'a::Text2d', 'a::Window', 'a::Sprite', 'a::Handle<x::Y>', 'a::Foo']) {
      expect(Object.keys(ICONS)).toContain(componentIcon(t));
    }
  });
});
