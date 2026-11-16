//! wgpu-рендерер.
//!
//! Кадр:
//!   1. (если есть сцена) compute-проход march.wgsl: луч на ячейку → сразу символ,
//!      цвет символа и фон в текстуры сетки сцены;
//!   2. render-проход grid.wgsl: сетка сцены, затем слои с CPU (субтитры, HUD,
//!      экран шрифтов). Слой-«оверлей» рисуется поверх с прозрачностью.
//!
//! Атлас всех шрифтов (R8Unorm) общий и грузится один раз.

use crate::font::{ATLAS_COLS, Atlas};
use crate::grid::Grid;
use crate::sdf::{Accel, Scene};
use std::sync::Arc;
use winit::window::Window;

/// Как нарисовать сетку: где, каким шрифтом и в каком масштабе.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Прямоугольник в физических пикселях цели: x, y, w, h.
    pub rect: (u32, u32, u32, u32),
    /// Левый верхний угол сетки (внутри rect), px.
    pub origin: (i32, i32),
    /// Индекс шрифта в атласе (порядок `FontId::ALL`).
    pub font: usize,
    pub glyph: (u32, u32),
    pub scale: u32,
    pub cell: (u32, u32),
    pub gap_on: bool,
    pub gap_color: [f32; 3],
    pub bg_gain: f32,
    /// Поверх сцены с прозрачностью: альфа fg — непрозрачность символа, альфа bg — плашки.
    pub overlay: bool,
}

pub struct Layer<'a> {
    pub grid: &'a Grid,
    pub view: View,
}

/// Сетка сцены, которую считает GPU.
pub struct ScenePass<'a> {
    pub frame: &'a FrameUniform,
    pub cols: u32,
    pub rows: u32,
    pub view: View,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    origin: [i32; 2],
    grid: [u32; 2],
    cell: [u32; 2],
    glyph: [u32; 2],
    scale: u32,
    atlas_cols: u32,
    atlas_y: u32,
    gap_on: u32,
    gap_color: [f32; 4],
    bg_gain: f32,
    srgb_out: u32,
    overlay: u32,
    _pad: u32,
}

/// Параметры кадра для march.wgsl (раскладка совпадает со `struct Frame`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FrameUniform {
    pub cam_pos: [f32; 4],
    pub cam_fwd: [f32; 4],
    pub cam_right: [f32; 4],
    pub cam_up: [f32; 4],
    pub grid: [u32; 4],
    pub sun_dir: [f32; 4],
    pub sun_col: [f32; 4],
    pub sky_top: [f32; 4],
    pub sky_hz: [f32; 4],
    pub ambient: [f32; 4],
    pub style: [f32; 4],
    pub ramp_info: [u32; 4],
    pub ramp: [[u32; 4]; 8],
    pub accel_o: [f32; 4],
    pub accel_n: [u32; 4],
    /// Эффекты: затемнение, засветка, виньетка, вспышка.
    pub fx: [f32; 4],
    /// Свечение (сила, порог), подсветка фона, косинус радиуса диска луны.
    pub fx2: [f32; 4],
    /// Радиус свечения в ячейках (x).
    pub fx3: [f32; 4],
    pub lights: [[f32; 4]; 32],
}

/// Сколько точечных источников влезает в кадр.
pub const MAX_LIGHTS: usize = 16;

/// GPU-ресурсы одной сетки. Текстуры пересоздаются только при смене размера.
struct LayerGpu {
    cols: u32,
    rows: u32,
    glyph: wgpu::Texture,
    fg: wgpu::Texture,
    bg: wgpu::Texture,
    params: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

/// Сцена на GPU: формы, сетка ускорения, кадр, выходные текстуры.
struct SceneGpu {
    prims: wgpu::Buffer,
    ranges: wgpu::Buffer,
    list: wgpu::Buffer,
    frame: wgpu::Buffer,
    /// Выходная сетка, промежуточный цвет, группы привязок проходов march и post.
    out: Option<(LayerGpu, wgpu::Texture, wgpu::BindGroup, wgpu::BindGroup)>,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    pipeline_overlay: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    march: wgpu::ComputePipeline,
    march_layout: wgpu::BindGroupLayout,
    post: wgpu::ComputePipeline,
    post_layout: wgpu::BindGroupLayout,
    atlas_view: wgpu::TextureView,
    atlas_origin_y: [u32; 3],
    layers: Vec<LayerGpu>,
    scene: Option<SceneGpu>,
    pub adapter_info: String,
}

/// Предпочитаем не-sRGB формат: значения сетки уже в «экранных» единицах,
/// как в WebGL-эталоне. Если есть только sRGB — шейдер переводит в линейные.
fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(formats[0])
}

