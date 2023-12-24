//! Screen and rendering

use std::collections::BTreeMap;

use bevy::{
    prelude::shape::Quad,
    reflect::TypeUuid,
    render::render_resource::{
        AsBindGroup, Extent3d, ShaderRef, TextureDescriptor, TextureDimension, TextureFormat,
        TextureUsages,
    },
    sprite::{Material2d, Material2dPlugin, MaterialMesh2dBundle},
    window::{PrimaryWindow, WindowResized},
};

#[cfg(feature = "line")]
use crate::line::draw_line;
use crate::{
    animation::{copy_animation_params, draw_spatial, PxAnimationStart},
    asset::{get_asset, PxAsset},
    filter::{draw_filter, PxFilterData},
    image::{PxImage, PxImageSliceMut},
    palette::Palette,
    position::PxLayer,
    prelude::*,
    set::PxSet,
};

const SCREEN_SHADER_HANDLE: Handle<Shader> = Handle::weak_from_u128(11708045509772077871);

pub(crate) fn screen_plugin<L: PxLayer>(size: UVec2) -> impl FnOnce(&mut App) {
    move |app| {
        app.world.resource_mut::<Assets<Shader>>().insert(
            SCREEN_SHADER_HANDLE,
            Shader::from_wgsl(include_str!("screen.wgsl"), "screen.wgsl"),
        );
        app.add_plugins(Material2dPlugin::<ScreenMaterial>::default())
            .configure_set(PostUpdate, PxSet::Draw)
            .add_systems(
                Update,
                init_screen(size).run_if(resource_added::<Palette>()),
            )
            .add_systems(
                PostUpdate,
                (
                    update_screen,
                    (
                        (
                            clear_screen,
                            (apply_deferred, draw_screen::<L>)
                                .chain()
                                .in_set(PxSet::Draw),
                        )
                            .chain(),
                        resize_screen,
                        update_screen_palette,
                    )
                        .in_set(PxSet::Loaded),
                ),
            );
    }
}

#[derive(Clone, Resource)]
pub(crate) struct Screen {
    pub(crate) image: Handle<Image>,
    pub(crate) size: UVec2,
}

#[derive(Component)]
pub(crate) struct ScreenMarker;

#[derive(AsBindGroup, Asset, Clone, Reflect, TypeUuid)]
#[uuid = "aee2fc17-8009-487a-84ac-c8bc3826e958"]
struct ScreenMaterial {
    #[uniform(0)]
    palette: [Vec3; 256],
    #[texture(1, sample_type = "u_int")]
    image: Handle<Image>,
}

impl Material2d for ScreenMaterial {
    fn fragment_shader() -> ShaderRef {
        SCREEN_SHADER_HANDLE.into()
    }
}

fn screen_scale(screen_size: UVec2, window_size: Vec2) -> Vec2 {
    let aspect = screen_size.y as f32 / screen_size.x as f32;

    Vec2::from(match window_size.y > aspect * window_size.x {
        true => (window_size.x, window_size.x * aspect),
        false => (window_size.y / aspect, window_size.y),
    })
}

fn init_screen(
    size: UVec2,
) -> impl Fn(
    Commands,
    EventWriter<WindowResized>,
    Query<(Entity, &Window), With<PrimaryWindow>>,
    Res<Palette>,
    ResMut<Assets<Image>>,
    ResMut<Assets<Mesh>>,
    ResMut<Assets<ScreenMaterial>>,
) {
    move |mut commands,
          mut window_resized,
          windows,
          palette,
          mut images,
          mut meshes,
          mut screen_materials| {
        let mut screen_palette = [default(); 256];

        for (i, [r, g, b]) in palette.colors.iter().enumerate() {
            let [r, g, b, _] = Color::rgb_u8(*r, *g, *b).as_linear_rgba_f32();
            screen_palette[i] = Vec3::new(r, g, b);
        }

        let image = images.add(Image {
            data: vec![0; (size.x * size.y) as usize],
            texture_descriptor: TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: size.x,
                    height: size.y,
                    ..default()
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::R8Uint,
                usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                view_formats: &[TextureFormat::R8Uint],
            },
            ..default()
        });

        commands.insert_resource(Screen {
            image: image.clone(),
            size,
        });

        let (entity, window) = windows.single();
        let calculated_screen_scale =
            screen_scale(size, Vec2::new(window.width(), window.height())).extend(1.);

        commands.spawn((
            MaterialMesh2dBundle {
                mesh: meshes.add(Quad::default().into()).into(),
                material: screen_materials.add(ScreenMaterial {
                    image,
                    palette: screen_palette,
                }),
                transform: Transform::from_scale(calculated_screen_scale),
                // Ensure transform matches global_transform to ensure correct rendering for WASM
                global_transform: GlobalTransform::from_scale(calculated_screen_scale),
                ..default()
            },
            ScreenMarker,
            Name::new("Screen"),
        ));

        // I do not know why, but the screen does not display unless the window has been resized
        window_resized.send(WindowResized {
            window: entity,
            width: window.width(),
            height: window.height(),
        });
    }
}

