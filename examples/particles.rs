// In this program, a particle emitter is spawned

use std::time::Duration;

use bevy::{ecs::system::EntityCommands, prelude::*};
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

    // Spawn a particle emitter
    commands.spawn_bundle(PxEmitterBundle::<Layer> {
        // Any `IntoIterator<Item = Handle<PxSprite>>` works here
        sprites: [
            sprites.load_animated("sprite/snow_1.png", 2),
            sprites.load_animated("sprite/snow_2.png", 2),
        ]
        .into(),
        // Range where the particles can spawn
        range: IRect::new((-4, 36).into(), (36, 36).into()).into(),
        // Particle lifetime
        lifetime: Duration::from_secs(30).into(),
        // Particle starting velocity
        velocity: Vec2::new(0., -2.5).into(),
        // Range of how often the particles spawn
        frequency: PxEmitterFrequency::new(Duration::from_millis(800), Duration::from_millis(1500)),
        // `PxEmitterSimulation::Simulate` spawns particles
        // as soon as the `PxEmitterBundle` is spawned, with values as if they had been spawned
        // earlier. This is useful when an emitter comes into view,
        // and you want it to look like it had been emitting particles all along.
        simulation: PxEmitterSimulation::Simulate,
        // This function is run on each particle that spawns. It is run
        // after all of the other components are added, so you can use this to override components.
        on_spawn: (|particle: &mut EntityCommands| {
            // Let's make each particle animated
            particle.insert_bundle(PxAnimationBundle {
                on_finish: PxAnimationFinishBehavior::Loop,
                ..default()
            });
        })
        .into(),
        ..default()
    });
}

#[px_layer]
struct Layer;
