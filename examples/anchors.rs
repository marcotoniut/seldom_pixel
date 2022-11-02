// In this program, anchors are demonstrated

use bevy::prelude::*;
use seldom_pixel::prelude::*;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(WindowDescriptor {
            width: 512.,
            height: 512.,
            ..default()
        })
        .add_plugins(DefaultPlugins)
        .add_plugin(PxPlugin::<Layer>::new(
            UVec2::splat(32),
            "palette/palette_1.png".into(),
        ))
        .add_startup_system(init)
        .run();
}

fn init(mut commands: Commands, mut sprites: PxAssets<PxSprite>) {
    commands.spawn_bundle(Camera2dBundle::default());

    // Centered
    commands.spawn_bundle(PxSpriteBundle::<Layer> {
        sprite: sprites.load("sprite/mage.png"),
        position: IVec2::new(8, 16).into(),
        ..default()
    });

    // Bottom Left
    commands.spawn_bundle(PxSpriteBundle::<Layer> {
        sprite: sprites.load("sprite/mage.png"),
        position: IVec2::splat(16).into(),
        anchor: PxAnchor::BottomLeft,
        ..default()
    });

    // Custom. Values range from 0 to 1, with the origin at the bottom left corner.
    commands.spawn_bundle(PxSpriteBundle::<Layer> {
        sprite: sprites.load("sprite/mage.png"),
        position: IVec2::new(24, 16).into(),
        anchor: Vec2::new(0.2, 0.8).into(),
        ..default()
    });
}

#[px_layer]
struct Layer;
