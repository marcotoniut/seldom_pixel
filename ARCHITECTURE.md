# `seldom_pixel` Architecture

_Last updated: 2025-12-26_

This document describes the current engine architecture and tracks forward-looking architectural work. For historical notes and an older implementation plan (now largely completed), see `ARCHITECTURE_REVIEW.md`.

---

## 1. High-level intent

- Bevy plugin for limited-palette 2D pixel art games.
- Assets are stored as palette indices (8-bit) rather than RGBA.
- Default rendering composites into a CPU pixel buffer, uploads as `R8Uint`, then displays via a fullscreen shader (`src/screen/node.rs`, `src/screen/draw.rs`, `src/screen/pipeline.rs`, `src/screen.wgsl`).
- Optional experimental GPU sprite path exists behind `gpu_palette` for direct sprite drawing (`src/screen/gpu_sprite.rs`, `src/gpu_sprite.wgsl`).

---

## 2. Rendering pipeline (where sprites get drawn)

### 2.1 CPU compositing path (default)

Evidence / touchpoints:

- Render graph node: `PxRenderNode<L>::run` in `src/screen/node.rs`.
- Layer drawing: `draw::draw_layers` in `src/screen/draw.rs`.
- Reused buffers: `PxRenderBuffer` in `src/screen/pipeline.rs`.
- Present shader: `src/screen.wgsl`.

Data flow:

1. `PxRenderNode<L>::run` queries render-world ECS and groups drawables by layer into a `BTreeMap<L, LayerContents>` (`src/screen/node.rs`).
2. `draw::draw_layers` draws each layer into a CPU `Image` (palette indices), applying filters, text, tilemaps, etc (`src/screen/draw.rs`).
3. The CPU `Image` is uploaded once per frame to a reused GPU `R8Uint` texture (`RenderQueue::write_texture` in `src/screen/node.rs`).
4. A fullscreen quad converts indices → RGB via palette uniform (`src/screen.wgsl` + `PxUniform` in `src/screen/pipeline.rs`).

Batching boundaries / draw calls:

- This path’s GPU work is essentially “one upload + one draw” per frame (present pass in `src/screen/node.rs`), regardless of sprite count. SpriteAtlas does not reduce draw calls here because sprites are blitted in software.

### 2.2 GPU palette sprite path (`gpu_palette` feature)

Evidence / touchpoints:

- GPU sprite node: `PxGpuSpriteNode<L>::run` in `src/screen/gpu_sprite.rs`.
- Sprite vertex format: `SpriteVertex { position: [f32;2], uv: [f32;2], layer: u32 }` in `src/screen/gpu_sprite.rs`.
- GPU sprite shader: `src/gpu_sprite.wgsl`.
- GPU sprite texture upload: `PxSpriteGpu::prepare_asset` in `src/sprite.rs` (creates `TextureFormat::R8Uint` texture per sprite asset).

Current batching boundaries:

- UVs are per-vertex (not instanced): `SpriteVertex::layout` uses `VertexStepMode::Vertex` (`src/screen/gpu_sprite.rs`).
- Bind group is effectively “per sprite texture”: `bind_groups: HashMap<Handle<PxSpriteAsset>, BindGroup>` is built each frame, and `render_pass.set_bind_group` occurs per draw (`src/screen/gpu_sprite.rs`).
- Draw call is “per sprite”: `render_pass.draw(draw.range.clone(), 0..1)` for each sprite (`src/screen/gpu_sprite.rs`).

Eligibility constraints:

- GPU sprite drawing is disabled if a sprite has a `PxFilter` or uses `PxFrameTransition::Dither` (`gpu_sprite_supported` in `src/screen.rs`).

---

## 3. Asset model (how textures/images are represented)

### 3.1 Palette-indexed CPU images

- `PxImage` is the core CPU storage type for palette indices (`src/image.rs`).
- RGBA → palette indices conversion is centralized in `PxImage::palette_indices` (`src/image.rs`).
- `Palette` is loaded from `*.palette.png` and builds an RGB→index map (`Palette::new` in `src/palette.rs`).

### 3.2 Sprite assets and animation layout

- Sprite assets load from `*.px_sprite.png` into `PxSpriteAsset { data: PxImage, frame_size }` (`PxSpriteLoader` in `src/sprite.rs`).
- Animated sprite frames are laid out top-to-bottom in the source image (`PxSpriteAsset::draw` uses `frame_size` + `width` math; `src/sprite.rs`).
- Frame selection is via `PxFrameView` / `PxFrameControl` (`src/frame.rs`) and optionally driven by `PxAnimationPlugin` (`src/animation.rs`).

### 3.3 Tilesets and existing “atlas-like” slicing

- `PxTilesetLoader` slices a tileset sheet into a `Vec<PxSpriteAsset>` at load time (`src/map.rs`). This is “atlas as input format” rather than a runtime atlas system.

---

## 4. SpriteAtlas evaluation (feasibility + ROI)

### 4.1 Verdict

Recommended later (or specifically for the `gpu_palette` path).

Evidence:

- Default renderer composites into a CPU buffer and presents as a single fullscreen pass (`src/screen/node.rs`, `src/screen/draw.rs`). Atlasing does not reduce draw calls/binds on this path because sprites are not individually textured on the GPU.
- The GPU sprite path uploads one GPU texture per `PxSpriteAsset` (`PxSpriteGpu::prepare_asset` in `src/sprite.rs`) and draws/binds per sprite (`src/screen/gpu_sprite.rs`). Atlasing can substantially reduce texture count and bind-group changes here.

Expected wins (where they apply):

- `gpu_palette`: fewer GPU textures, fewer bind group switches, lower per-asset upload overhead.
- CPU path: minimal to no performance win; potential authoring/IO convenience only.

Complexity:

- Medium: new atlas asset + loader + (for `gpu_palette`) render asset + GPU draw-path changes to compute region UVs and bind per atlas texture.

Biggest blockers:

- GPU sprite path is intentionally limited (no filters, no dither transitions) (`src/screen.rs`), limiting the percentage of sprites that benefit.
- Correctness risk is UV/texel edge behavior (see `src/gpu_sprite.wgsl`).

---

## 5. SpriteAtlas MVP design (prebaked-only)

Non-goals for MVP:

- Runtime packing.
- Rotation / trimming / polygon meshes.
- Mipmapping + linear filtered sampling (today textures are `mip_level_count: 1` and use `textureLoad`).

Core asset types (new module suggested: `src/atlas.rs` or `src/sprite_atlas.rs`):

- `PxSpriteAtlasAsset` (`Asset`):
  - `size: UVec2`
  - `data: PxImage` (entire atlas, palette indices)
  - `regions: Vec<AtlasRegion>`
  - optional `names: HashMap<String, u32>` (authoring convenience)
- `AtlasRegion`:
  - `frames: Vec<AtlasRect>` (MVP supports animated regions via multiple rects)
  - `frame_size: UVec2` (validated)
- `AtlasRect`:
  - pixel-space `{ x, y, w, h }` (top-left origin in atlas texture space)
- `AtlasRegionId(u32)` newtype.

How entities reference atlas regions:

- Add a new component rather than changing `PxSprite`:
  - `PxAtlasSprite { atlas: Handle<PxSpriteAtlasAsset>, region: AtlasRegionId }`
  - `#[require(PxPosition, PxAnchor, DefaultLayer, PxCanvas)]` (mirrors `PxSprite` pattern in `src/sprite.rs`)

Renderer consumption:

- `gpu_palette`: compute per-vertex UVs on CPU using the selected `AtlasRect` and atlas dimensions (keeps `SpriteVertex` unchanged in `src/screen/gpu_sprite.rs`).
- CPU path (optional): implement a blit-from-atlas-rect draw path using `PxImage` reads (similar to how `PxSpriteAsset::draw` iterates pixels today).

Padding / bleed:

- For current `textureLoad` sampling, filtering bleed is not expected, but metadata should allow a “padded rect” vs “inner rect” later if linear sampling/mips are added.

---

## 6. Integration plan (incremental, minimal rewrites)

Milestone 1 — Atlas asset + loader (prebaked metadata)

- Add `PxSpriteAtlasAsset` + `AssetLoader` for something like `*.px_atlas.ron` or `*.px_atlas.json`.
- Loader pattern:
  - load atlas PNG via `ImageLoader` (see `PxSpriteLoader::load` in `src/sprite.rs`)
  - load palette via `LoadContext::loader().immediate().load::<Palette>(...)` (see `src/sprite.rs`, `src/map.rs`)
  - convert to `PxImage` via `PxImage::palette_indices` (`src/image.rs`)
  - parse region rects + validate bounds and per-region `frame_size` invariants

Milestone 2 — GPU path support (where ROI exists)

- Add `PxSpriteAtlasGpu: RenderAsset` (behind `gpu_palette`) that uploads one `R8Uint` texture for the atlas (mirror `PxSpriteGpu::prepare_asset` in `src/sprite.rs`).
- Extend `PxGpuSpriteNode<L>` (`src/screen/gpu_sprite.rs`) to:
  - query atlas sprites
  - compute UVs from atlas rects
  - cache bind groups keyed by atlas handle/texture (not per sprite region)

Milestone 3 — Animation + frame counts

- Make `PxAtlasSprite` participate in `PxFrameCount` updates (parallel to `AnimatedAssetComponent` patterns in `src/animation.rs` / `src/sprite.rs`).
- Define frame selection behavior: `PxFrameView` chooses the region frame index within `AtlasRegion.frames`.

Milestone 4 (optional) — Authoring + examples

- Add an example demonstrating atlas sprites under `gpu_palette`.
- Add minimal docs describing atlas metadata format expectations.

---

## 7. Risks / edge cases

- GPU constraints: atlas provides no benefit for sprites excluded from GPU rendering (`PxFilter`, `PxFrameTransition::Dither`) (`src/screen.rs`).
- UV and bounds correctness: atlas UV math must not produce out-of-bounds `textureLoad` coordinates (`src/gpu_sprite.wgsl`).
- Trim/rotation/pivots: common atlas packer features; defer for MVP. If needed later, they require explicit per-frame origin/pivot support to avoid jitter (ties into `Spatial::frame_size` expectations).
- Hot reload: atlas metadata and image reload must update region frame counts and prepared GPU textures without stale handle lookups (use `AssetEvent` patterns similar to `src/animation.rs`).

---

## 8. Alternatives

- Do nothing: best if most rendering remains CPU-composited (current default).
- Prebaked “atlas as input, slice into many sprites”: similar to `PxTilesetLoader` (`src/map.rs`); improves authoring but does not reduce GPU binds if you still upload per-sprite textures.
- Texture arrays: avoids padding/bleed and simplifies sampling, but is a bigger shader/pipeline change than “atlas via UVs” and forces tighter constraints on sprite dimensions.

---

## 9. Open architecture work (roadmap)

- SpriteAtlas (prebaked-only) for the `gpu_palette` path: see sections 4–6.
- GPU sprite batching improvements beyond atlasing:
  - reduce per-sprite draw calls via instancing and sorting by texture/bind group (`src/screen/gpu_sprite.rs`).
- Clarify and document palette lifecycle constraints:
  - asset-loading palette is currently a single “global” palette by policy (`PaletteHandle` + loader usage; `src/palette.rs`, `src/sprite.rs`).
  - decide whether multiple asset palettes are in-scope or a non-goal.