fn resize_screen(
    mut window_resized: EventReader<WindowResized>,
    mut screens: Query<&mut Transform, With<ScreenMarker>>,
    screen: Res<Screen>,
) {
    if let Some(window_resized) = window_resized.iter().last() {
        let mut transform = screens.single_mut();

        transform.scale = screen_scale(
            screen.size,
            Vec2::new(window_resized.width, window_resized.height),
        )
        .extend(1.);
    }
}

fn clear_screen(screen: Res<Screen>, mut images: ResMut<Assets<Image>>) {
    for pixel in images.get_mut(&screen.image).unwrap().data.iter_mut() {
        *pixel = 0;
    }
}

fn draw_screen<L: PxLayer>(
    #[cfg(feature = "map")] maps: Query<(
        &TilemapSize,
        &TileStorage,
        &Handle<PxTileset>,
        &PxPosition,
        &L,
        &PxCanvas,
        &Visibility,
        Option<(
            &PxAnimationDirection,
            &PxAnimationDuration,
            &PxAnimationFinishBehavior,
            &PxAnimationFrameTransition,
            &PxAnimationStart,
        )>,
        Option<&Handle<PxFilter>>,
    )>,
    #[cfg(feature = "map")] tiles: Query<(
        &TileTextureIndex,
        &Visibility,
        Option<&Handle<PxFilter>>,
    )>,
    sprites: Query<(
        &Handle<PxSprite>,
        &PxPosition,
        &PxAnchor,
        &L,
        &PxCanvas,
        &Visibility,
        Option<(
            &PxAnimationDirection,
            &PxAnimationDuration,
            &PxAnimationFinishBehavior,
            &PxAnimationFrameTransition,
            &PxAnimationStart,
        )>,
        Option<&Handle<PxFilter>>,
    )>,
    texts: Query<(
        &PxText,
        &Handle<PxTypeface>,
        &PxRect,
        &PxAnchor,
        &L,
        &PxCanvas,
        &Visibility,
        Option<(
            &PxAnimationDirection,
            &PxAnimationDuration,
            &PxAnimationFinishBehavior,
            &PxAnimationFrameTransition,
            &PxAnimationStart,
        )>,
        Option<&Handle<PxFilter>>,
    )>,
    #[cfg(feature = "line")] lines: Query<(
        &PxLine,
        &Handle<PxFilter>,
        &PxFilterLayers<L>,
        &PxCanvas,
        &Visibility,
        Option<(
            &PxAnimationDirection,
            &PxAnimationDuration,
            &PxAnimationFinishBehavior,
            &PxAnimationFrameTransition,
            &PxAnimationStart,
        )>,
    )>,
    filters: Query<
        (
            &Handle<PxFilter>,
            &PxFilterLayers<L>,
            &Visibility,
            Option<(
                &PxAnimationDirection,
                &PxAnimationDuration,
                &PxAnimationFinishBehavior,
                &PxAnimationFrameTransition,
                &PxAnimationStart,
            )>,
        ),
        Without<PxCanvas>,
    >,
    #[cfg(feature = "map")] tilesets: Res<Assets<PxTileset>>,
    sprite_assets: Res<Assets<PxSprite>>,
    typefaces: Res<Assets<PxTypeface>>,
    filter_assets: Res<Assets<PxFilter>>,
    screen: Res<Screen>,
    camera: Res<PxCamera>,
    time: Res<Time<Real>>,
    mut images: ResMut<Assets<Image>>,
) {
    let image = images.get_mut(&screen.image).unwrap();

    #[cfg(all(feature = "line", feature = "map"))]
    let mut layer_contents =
        BTreeMap::<_, (Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>)>::default();
    #[cfg(all(not(feature = "line"), feature = "map"))]
    let mut layer_contents =
        BTreeMap::<_, (Vec<_>, Vec<_>, Vec<_>, (), Vec<_>, (), Vec<_>)>::default();
    #[cfg(all(feature = "line", not(feature = "map")))]
    let mut layer_contents =
        BTreeMap::<_, ((), Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>)>::default();
    #[cfg(all(not(feature = "line"), not(feature = "map")))]
    let mut layer_contents = BTreeMap::<_, ((), Vec<_>, Vec<_>, (), Vec<_>, (), Vec<_>)>::default();

    #[cfg(feature = "map")]
    for (size, storage, tileset, position, layer, canvas, visibility, animation, filter) in &maps {
        if let Visibility::Hidden = visibility {
            continue;
        }

        if let Some((maps, _, _, _, _, _, _)) = layer_contents.get_mut(layer) {
            maps.push((size, storage, tileset, position, canvas, animation, filter));
        } else {
            layer_contents.insert(
                layer.clone(),
                (
                    vec![(size, storage, tileset, position, canvas, animation, filter)],
                    default(),
                    default(),
                    default(),
                    default(),
                    default(),
                    default(),
                ),
            );
        }
    }

    for (sprite, position, anchor, layer, canvas, visibility, animation, filter) in &sprites {
        if let Visibility::Hidden = visibility {
            continue;
        }

        if let Some((_, sprites, _, _, _, _, _)) = layer_contents.get_mut(layer) {
            sprites.push((sprite, position, anchor, canvas, animation, filter));
        } else {
            layer_contents.insert(
                layer.clone(),
                (
                    default(),
                    vec![(sprite, position, anchor, canvas, animation, filter)],
                    default(),
                    default(),
                    default(),
                    default(),
                    default(),
                ),
            );
        }
    }

    for (text, typeface, rect, alignment, layer, canvas, visibility, animation, filter) in &texts {
        if let Visibility::Hidden = visibility {
            continue;
        }

        if let Some((_, _, texts, _, _, _, _)) = layer_contents.get_mut(layer) {
            texts.push((text, typeface, rect, alignment, canvas, animation, filter));
        } else {
            layer_contents.insert(
                layer.clone(),
                (
                    default(),
                    default(),
                    vec![(text, typeface, rect, alignment, canvas, animation, filter)],
                    default(),
                    default(),
                    default(),
                    default(),
                ),
            );
        }
    }

    #[cfg(feature = "line")]
    for (line, filter, layers, canvas, visibility, animation) in &lines {
        for (layer, clip) in match layers {
            PxFilterLayers::Single { layer, clip } => vec![(layer.clone(), *clip)],
            PxFilterLayers::Many(layers) => {
                layers.iter().map(|layer| (layer.clone(), true)).collect()
            }
            PxFilterLayers::Select(select_fn) => layer_contents
                .keys()
                .filter_map(|layer| select_fn(layer).then(|| (layer.clone(), true)))
                .collect(),
        }
        .into_iter()
        {
            if let Some((_, _, _, clip_lines, _, over_lines, _)) = layer_contents.get_mut(&layer) {
                if clip { clip_lines } else { over_lines }
                    .push((line, filter, canvas, visibility, animation));
            } else {
                let lines = vec![(line, filter, canvas, visibility, animation)];

                layer_contents.insert(
                    layer,
                    if clip {
                        (
                            default(),
                            default(),
                            default(),
                            lines,
                            default(),
                            default(),
                            default(),
                        )
                    } else {
                        (
                            default(),
                            default(),
                            default(),
                            default(),
                            default(),
                            lines,
                            default(),
                        )
                    },
                );
            }
        }
    }

    for (filter, layers, visibility, animation) in &filters {
        if let Visibility::Hidden = visibility {
            continue;
        }

        for (layer, clip) in match layers {
            PxFilterLayers::Single { layer, clip } => vec![(layer.clone(), *clip)],
            PxFilterLayers::Many(layers) => {
                layers.iter().map(|layer| (layer.clone(), true)).collect()
            }
            PxFilterLayers::Select(select_fn) => layer_contents
                .keys()
                .filter(|layer| select_fn(layer))
                .map(|layer| (layer.clone(), true))
                .collect(),
        }
        .into_iter()
        {
            if let Some((_, _, _, _, clip_filters, _, over_filters)) =
                layer_contents.get_mut(&layer)
            {
                if clip { clip_filters } else { over_filters }.push((filter, animation));
            } else {
                let filters = vec![(filter, animation)];

                layer_contents.insert(
                    layer,
                    if clip {
                        (
                            default(),
                            default(),
                            default(),
                            default(),
                            filters,
                            default(),
                            default(),
                        )
                    } else {
                        (
                            default(),
                            default(),
                            default(),
                            default(),
                            default(),
                            default(),
                            filters,
                        )
                    },
                );
            }
        }
    }

    let mut layer_image = PxImage::<Option<u8>>::empty_from_image(image);
    let mut image_slice = PxImageSliceMut::from_image_mut(image);

    #[allow(unused_variables)]
    for (_, (maps, sprites, texts, clip_lines, clip_filters, over_lines, over_filters)) in
        layer_contents.into_iter()
    {
        layer_image.clear();

        #[cfg(feature = "map")]
        for (size, storage, tileset, position, canvas, animation, map_filter) in maps.into_iter() {
            if let Some(PxAsset::Loaded { asset: tileset }) = tilesets.get(tileset) {
                let map_filter = get_asset(&filter_assets, map_filter);

                for (i, tile) in storage
                    .iter()
                    .enumerate()
                    .filter_map(|(i, tile)| tile.map(|tile| (i, tile)))
                {
                    let (TileTextureIndex(tile), visibility, tile_filter) = tiles
                        .get(tile)
                        .expect("entity in map storage is not a valid tile");

                    if let Visibility::Hidden = visibility {
                        continue;
                    }

                    draw_spatial(
                        tileset
                            .tileset
                            .get(*tile as usize)
                            .unwrap_or_else(|| panic!("tile texture index out of bounds: the len is {}, but the index is {tile}", tileset.tileset.len())),
                        (),
                        &mut layer_image,
                        (**position
                            + IVec2::new((i as u32 % size.x) as i32, (i as u32 / size.x) as i32)
                                * tileset.tile_size().as_ivec2())
                        .into(),
                        PxAnchor::BottomLeft,
                        *canvas,
                        copy_animation_params(animation, &time),
                        [get_asset(&filter_assets, tile_filter), map_filter]
                            .into_iter()
                            .flatten(),
                        *camera,
                    );
                }
            }
        }

        for (sprite, position, anchor, canvas, animation, filter) in sprites {
            if let Some(PxAsset::Loaded { asset: sprite }) = sprite_assets.get(sprite) {
                draw_spatial(
                    sprite,
                    (),
                    &mut layer_image,
                    *position,
                    *anchor,
                    *canvas,
                    copy_animation_params(animation, &time),
                    get_asset(&filter_assets, filter),
                    *camera,
                );
            }
        }

        for (text, typeface, rect, alignment, canvas, animation, filter) in texts {
            if let Some(PxAsset::Loaded { asset: typeface }) = typefaces.get(typeface) {
                let rect = match canvas {
                    PxCanvas::World => **rect - **camera,
                    PxCanvas::Camera => **rect,
                };
                let rect_size = rect.size();
                let line_count = (rect_size.y + 1) / (typeface.height + 1);

                let mut lines = Vec::default();
                let mut line = Vec::default();
                let mut line_width = 0;
                let mut word = Vec::default();
                let mut word_width = 0;
                let mut separator = Vec::default();
                let mut separator_width = 0;
                for character in text.chars() {
                    let (character_width, is_separator) = typeface
                        .characters
                        .get(&character)
                        .map(|character| (character.data.width() as u32, false))
                        .unwrap_or_else(|| {
                            (
                                typeface
                                    .separators
                                    .get(&character)
                                    .expect("received character that isn't in typeface")
                                    .width,
                                true,
                            )
                        });

                    if if is_separator {
                        if line_width + separator_width + word_width - 1 > rect_size.x {
                            lines.push((line_width, line));
                            line_width = word_width - 1;
                            line = word;
                            word_width = 0;
                            word = default();
                            separator_width = character_width;
                            separator = vec![character];
                            true
                        } else if word.is_empty() {
                            separator_width += character_width;
                            separator.push(character);
                            false
                        } else {
                            line_width += separator_width + word_width - 1;
                            line.append(&mut separator);
                            line.append(&mut word);
                            word_width = 0;
                            separator_width = character_width;
                            separator = vec![character];
                            false
                        }
                    } else if word_width + character_width > rect_size.x {
                        if !line.is_empty() {
                            lines.push((line_width, line));
                            line_width = 0;
                            line = default();
                        }

                        if word_width > 0 {
                            lines.push((word_width - 1, word));
                        }
                        word_width = character_width + 1;
                        word = vec![character];
                        separator_width = 0;
                        separator = default();
                        true
                    } else {
                        word_width += character_width + 1;
                        word.push(character);
                        false
                    } && lines.len() as u32 > line_count
                    {
                        line_width = 0;
                        line.clear();
                        word_width = 0;
                        word.clear();
                        separator_width = 0;
                        separator.clear();
                        break;
                    }
                }

                if line_width + separator_width + word_width + 1 > rect_size.x {
                    lines.push((line_width, line));
                    if word_width > 0 {
                        lines.push((word_width - 1, word));
                    }
                } else if !word.is_empty() {
                    line_width += separator_width + word_width - 1;
                    line.append(&mut separator);
                    line.append(&mut word);
                    lines.push((line_width, line));
                }

                if lines.len() as u32 > line_count {
                    for _ in 0..lines.len() as u32 - line_count {
                        lines.pop();
                    }
                }

                let mut text_image = PxImage::empty(rect_size);
                let lines_height =
                    (lines.len() as u32 * typeface.height + lines.len() as u32).max(1) - 1;
                let mut line_y = alignment.y_pos(rect_size.y - lines_height)
                    + lines.len() as u32 * (typeface.height + 1);

                for (line_width, line) in lines {
                    line_y -= typeface.height + 1;
                    let mut character_x = alignment.x_pos(rect_size.x - line_width);
                    let mut was_character = false;

                    for character in line {
                        character_x += if let Some(character) = typeface.characters.get(&character)
                        {
                            was_character = true;

                            draw_spatial(
                                character,
                                (),
                                &mut text_image,
                                IVec2::new(character_x as i32, line_y as i32).into(),
                                PxAnchor::BottomLeft,
                                PxCanvas::Camera,
                                copy_animation_params(animation, &time),
                                get_asset(&filter_assets, filter),
                                *camera,
                            );

                            character.data.width() as u32 + 1
                        } else {
                            if was_character {
                                character_x -= 1;
                            }
                            was_character = false;

                            typeface.separators.get(&character).unwrap().width
                        };
                    }
                }

                if let Some(filter) = filter {
                    if let Some(PxAsset::Loaded {
                        asset: PxFilterData(filter),
                    }) = filter_assets.get(filter)
                    {
                        text_image.slice_all_mut().for_each_mut(|_, _, pixel| {
                            if let Some(pixel) = pixel {
                                *pixel = filter.pixel(IVec2::new(*pixel as i32, 0));
                            }
                        });
                    }
                }

                layer_image.slice_mut(rect).draw(&text_image);
            }
        }

        // This is where I draw the line! /j
        #[cfg(feature = "line")]
        for (line, filter, canvas, visibility, animation) in clip_lines {
            if let Visibility::Visible | Visibility::Inherited = visibility {
                if let Some(PxAsset::Loaded { asset: filter }) = filter_assets.get(filter) {
                    draw_line(
                        line,
                        filter,
                        &mut layer_image.slice_all_mut(),
                        *canvas,
                        copy_animation_params(animation, &time),
                        *camera,
                    );
                }
            }
        }

        for (filter, animation) in clip_filters {
            if let Some(PxAsset::Loaded { asset: filter }) = filter_assets.get(filter) {
                draw_filter(
                    filter,
                    copy_animation_params(animation, &time),
                    &mut layer_image.slice_all_mut(),
                );
            }
        }

        image_slice.draw(&layer_image);

        #[cfg(feature = "line")]
        for (line, filter, canvas, visibility, animation) in over_lines {
            if let Visibility::Visible | Visibility::Inherited = visibility {
                if let Some(PxAsset::Loaded { asset: filter }) = filter_assets.get(filter) {
                    draw_line(
                        line,
                        filter,
                        &mut image_slice,
                        *canvas,
                        copy_animation_params(animation, &time),
                        *camera,
                    );
                }
            }
        }

        for (filter, animation) in over_filters {
            if let Some(PxAsset::Loaded { asset: filter }) = filter_assets.get(filter) {
                draw_filter(
                    filter,
                    copy_animation_params(animation, &time),
                    &mut image_slice,
                );
            }
        }
    }
}

fn update_screen(
    screen_materials: Query<&Handle<ScreenMaterial>>,
    mut asset_events: EventWriter<AssetEvent<ScreenMaterial>>,
) {
    for handle in &screen_materials {
        asset_events.send(AssetEvent::Modified { id: handle.id() });
    }
}

fn update_screen_palette(
    screen_materials: Query<&Handle<ScreenMaterial>>,
    palette: Res<Palette>,
    mut screen_material_assets: ResMut<Assets<ScreenMaterial>>,
) {
    if !palette.is_changed() {
        return;
    }

    let mut screen_palette = [default(); 256];

    for (i, [r, g, b]) in palette.colors.iter().enumerate() {
        let [r, g, b, _] = Color::rgb_u8(*r, *g, *b).as_linear_rgba_f32();
        screen_palette[i] = Vec3::new(r, g, b);
    }

    for screen_material in &screen_materials {
        screen_material_assets
            .get_mut(screen_material)
            .unwrap()
            .palette = screen_palette;
    }
}
