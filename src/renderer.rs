//! wgpu-рендерер сетки символов.
//!
//! На каждый «слой» (обычно один — весь экран; на отладочном экране шрифтов — несколько
//! панелей) — три текстуры сетки (глиф R16Uint, fg и bg Rgba8Unorm) и uniform-буфер.
//! Атлас всех шрифтов (R8Unorm) общий и грузится один раз.
//! Каждый кадр на GPU уходят только буферы сетки и параметры.

use crate::font::{ATLAS_COLS, Atlas};
use crate::grid::Grid;
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
}

pub struct Layer<'a> {
    pub grid: &'a Grid,
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
    _pad: [u32; 2],
}

/// GPU-ресурсы одного слоя. Текстуры пересоздаются только при смене размера сетки.
struct LayerGpu {
    cols: u32,
    rows: u32,
    glyph: wgpu::Texture,
    fg: wgpu::Texture,
    bg: wgpu::Texture,
    params: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: Option<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration)>,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    atlas_view: wgpu::TextureView,
    atlas_origin_y: [u32; 3],
    layers: Vec<LayerGpu>,
    pub adapter_info: String,
}

/// Предпочитаем не-sRGB формат: значения сетки уже в «экранных» единицах,
/// как в WebGL-эталоне. Если есть только sRGB — шейдер переводит в линейные.
fn pick_format(formats: &[wgpu::TextureFormat]) -> wgpu::TextureFormat {
    formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(formats[0])
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
            present_mode: if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync },
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

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });
        let tex_entry = |binding, sample_type| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let unfilterable = wgpu::TextureSampleType::Float { filterable: false };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1, wgpu::TextureSampleType::Uint),
                tex_entry(2, unfilterable),
                tex_entry(3, unfilterable),
                tex_entry(4, unfilterable),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });

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
            layout,
            layers: Vec::new(),
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

    /// Нарисовать слои в окно.
    pub fn render(&mut self, layers: &[Layer]) {
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
        self.encode(&view, w, h, layers);
        self.queue.present(frame);
    }

    /// Нарисовать слои в offscreen-текстуру и вернуть RGBA8 построчно (для --screenshot).
    pub fn render_offscreen(&mut self, w: u32, h: u32, layers: &[Layer]) -> Vec<u8> {
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
        self.encode(&tex.create_view(&Default::default()), w, h, layers);

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

    fn encode(&mut self, target: &wgpu::TextureView, tw: u32, th: u32, layers: &[Layer]) {
        while self.layers.len() < layers.len() {
            let g = Grid::new(1, 1);
            self.layers.push(self.create_layer(&g));
        }
        for (k, l) in layers.iter().enumerate() {
            if self.layers[k].cols != l.grid.cols || self.layers[k].rows != l.grid.rows {
                self.layers[k] = self.create_layer(l.grid);
            }
            let gpu = &self.layers[k];
            let (c, r) = (l.grid.cols, l.grid.rows);
            write_tex(&self.queue, &gpu.glyph, l.grid.glyph_bytes(), c, r, 2);
            write_tex(&self.queue, &gpu.fg, l.grid.fg_bytes(), c, r, 4);
            write_tex(&self.queue, &gpu.bg, l.grid.bg_bytes(), c, r, 4);
            let v = &l.view;
            let p = Params {
                origin: [v.rect.0 as i32 + v.origin.0, v.rect.1 as i32 + v.origin.1],
                grid: [c, r],
                cell: [v.cell.0, v.cell.1],
                glyph: [v.glyph.0, v.glyph.1],
                scale: v.scale,
                atlas_cols: ATLAS_COLS,
                atlas_y: self.atlas_origin_y[v.font],
                gap_on: v.gap_on as u32,
                gap_color: [v.gap_color[0], v.gap_color[1], v.gap_color[2], 1.0],
                bg_gain: v.bg_gain,
                srgb_out: self.format.is_srgb() as u32,
                _pad: [0; 2],
            };
            self.queue.write_buffer(&gpu.params, 0, bytemuck::bytes_of(&p));
        }

        let mut enc = self.device.create_command_encoder(&Default::default());
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
            pass.set_pipeline(&self.pipeline);
            for (k, l) in layers.iter().enumerate() {
                let (x, y, w, h) = l.view.rect;
                let (x0, y0) = (x.min(tw), y.min(th));
                let (x1, y1) = ((x + w).min(tw), (y + h).min(th));
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                pass.set_scissor_rect(x0, y0, x1 - x0, y1 - y0);
                pass.set_bind_group(0, &self.layers[k].bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([enc.finish()]);
    }

    fn create_layer(&self, grid: &Grid) -> LayerGpu {
        let (cols, rows) = (grid.cols.max(1), grid.rows.max(1));
        let tex = |label, format| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: cols, height: rows, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let glyph = tex("grid-glyph", wgpu::TextureFormat::R16Uint);
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
        LayerGpu { cols: grid.cols, rows: grid.rows, glyph, fg, bg, params, bind_group }
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
