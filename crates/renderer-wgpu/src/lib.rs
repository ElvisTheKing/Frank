#![forbid(unsafe_code)]

use std::{collections::HashMap, collections::VecDeque, mem::size_of, ops::Range};

use bytemuck::{Pod, Zeroable};
use egui::{PaintCallback, Rect};
use egui_wgpu::{Callback, CallbackResources, CallbackTrait, RenderState, ScreenDescriptor, wgpu};
use viewer_model::{ImageId, PaneId};

pub const TILE_SIZE: u32 = 512;
pub const TILE_BORDER: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneLayout {
    pub pane_id: PaneId,
    pub physical_rect: [f32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkspaceLayout {
    pub panes: Vec<PaneLayout>,
    pub physical_size: [u32; 2],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TileKey {
    pub image_id: ImageId,
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TilePixelFormat {
    Rgba8Srgb,
    Rgba16Float,
}

impl TilePixelFormat {
    #[must_use]
    pub const fn wgpu_format(self) -> wgpu::TextureFormat {
        match self {
            Self::Rgba8Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            Self::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
        }
    }
}

#[derive(Debug)]
pub struct UploadImage {
    pub image_id: ImageId,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<UploadTile>,
}

#[derive(Debug)]
pub struct UploadTile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct UploadProgress {
    pub uploaded_bytes: usize,
    pub total_bytes: usize,
    pub uploaded_tiles: usize,
    pub total_tiles: usize,
}

impl UploadProgress {
    #[must_use]
    pub const fn is_complete(self) -> bool {
        self.uploaded_tiles == self.total_tiles
    }

    #[must_use]
    pub fn fraction(self) -> f32 {
        if self.total_bytes == 0 {
            1.0
        } else {
            self.uploaded_bytes as f32 / self.total_bytes as f32
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PaneRenderState {
    pub pane_id: PaneId,
    /// Distinguishes multiple independently prepared renders in one pane.
    pub render_slot: u8,
    pub image_id: ImageId,
    pub center: [f32; 2],
    /// Logical source dimensions. An embedded preview is stretched across this
    /// coordinate space so pan and zoom do not jump when full RAW replaces it.
    pub source_size: [u32; 2],
    pub source_pixels_per_physical_pixel: f32,
    /// Clockwise display rotation around the pane center.
    pub rotation_degrees: f32,
    pub physical_size: [f32; 2],
    /// Visible region in physical pane pixels: `[left, top, right, bottom]`.
    pub clip_rect: [f32; 4],
    pub exposure_ev: f32,
    pub gamma: f32,
    pub color_gain: [f32; 3],
    pub normalization_exposure_ev: f32,
    pub normalization_gamma: [f32; 3],
    pub normalization_color_gain: [f32; 3],
}

pub struct TileRenderer {
    render_state: RenderState,
}

impl TileRenderer {
    #[must_use]
    pub fn new(render_state: RenderState) -> Self {
        let resources = ImageRenderResources::new(&render_state.device, render_state.target_format);
        render_state
            .renderer
            .write()
            .callback_resources
            .insert(resources);
        Self { render_state }
    }

    pub fn enqueue_image(&self, image: UploadImage) {
        let total_bytes = image.tiles.iter().map(|tile| tile.rgba.len()).sum();
        let total_tiles = image.tiles.len();
        let gpu_image = GpuImage {
            width: image.width,
            height: image.height,
            total_bytes,
            uploaded_bytes: 0,
            total_tiles,
            pending: image.tiles.into(),
            tiles: Vec::with_capacity(total_tiles),
        };
        self.with_resources_mut(|resources| {
            resources.images.insert(image.image_id, gpu_image);
        });
    }

    pub fn remove_image(&self, image_id: ImageId) {
        self.with_resources_mut(|resources| {
            resources.images.remove(&image_id);
        });
    }

    pub fn upload_with_budget(&self, byte_budget: usize) -> usize {
        let mut uploaded = 0;
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;
        self.with_resources_mut(|resources| {
            let image_ids: Vec<_> = resources.images.keys().copied().collect();
            for image_id in image_ids {
                loop {
                    let next_tile = resources
                        .images
                        .get_mut(&image_id)
                        .and_then(|image| image.pending.pop_front());
                    let Some(tile) = next_tile else {
                        break;
                    };
                    let tile_bytes = tile.rgba.len();
                    if uploaded > 0 && uploaded + tile_bytes > byte_budget {
                        resources
                            .images
                            .get_mut(&image_id)
                            .expect("image exists")
                            .pending
                            .push_front(tile);
                        return;
                    }
                    let gpu_tile = upload_tile(
                        device,
                        queue,
                        &resources.texture_bind_group_layout,
                        &resources.linear_sampler,
                        &resources.nearest_sampler,
                        tile,
                    );
                    let image = resources.images.get_mut(&image_id).expect("image exists");
                    image.uploaded_bytes += tile_bytes;
                    image.tiles.push(gpu_tile);
                    uploaded += tile_bytes;
                    if uploaded >= byte_budget {
                        return;
                    }
                }
            }
        });
        uploaded
    }

    #[must_use]
    pub fn upload_progress(&self, image_id: ImageId) -> Option<UploadProgress> {
        let renderer = self.render_state.renderer.read();
        let resources = renderer.callback_resources.get::<ImageRenderResources>()?;
        let image = resources.images.get(&image_id)?;
        Some(UploadProgress {
            uploaded_bytes: image.uploaded_bytes,
            total_bytes: image.total_bytes,
            uploaded_tiles: image.tiles.len(),
            total_tiles: image.total_tiles,
        })
    }

    #[must_use]
    pub fn paint_callback(&self, rect: Rect, state: PaneRenderState) -> PaintCallback {
        Callback::new_paint_callback(rect, ImagePaneCallback { state })
    }

    #[must_use]
    pub const fn required_features() -> wgpu::Features {
        wgpu::Features::empty()
    }

    fn with_resources_mut(&self, update: impl FnOnce(&mut ImageRenderResources)) {
        let mut renderer = self.render_state.renderer.write();
        let resources = renderer
            .callback_resources
            .get_mut::<ImageRenderResources>()
            .expect("tile renderer resources were registered");
        update(resources);
    }
}

struct ImagePaneCallback {
    state: PaneRenderState,
}

impl CallbackTrait for ImagePaneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources = callback_resources
            .get_mut::<ImageRenderResources>()
            .expect("tile renderer resources were registered");
        resources.prepare_pane(device, queue, self.state);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let resources = callback_resources
            .get::<ImageRenderResources>()
            .expect("tile renderer resources were registered");
        resources.paint_pane(render_pass, self.state);
    }
}

struct ImageRenderResources {
    pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    adjustment_bind_group_layout: wgpu::BindGroupLayout,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    images: HashMap<ImageId, GpuImage>,
    panes: HashMap<(PaneId, u8), PaneGpu>,
}

impl ImageRenderResources {
    fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("imagecompare-tile-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("tile.wgsl").into()),
        });
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("imagecompare-tile-texture-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let adjustment_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("imagecompare-display-adjustment-layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("imagecompare-tile-pipeline-layout"),
            bind_group_layouts: &[
                Some(&texture_bind_group_layout),
                Some(&adjustment_bind_group_layout),
            ],
            immediate_size: 0,
        });
        let vertex_attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("imagecompare-tile-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &vertex_attributes,
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("imagecompare-tile-linear-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("imagecompare-tile-nearest-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Self {
            pipeline,
            texture_bind_group_layout,
            adjustment_bind_group_layout,
            linear_sampler,
            nearest_sampler,
            images: HashMap::new(),
            panes: HashMap::new(),
        }
    }

    fn prepare_pane(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, state: PaneRenderState) {
        let Some(image) = self.images.get(&state.image_id) else {
            return;
        };
        let (vertices, draws) = build_visible_vertices(image, state);
        let required_size = (vertices.len().max(1) * size_of::<Vertex>()) as wgpu::BufferAddress;
        let pane = self
            .panes
            .entry((state.pane_id, state.render_slot))
            .or_insert_with(|| {
                let adjustment_buffer = create_adjustment_buffer(device);
                let adjustment_bind_group = create_adjustment_bind_group(
                    device,
                    &self.adjustment_bind_group_layout,
                    &adjustment_buffer,
                );
                PaneGpu {
                    vertex_buffer: create_vertex_buffer(device, required_size),
                    capacity: required_size,
                    draws: Vec::new(),
                    adjustment_buffer,
                    adjustment_bind_group,
                }
            });
        queue.write_buffer(
            &pane.adjustment_buffer,
            0,
            bytemuck::bytes_of(&DisplayAdjustment::new(
                state.exposure_ev,
                state.gamma,
                state.color_gain,
                state.normalization_exposure_ev,
                state.normalization_gamma,
                state.normalization_color_gain,
            )),
        );
        if pane.capacity < required_size {
            pane.vertex_buffer = create_vertex_buffer(device, required_size.next_power_of_two());
            pane.capacity = required_size.next_power_of_two();
        }
        if !vertices.is_empty() {
            queue.write_buffer(&pane.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        pane.draws = draws;
    }

    fn paint_pane(&self, render_pass: &mut wgpu::RenderPass<'_>, state: PaneRenderState) {
        let (Some(image), Some(pane)) = (
            self.images.get(&state.image_id),
            self.panes.get(&(state.pane_id, state.render_slot)),
        ) else {
            return;
        };
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_vertex_buffer(0, pane.vertex_buffer.slice(..));
        render_pass.set_bind_group(1, &pane.adjustment_bind_group, &[]);
        for draw in &pane.draws {
            let Some(tile) = image.tiles.get(draw.tile_index) else {
                continue;
            };
            let preview_pixels_per_physical_pixel = state.source_pixels_per_physical_pixel
                * image.width as f32
                / state.source_size[0].max(1) as f32;
            let bind_group = if use_nearest_sampling(preview_pixels_per_physical_pixel) {
                &tile.nearest_bind_group
            } else {
                &tile.linear_bind_group
            };
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(draw.vertices.clone(), 0..1);
        }
    }
}

fn use_nearest_sampling(source_pixels_per_physical_pixel: f32) -> bool {
    source_pixels_per_physical_pixel <= 1.0 + f32::EPSILON
}

fn create_adjustment_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("imagecompare-display-adjustment"),
        size: size_of::<DisplayAdjustment>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_adjustment_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("imagecompare-display-adjustment-bind-group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

fn upload_tile(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bind_group_layout: &wgpu::BindGroupLayout,
    linear_sampler: &wgpu::Sampler,
    nearest_sampler: &wgpu::Sampler,
    tile: UploadTile,
) -> GpuTile {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("imagecompare-image-tile"),
        size: wgpu::Extent3d {
            width: tile.width,
            height: tile.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &tile.rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(tile.width * 4),
            rows_per_image: Some(tile.height),
        },
        texture.size(),
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let make_bind_group = |label, sampler: &wgpu::Sampler| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    let linear_bind_group =
        make_bind_group("imagecompare-image-tile-linear-bind-group", linear_sampler);
    let nearest_bind_group = make_bind_group(
        "imagecompare-image-tile-nearest-bind-group",
        nearest_sampler,
    );
    GpuTile {
        x: tile.x,
        y: tile.y,
        width: tile.width,
        height: tile.height,
        linear_bind_group,
        nearest_bind_group,
    }
}

fn build_visible_vertices(
    image: &GpuImage,
    state: PaneRenderState,
) -> (Vec<Vertex>, Vec<DrawCommand>) {
    let mut vertices = Vec::with_capacity(image.tiles.len() * 6);
    let mut draws = Vec::with_capacity(image.tiles.len());

    for (tile_index, tile) in image.tiles.iter().enumerate() {
        if let Some(tile_vertices) = visible_tile_vertices(
            [image.width, image.height],
            [tile.x, tile.y, tile.width, tile.height],
            state,
        ) {
            let start = vertices.len() as u32;
            vertices.extend_from_slice(&tile_vertices);
            draws.push(DrawCommand {
                tile_index,
                vertices: start..vertices.len() as u32,
            });
        }
    }
    (vertices, draws)
}

fn visible_tile_vertices(
    image_size: [u32; 2],
    tile: [u32; 4],
    state: PaneRenderState,
) -> Option<Vec<Vertex>> {
    let pane_width = state.physical_size[0].max(1.0);
    let pane_height = state.physical_size[1].max(1.0);
    let source_scale = state.source_pixels_per_physical_pixel.max(1.0 / 64.0);
    let scale_x = source_scale * image_size[0] as f32 / state.source_size[0].max(1) as f32;
    let scale_y = source_scale * image_size[1] as f32 / state.source_size[1].max(1) as f32;
    let center_x = state.center[0] * image_size[0] as f32;
    let center_y = state.center[1] * image_size[1] as f32;
    let pane_center = [pane_width * 0.5, pane_height * 0.5];
    let source_to_screen = |source: [f32; 2]| {
        let unrotated = [
            pane_center[0] + (source[0] - center_x) / scale_x,
            pane_center[1] + (source[1] - center_y) / scale_y,
        ];
        rotate_screen_point(unrotated, pane_center, state.rotation_degrees)
    };
    let left = tile[0] as f32;
    let top = tile[1] as f32;
    let right = left + tile[2] as f32;
    let bottom = top + tile[3] as f32;
    let mut polygon = vec![
        ClipVertex::new(source_to_screen([left, top]), [0.0, 0.0]),
        ClipVertex::new(source_to_screen([left, bottom]), [0.0, 1.0]),
        ClipVertex::new(source_to_screen([right, bottom]), [1.0, 1.0]),
        ClipVertex::new(source_to_screen([right, top]), [1.0, 0.0]),
    ];
    let clip_left = state.clip_rect[0].clamp(0.0, pane_width);
    let clip_top = state.clip_rect[1].clamp(0.0, pane_height);
    let clip_right = state.clip_rect[2].clamp(clip_left, pane_width);
    let clip_bottom = state.clip_rect[3].clamp(clip_top, pane_height);
    for edge in [
        ClipEdge::Left(clip_left),
        ClipEdge::Right(clip_right),
        ClipEdge::Top(clip_top),
        ClipEdge::Bottom(clip_bottom),
    ] {
        polygon = clip_polygon(&polygon, edge);
    }
    if polygon.len() < 3 {
        return None;
    }
    let to_vertex = |vertex: ClipVertex| {
        Vertex::new(
            vertex.position[0] / pane_width * 2.0 - 1.0,
            1.0 - vertex.position[1] / pane_height * 2.0,
            vertex.uv[0],
            vertex.uv[1],
        )
    };
    let mut vertices = Vec::with_capacity((polygon.len() - 2) * 3);
    for index in 1..polygon.len() - 1 {
        vertices.push(to_vertex(polygon[0]));
        vertices.push(to_vertex(polygon[index]));
        vertices.push(to_vertex(polygon[index + 1]));
    }
    Some(vertices)
}

fn rotate_screen_point(point: [f32; 2], center: [f32; 2], degrees: f32) -> [f32; 2] {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let x = point[0] - center[0];
    let y = point[1] - center[1];
    [center[0] + cos * x - sin * y, center[1] + sin * x + cos * y]
}

#[derive(Clone, Copy)]
struct ClipVertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl ClipVertex {
    const fn new(position: [f32; 2], uv: [f32; 2]) -> Self {
        Self { position, uv }
    }

    fn interpolate(self, other: Self, amount: f32) -> Self {
        Self {
            position: [
                self.position[0] + (other.position[0] - self.position[0]) * amount,
                self.position[1] + (other.position[1] - self.position[1]) * amount,
            ],
            uv: [
                self.uv[0] + (other.uv[0] - self.uv[0]) * amount,
                self.uv[1] + (other.uv[1] - self.uv[1]) * amount,
            ],
        }
    }
}

#[derive(Clone, Copy)]
enum ClipEdge {
    Left(f32),
    Right(f32),
    Top(f32),
    Bottom(f32),
}

impl ClipEdge {
    fn inside(self, vertex: ClipVertex) -> bool {
        match self {
            Self::Left(value) => vertex.position[0] >= value,
            Self::Right(value) => vertex.position[0] <= value,
            Self::Top(value) => vertex.position[1] >= value,
            Self::Bottom(value) => vertex.position[1] <= value,
        }
    }

    fn intersection(self, from: ClipVertex, to: ClipVertex) -> ClipVertex {
        let (axis, value) = match self {
            Self::Left(value) | Self::Right(value) => (0, value),
            Self::Top(value) | Self::Bottom(value) => (1, value),
        };
        let distance = to.position[axis] - from.position[axis];
        let amount = if distance.abs() <= f32::EPSILON {
            0.0
        } else {
            ((value - from.position[axis]) / distance).clamp(0.0, 1.0)
        };
        from.interpolate(to, amount)
    }
}

fn clip_polygon(polygon: &[ClipVertex], edge: ClipEdge) -> Vec<ClipVertex> {
    let Some(&last) = polygon.last() else {
        return Vec::new();
    };
    let mut output = Vec::with_capacity(polygon.len() + 1);
    let mut previous = last;
    let mut previous_inside = edge.inside(previous);
    for &current in polygon {
        let current_inside = edge.inside(current);
        if current_inside != previous_inside {
            output.push(edge.intersection(previous, current));
        }
        if current_inside {
            output.push(current);
        }
        previous = current;
        previous_inside = current_inside;
    }
    output
}

fn create_vertex_buffer(device: &wgpu::Device, size: wgpu::BufferAddress) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("imagecompare-pane-vertices"),
        size: size.max(size_of::<Vertex>() as u64),
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl Vertex {
    const fn new(x: f32, y: f32, u: f32, v: f32) -> Self {
        Self {
            position: [x, y],
            uv: [u, v],
        }
    }
}

struct GpuImage {
    width: u32,
    height: u32,
    total_bytes: usize,
    uploaded_bytes: usize,
    total_tiles: usize,
    pending: VecDeque<UploadTile>,
    tiles: Vec<GpuTile>,
}

struct GpuTile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    linear_bind_group: wgpu::BindGroup,
    nearest_bind_group: wgpu::BindGroup,
}

struct PaneGpu {
    vertex_buffer: wgpu::Buffer,
    capacity: wgpu::BufferAddress,
    draws: Vec<DrawCommand>,
    adjustment_buffer: wgpu::Buffer,
    adjustment_bind_group: wgpu::BindGroup,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DisplayAdjustment {
    exposure_ev: f32,
    gamma: f32,
    color_gain_red: f32,
    color_gain_green: f32,
    color_gain_blue: f32,
    normalization_exposure_ev: f32,
    normalization_gamma_red: f32,
    normalization_gamma_green: f32,
    normalization_color_gain_red: f32,
    normalization_gamma_blue: f32,
    normalization_color_gain_green: f32,
    normalization_color_gain_blue: f32,
}

impl DisplayAdjustment {
    const fn new(
        exposure_ev: f32,
        gamma: f32,
        color_gain: [f32; 3],
        normalization_exposure_ev: f32,
        normalization_gamma: [f32; 3],
        normalization_color_gain: [f32; 3],
    ) -> Self {
        Self {
            exposure_ev,
            gamma,
            color_gain_red: color_gain[0],
            color_gain_green: color_gain[1],
            color_gain_blue: color_gain[2],
            normalization_exposure_ev,
            normalization_gamma_red: normalization_gamma[0],
            normalization_gamma_green: normalization_gamma[1],
            normalization_color_gain_red: normalization_color_gain[0],
            normalization_gamma_blue: normalization_gamma[2],
            normalization_color_gain_green: normalization_color_gain[1],
            normalization_color_gain_blue: normalization_color_gain[2],
        }
    }
}

struct DrawCommand {
    tile_index: usize,
    vertices: Range<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_state(source_size: [u32; 2], physical_size: [f32; 2]) -> PaneRenderState {
        PaneRenderState {
            pane_id: PaneId(1),
            render_slot: 0,
            image_id: ImageId(1),
            center: [0.5, 0.5],
            source_size,
            source_pixels_per_physical_pixel: 1.0,
            rotation_degrees: 0.0,
            physical_size,
            clip_rect: [0.0, 0.0, physical_size[0], physical_size[1]],
            exposure_ev: 0.0,
            gamma: 1.0,
            color_gain: [1.0; 3],
            normalization_exposure_ev: 0.0,
            normalization_gamma: [1.0; 3],
            normalization_color_gain: [1.0; 3],
        }
    }

    #[test]
    fn phase_zero_uses_portable_gpu_features_only() {
        assert!(TileRenderer::required_features().is_empty());
    }

    #[test]
    fn display_formats_are_explicit() {
        assert_eq!(
            TilePixelFormat::Rgba8Srgb.wgpu_format(),
            wgpu::TextureFormat::Rgba8UnormSrgb
        );
    }

    #[test]
    fn display_adjustment_matches_the_three_vec4_shader_layout() {
        assert_eq!(size_of::<DisplayAdjustment>(), 48);
        let adjustment = DisplayAdjustment::new(
            1.25,
            0.8,
            [0.9, 1.1, 1.2],
            -0.5,
            [1.1, 0.9, 1.2],
            [1.2, 0.8, 1.0],
        );
        assert_eq!(adjustment.exposure_ev, 1.25);
        assert_eq!(adjustment.gamma, 0.8);
        assert_eq!(adjustment.color_gain_red, 0.9);
        assert_eq!(adjustment.color_gain_green, 1.1);
        assert_eq!(adjustment.color_gain_blue, 1.2);
        assert_eq!(adjustment.normalization_exposure_ev, -0.5);
        assert_eq!(adjustment.normalization_gamma_red, 1.1);
        assert_eq!(adjustment.normalization_gamma_green, 0.9);
        assert_eq!(adjustment.normalization_gamma_blue, 1.2);
        assert_eq!(adjustment.normalization_color_gain_red, 1.2);
        assert_eq!(adjustment.normalization_color_gain_green, 0.8);
        assert_eq!(adjustment.normalization_color_gain_blue, 1.0);
    }

    #[test]
    fn one_to_one_and_closer_use_nearest_sampling() {
        assert!(use_nearest_sampling(1.0));
        assert!(use_nearest_sampling(0.5));
        assert!(!use_nearest_sampling(1.01));
    }

    #[test]
    fn upload_progress_handles_an_empty_image() {
        assert_eq!(
            UploadProgress {
                uploaded_bytes: 0,
                total_bytes: 0,
                uploaded_tiles: 0,
                total_tiles: 0,
            }
            .fraction(),
            1.0
        );
    }

    #[test]
    fn upload_progress_reports_partial_and_complete_tile_uploads() {
        let partial = UploadProgress {
            uploaded_bytes: 25,
            total_bytes: 100,
            uploaded_tiles: 1,
            total_tiles: 2,
        };
        assert!((partial.fraction() - 0.25).abs() < f32::EPSILON);
        assert!(!partial.is_complete());

        assert!(
            UploadProgress {
                uploaded_tiles: 2,
                ..partial
            }
            .is_complete()
        );
    }

    #[test]
    fn full_image_tile_maps_exactly_to_the_pane() {
        let vertices = visible_tile_vertices(
            [100, 100],
            [0, 0, 100, 100],
            pane_state([100, 100], [100.0, 100.0]),
        )
        .expect("full image is visible");
        assert_eq!(vertices[0], Vertex::new(-1.0, 1.0, 0.0, 0.0));
        assert_eq!(vertices[2], Vertex::new(1.0, -1.0, 1.0, 1.0));
        assert_eq!(vertices[5], Vertex::new(1.0, 1.0, 1.0, 0.0));
    }

    #[test]
    fn preview_tiles_use_logical_source_dimensions_and_cull_offscreen_tiles() {
        let preview = visible_tile_vertices(
            [50, 50],
            [0, 0, 50, 50],
            pane_state([100, 100], [100.0, 100.0]),
        )
        .expect("scaled preview fills the logical source area");
        assert_eq!(preview[0].position, [-1.0, 1.0]);
        assert_eq!(preview[2].position, [1.0, -1.0]);

        assert!(
            visible_tile_vertices(
                [1_000, 1_000],
                [0, 0, 100, 100],
                pane_state([1_000, 1_000], [100.0, 100.0]),
            )
            .is_none()
        );
    }

    #[test]
    fn tile_geometry_is_cropped_without_rescaling_for_split_comparison() {
        let mut state = pane_state([100, 100], [100.0, 100.0]);
        state.clip_rect = [0.0, 0.0, 40.0, 100.0];
        let vertices = visible_tile_vertices([100, 100], [0, 0, 100, 100], state)
            .expect("left split remains visible");

        let contains = |position: [f32; 2], uv: [f32; 2]| {
            vertices.iter().any(|vertex| {
                (vertex.position[0] - position[0]).abs() < 0.000_001
                    && (vertex.position[1] - position[1]).abs() < 0.000_001
                    && (vertex.uv[0] - uv[0]).abs() < 0.000_001
                    && (vertex.uv[1] - uv[1]).abs() < 0.000_001
            })
        };
        assert!(contains([-1.0, 1.0], [0.0, 0.0]));
        assert!(contains([-0.2, -1.0], [0.4, 1.0]));
        assert!(contains([-0.2, 1.0], [0.4, 0.0]));
    }

    #[test]
    fn rotation_is_applied_around_the_pane_center_before_clipping() {
        let mut state = pane_state([100, 50], [100.0, 100.0]);
        state.rotation_degrees = 90.0;
        let vertices = visible_tile_vertices([100, 50], [0, 0, 100, 50], state)
            .expect("rotated image remains visible");

        let min_x = vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = vertices
            .iter()
            .map(|vertex| vertex.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_y = vertices
            .iter()
            .map(|vertex| vertex.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x + 0.5).abs() < 0.000_001);
        assert!((max_x - 0.5).abs() < 0.000_001);
        assert!((min_y + 1.0).abs() < 0.000_001);
        assert!((max_y - 1.0).abs() < 0.000_001);

        state.clip_rect = [0.0, 0.0, 50.0, 100.0];
        let clipped = visible_tile_vertices([100, 50], [0, 0, 100, 50], state)
            .expect("left half of rotated image remains visible");
        assert!(
            clipped
                .iter()
                .all(|vertex| vertex.position[0] <= f32::EPSILON)
        );
    }

    #[test]
    fn tiles_outside_the_comparison_clip_are_culled() {
        let mut state = pane_state([100, 100], [100.0, 100.0]);
        state.clip_rect = [60.0, 0.0, 100.0, 100.0];

        assert!(visible_tile_vertices([100, 100], [0, 0, 50, 100], state).is_none());
    }
}
