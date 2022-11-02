// In this game, you can press buttons

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
            UVec2::splat(16),
            "palette/palette_1.png".into(),
        ))
        .add_startup_system(init)
        .add_system(interact_buttons)
        .run();
}

fn init(
    mut commands: Commands,
    mut sprites: PxAssets<PxSprite>,
    mut filters: PxAssets<PxFilter>,
    mut cursor: ResMut<PxCursor>,
) {
    commands.spawn_bundle(Camera2dBundle::default());

    let idle = filters.load("filter/invert.png");

    // Switch to an in-game cursor to show the player that they can click on things
    *cursor = PxCursor::Filter {
        idle: idle.clone(),
        left_click: filters.load("filter/invert_dim.png"),
        right_click: idle,
    };

    let button_idle = sprites.load("sprite/button_idle.png");

    // Sprite-based button
    commands
        .spawn_bundle(PxSpriteBundle::<Layer> {
            sprite: button_idle.clone(),
            position: IVec2::new(8, 4).into(),
            ..default()
        })
        .insert_bundle(PxButtonSpriteBundle {
            bounds: UVec2::new(8, 4).into(),
            idle: button_idle.clone().into(),
            hover: sprites.load("sprite/button_hover.png").into(),
            click: sprites.load("sprite/button_click.png").into(),
        })
        .insert(Button);

    // Filter-based button
    commands
        .spawn_bundle(PxSpriteBundle::<Layer> {
            sprite: button_idle,
            position: IVec2::new(8, 12).into(),
            ..default()
        })
        .insert_bundle(PxButtonFilterBundle {
            bounds: UVec2::new(8, 4).into(),
            idle: filters.load("palette/palette_1.png").into(),
            hover: filters.load("filter/hover.png").into(),
            click: filters.load("filter/click.png").into(),
        })
        .insert(Button);
}

#[derive(Component)]
struct Button;

fn interact_buttons(
    hovers: Query<(), (With<Button>, Added<PxHover>)>,
    clicks: Query<(), (With<Button>, Added<PxClick>)>,
) {
    for _ in &hovers {
        info!("Hover!");
    }

    for _ in &clicks {
        info!("Click!");
    }
}

#[px_layer]
struct Layer;