fn present_mode(vsync: bool) -> wgpu::PresentMode {
    if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync }
}

impl Renderer {
    /// Рендерер для окна.
    pub async fn for_window(window: Arc<Window>, vsync: bool, atlas: &Atlas) -> Renderer {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).expect("не удалось создать поверхность окна");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .expect("нет подходящего GPU-адаптера");
        let caps = surface.get_capabilities(&adapter);
        let format = pick_format(&caps.formats);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: present_mode(vsync),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::default(),
        };
        let mut r = Renderer::with_adapter(&adapter, format, atlas).await;
        surface.configure(&r.device, &config);
        r.surface = Some((surface, config));
        r
    }

    /// Рендерер без окна (для --screenshot).
    pub async fn headless(atlas: &Atlas) -> Renderer {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("нет GPU-адаптера (для --screenshot подойдёт и программный, например lavapipe)");
        Renderer::with_adapter(&adapter, wgpu::TextureFormat::Rgba8Unorm, atlas).await
    }

    async fn with_adapter(adapter: &wgpu::Adapter, format: wgpu::TextureFormat, atlas: &Atlas) -> Renderer {
        let info = adapter.get_info();
        let adapter_info = format!("{} ({:?})", info.name, info.backend);
        log::info!("GPU: {adapter_info}, формат вывода {format:?}");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ascii-city"),
                required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .expect("не удалось открыть GPU-устройство");

        // ---------------- вывод сетки
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });
        let tex_entry = |binding, sample_type, vis| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let buf_entry = |binding, ty, vis| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: vis,
            ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let frag = wgpu::ShaderStages::FRAGMENT;
        let unfilterable = wgpu::TextureSampleType::Float { filterable: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid"),
            entries: &[
                buf_entry(0, wgpu::BufferBindingType::Uniform, frag),
                tex_entry(1, wgpu::TextureSampleType::Uint, frag),
                tex_entry(2, unfilterable, frag),
                tex_entry(3, unfilterable, frag),
                tex_entry(4, unfilterable, frag),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let make_pipeline = |blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("grid"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let pipeline = make_pipeline(None);
        let pipeline_overlay = make_pipeline(Some(wgpu::BlendState::ALPHA_BLENDING));

        // ---------------- ray marching
        let march_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("march.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/march.wgsl").into()),
        });
        let comp = wgpu::ShaderStages::COMPUTE;
        let ro = wgpu::BufferBindingType::Storage { read_only: true };
        let storage_tex = |binding, format| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: comp,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format,
                view_dimension: wgpu::TextureViewDimension::D2,
            },
            count: None,
        };
        let march_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("march"),
            entries: &[
                buf_entry(0, wgpu::BufferBindingType::Uniform, comp),
                buf_entry(1, ro, comp),
                buf_entry(2, ro, comp),
                buf_entry(3, ro, comp),
                storage_tex(7, wgpu::TextureFormat::Rgba16Float),
            ],
        });
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                buf_entry(0, wgpu::BufferBindingType::Uniform, comp),
                storage_tex(4, wgpu::TextureFormat::R32Uint),
                storage_tex(5, wgpu::TextureFormat::Rgba8Unorm),
                storage_tex(6, wgpu::TextureFormat::Rgba8Unorm),
                tex_entry(8, unfilterable, comp),
            ],
        });
        let compute = |label, layout: &wgpu::BindGroupLayout, entry| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(label),
                layout: Some(&device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(label),
                    bind_group_layouts: &[Some(layout)],
                    immediate_size: 0,
                })),
                module: &march_shader,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let march = compute("march", &march_layout, "cs_march");
        let post = compute("post", &post_layout, "cs_post");

        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: atlas.width, height: atlas.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        write_tex(&queue, &atlas_tex, &atlas.data, atlas.width, atlas.height, 1);

        Renderer {
            atlas_view: atlas_tex.create_view(&Default::default()),
            atlas_origin_y: atlas.origin_y,
            device,
            queue,
            surface: None,
            format,
            pipeline,
            pipeline_overlay,
            layout,
            march,
            march_layout,
            post,
            post_layout,
            layers: Vec::new(),
            scene: None,
            adapter_info,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if let Some((surface, config)) = &mut self.surface {
            config.width = width.max(1);
            config.height = height.max(1);
            surface.configure(&self.device, config);
        }
    }

    /// Загрузить сцену (формы и сетку ускорения) на GPU.
    pub fn load_scene(&mut self, scene: &Scene, accel: &Accel) {
        use wgpu::util::DeviceExt;
        let storage = |label, bytes: &[u8]| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            })
        };
        let prims = if scene.prims.is_empty() { vec![bytemuck::Zeroable::zeroed()] } else { scene.prims.clone() };
        let gpu = SceneGpu {
            prims: storage("prims", bytemuck::cast_slice(&prims)),
            ranges: storage("ranges", bytemuck::cast_slice(&accel.ranges)),
            list: storage("list", bytemuck::cast_slice(&accel.list)),
            frame: self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("frame"),
                size: std::mem::size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            out: None,
        };
        self.scene = Some(gpu);
    }

    /// Обновить формы сцены на GPU (подвижная геометрия).
    pub fn update_prims(&mut self, prims: &[crate::sdf::Prim]) {
        if let Some(g) = &self.scene
            && !prims.is_empty()
        {
            self.queue.write_buffer(&g.prims, 0, bytemuck::cast_slice(prims));
        }
    }

    /// Нарисовать кадр в окно.
    pub fn render(&mut self, scene: Option<ScenePass>, layers: &[Layer]) {
        let Some((surface, config)) = &self.surface else { return };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.device, config);
                return;
            }
            other => {
                log::warn!("кадр пропущен: {other:?}");
                return;
            }
        };
        let (w, h) = (config.width, config.height);
        let view = frame.texture.create_view(&Default::default());
        self.encode(&view, w, h, scene, layers);
        self.queue.present(frame);
    }

    /// Нарисовать кадр в offscreen-текстуру и вернуть RGBA8 построчно (для --screenshot).
    pub fn render_offscreen(&mut self, w: u32, h: u32, scene: Option<ScenePass>, layers: &[Layer]) -> Vec<u8> {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.encode(&tex.create_view(&Default::default()), w, h, scene, layers);

        let row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("чтение кадра"));
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("ожидание GPU");
        let data = buf.slice(..).get_mapped_range().expect("кадр не прочитан");
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let s = (y * row) as usize;
            out.extend_from_slice(&data[s..s + (w * 4) as usize]);
        }
        out
    }

    fn params(&self, v: &View, cols: u32, rows: u32) -> Params {
        Params {
            origin: [v.rect.0 as i32 + v.origin.0, v.rect.1 as i32 + v.origin.1],
            grid: [cols, rows],
            cell: [v.cell.0, v.cell.1],
            glyph: [v.glyph.0, v.glyph.1],
            scale: v.scale,
            atlas_cols: ATLAS_COLS,
            atlas_y: self.atlas_origin_y[v.font],
            gap_on: v.gap_on as u32,
            gap_color: [v.gap_color[0], v.gap_color[1], v.gap_color[2], 1.0],
            bg_gain: v.bg_gain,
            srgb_out: self.format.is_srgb() as u32,
            overlay: v.overlay as u32,
            _pad: 0,
        }
    }

    fn encode(&mut self, target: &wgpu::TextureView, tw: u32, th: u32, scene: Option<ScenePass>, layers: &[Layer]) {
        // ---------------- слои с CPU: загрузка
        while self.layers.len() < layers.len() {
            let g = Grid::new(1, 1);
            self.layers.push(self.create_layer(g.cols, g.rows, false));
        }
        for (k, l) in layers.iter().enumerate() {
            if self.layers[k].cols != l.grid.cols || self.layers[k].rows != l.grid.rows {
                self.layers[k] = self.create_layer(l.grid.cols, l.grid.rows, false);
            }
            let gpu = &self.layers[k];
            let (c, r) = (l.grid.cols, l.grid.rows);
            write_tex(&self.queue, &gpu.glyph, l.grid.glyph_bytes(), c, r, 2);
            write_tex(&self.queue, &gpu.fg, l.grid.fg_bytes(), c, r, 4);
            write_tex(&self.queue, &gpu.bg, l.grid.bg_bytes(), c, r, 4);
            let p = self.params(&l.view, c, r);
            self.queue.write_buffer(&gpu.params, 0, bytemuck::bytes_of(&p));
        }

        let mut enc = self.device.create_command_encoder(&Default::default());
        // ---------------- сцена: ray marching
        let scene_view = match (&scene, self.scene.is_some()) {
            (Some(s), true) if s.cols > 0 && s.rows > 0 => {
                let need = self
                    .scene
                    .as_ref()
                    .and_then(|g| g.out.as_ref())
                    .is_none_or(|(o, ..)| o.cols != s.cols || o.rows != s.rows);
                if need {
                    let out = self.create_layer(s.cols, s.rows, true);
                    let color = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("scene-color"),
                        size: wgpu::Extent3d { width: s.cols, height: s.rows, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba16Float,
                        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let cv = color.create_view(&Default::default());
                    let gpu = self.scene.as_ref().expect("сцена");
                    let views = [&out.glyph, &out.fg, &out.bg].map(|t| t.create_view(&Default::default()));
                    fn tv(b: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
                        wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(v) }
                    }
                    let bind_march = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("march"),
                        layout: &self.march_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: gpu.frame.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 1, resource: gpu.prims.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 2, resource: gpu.ranges.as_entire_binding() },
                            wgpu::BindGroupEntry { binding: 3, resource: gpu.list.as_entire_binding() },
                            tv(7, &cv),
                        ],
                    });
                    let bind_post = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("post"),
                        layout: &self.post_layout,
                        entries: &[
                            wgpu::BindGroupEntry { binding: 0, resource: gpu.frame.as_entire_binding() },
                            tv(4, &views[0]),
                            tv(5, &views[1]),
                            tv(6, &views[2]),
                            tv(8, &cv),
                        ],
                    });
                    self.scene.as_mut().expect("сцена").out = Some((out, color, bind_march, bind_post));
                }
                let p = self.params(&s.view, s.cols, s.rows);
                let gpu = self.scene.as_ref().expect("сцена");
                let (out, _, bind_march, bind_post) = gpu.out.as_ref().expect("выход сцены");
                self.queue.write_buffer(&gpu.frame, 0, bytemuck::bytes_of(s.frame));
                self.queue.write_buffer(&out.params, 0, bytemuck::bytes_of(&p));
                let groups = (s.cols.div_ceil(8), s.rows.div_ceil(8));
                for (pipe, bind, label) in [(&self.march, bind_march, "march"), (&self.post, bind_post, "post")] {
                    let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some(label),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(pipe);
                    pass.set_bind_group(0, bind, &[]);
                    pass.dispatch_workgroups(groups.0, groups.1, 1);
                }
                Some(s.view)
            }
            _ => None,
        };

        // ---------------- вывод
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("grid"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            let draw = |pass: &mut wgpu::RenderPass, view: &View, bind: &wgpu::BindGroup| {
                let (x, y, w, h) = view.rect;
                let (x0, y0) = (x.min(tw), y.min(th));
                let (x1, y1) = ((x + w).min(tw), (y + h).min(th));
                if x1 <= x0 || y1 <= y0 {
                    return;
                }
                pass.set_pipeline(if view.overlay { &self.pipeline_overlay } else { &self.pipeline });
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.set_bind_group(0, bind, &[]);
                pass.draw(0..3, 0..1);
            };
            if let (Some(v), Some(gpu)) = (scene_view, &self.scene)
                && let Some((out, ..)) = &gpu.out
            {
                draw(&mut pass, &v, &out.bind_group);
            }
            for (k, l) in layers.iter().enumerate() {
                draw(&mut pass, &l.view, &self.layers[k].bind_group);
            }
        }
        self.queue.submit([enc.finish()]);
    }

    /// Текстуры сетки. `gpu_written` — пишет compute-шейдер (глиф R32Uint, storage).
    fn create_layer(&self, cols: u32, rows: u32, gpu_written: bool) -> LayerGpu {
        let (c, r) = (cols.max(1), rows.max(1));
        let extra = if gpu_written { wgpu::TextureUsages::STORAGE_BINDING } else { wgpu::TextureUsages::COPY_DST };
        let tex = |label, format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: c, height: r, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | extra,
                view_formats: &[],
            })
        };
        let glyph_format = if gpu_written { wgpu::TextureFormat::R32Uint } else { wgpu::TextureFormat::R16Uint };
        let glyph = tex("grid-glyph", glyph_format);
        let fg = tex("grid-fg", wgpu::TextureFormat::Rgba8Unorm);
        let bg = tex("grid-bg", wgpu::TextureFormat::Rgba8Unorm);
        let params = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid-params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let views = [&glyph, &fg, &bg].map(|t| t.create_view(&Default::default()));
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&views[0]) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[1]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&views[2]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&self.atlas_view) },
            ],
        });
        LayerGpu { cols, rows, glyph, fg, bg, params, bind_group }
    }
}

fn write_tex(queue: &wgpu::Queue, tex: &wgpu::Texture, data: &[u8], w: u32, h: u32, bpp: u32) {
    if w == 0 || h == 0 {
        return;
    }
    queue.write_texture(
        tex.as_image_copy(),
        data,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * bpp), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}
