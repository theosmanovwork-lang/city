//! ASCII City — этап 1: окно, цикл с фиксированным шагом, вывод сетки символов.
//!
//! Режим без окна: `ascii_city --screenshot out.png [--preset all|<имя>|<номер>]
//! [--size 1920x1080] [--time 0.75] [--fonts]`.

mod app;
mod config;
mod font;
mod grid;
mod renderer;
mod scene;
mod style;

use app::{Action, App};
use config::Config;
use renderer::Renderer;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Fullscreen, Window, WindowId};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,ascii_city=info")).init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(k) = args.iter().position(|a| a == "--screenshot") {
        let out = args.get(k + 1).cloned().unwrap_or_else(|| "screenshot.png".into());
        screenshot(&args, &out);
        return;
    }
    let event_loop = EventLoop::new().expect("не удалось создать цикл событий");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut handler = Handler::new(App::new(Config::default()));
    event_loop.run_app(&mut handler).expect("ошибка цикла событий");
}

// ============================================================ ОКНО
struct Handler {
    app: App,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    last: Instant,
    acc: f64,
    fps: f64,
}

impl Handler {
    fn new(app: App) -> Handler {
        Handler { app, window: None, renderer: None, last: Instant::now(), acc: 0.0, fps: 60.0 }
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        // фиксированный шаг обновления, отрисовка — каждый кадр
        let step = 1.0 / self.app.cfg.update_hz;
        self.acc += dt;
        let mut n = 0;
        while self.acc >= step && n < self.app.cfg.max_updates_per_frame {
            self.app.update(step);
            self.acc -= step;
            n += 1;
        }
        if n == self.app.cfg.max_updates_per_frame {
            self.acc = 0.0; // не догоняем бесконечно после долгой паузы
        }
        let k = self.app.cfg.fps_smoothing;
        self.fps = self.fps * k + (1.0 / dt.max(1e-4)) * (1.0 - k);
        self.app.fps = Some(self.fps);

        self.app.draw();
        if let Some(r) = &mut self.renderer {
            r.render(&self.app.layers());
        }
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, ev: &KeyEvent) {
        if ev.state != ElementState::Pressed || ev.repeat {
            return;
        }
        let PhysicalKey::Code(code) = ev.physical_key else { return };
        let action = match code {
            KeyCode::Equal | KeyCode::NumpadAdd => Some(Action::DensityUp),
            KeyCode::Minus | KeyCode::NumpadSubtract => Some(Action::DensityDown),
            KeyCode::KeyG => Some(Action::ToggleGap),
            KeyCode::KeyB => Some(Action::ToggleBg),
            KeyCode::F1 => Some(Action::ToggleFontScreen),
            KeyCode::KeyT => Some(Action::ToggleFastTime),
            KeyCode::Space => Some(Action::TogglePause),
            KeyCode::F11 => {
                if let Some(w) = &self.window {
                    let full = w.fullscreen().is_some();
                    w.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
                }
                None
            }
            KeyCode::Escape => {
                event_loop.exit();
                None
            }
            _ => None,
        };
        if let Some(a) = action {
            self.app.action(a);
        }
    }
}

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let cfg = &self.app.cfg;
        let attrs = Window::default_attributes()
            .with_title(cfg.window_title)
            .with_inner_size(winit::dpi::LogicalSize::new(cfg.window_size.0, cfg.window_size.1));
        let window = Arc::new(event_loop.create_window(attrs).expect("не удалось создать окно"));
        let atlas = self.app.atlas();
        let renderer = pollster::block_on(Renderer::for_window(window.clone(), cfg.vsync, &atlas));
        log::info!("{}", renderer.adapter_info);
        let size = window.inner_size(); // физические пиксели
        self.app.set_size(size.width, size.height);
        self.renderer = Some(renderer);
        self.window = Some(window);
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            // Resized приходит и после смены DPI: сетка всегда считается в физических пикселях
            WindowEvent::Resized(size) => {
                self.app.set_size(size.width, size.height);
                if let Some(r) = &mut self.renderer {
                    r.resize(size.width, size.height);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                log::info!("масштаб экрана: {scale_factor}");
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(event_loop, &event),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

// ============================================================ СКРИНШОТЫ
fn arg<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|k| args.get(k + 1)).map(|s| s.as_str())
}

/// Рендер без окна в PNG. `--preset all` — по файлу на каждый пресет (`out-3x5.png` и т.д.).
fn screenshot(args: &[String], out: &str) {
    let mut app = App::new(Config::default());
    let (w, h) = arg(args, "--size")
        .and_then(|s| s.split_once('x'))
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .unwrap_or((1920, 1080));
    if let Some(t) = arg(args, "--time").and_then(|s| s.parse().ok()) {
        app.day_t = t;
    }
    app.font_screen = args.iter().any(|a| a == "--fonts");
    app.gap_on = !args.iter().any(|a| a == "--no-gap");
    app.bg_on = !args.iter().any(|a| a == "--no-bg");
    app.set_size(w, h);
    let mut renderer = pollster::block_on(Renderer::headless(&app.atlas()));
    eprintln!("GPU: {}", renderer.adapter_info);

    let n = app.cfg.presets.len();
    let which: Vec<usize> = match arg(args, "--preset") {
        Some("all") => (0..n).collect(),
        Some(s) => vec![
            s.parse::<usize>()
                .ok()
                .filter(|&k| k < n)
                .or_else(|| app.cfg.presets.iter().position(|p| p.name.replace(' ', "") == s.replace(' ', "")))
                .unwrap_or_else(|| panic!("нет пресета {s:?}")),
        ],
        None => vec![app.preset],
    };
    for k in which {
        app.preset = k;
        app.draw(); // прогрев: первый кадр выделяет буферы
        let t0 = Instant::now();
        app.draw();
        let draw_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let px = renderer.render_offscreen(w, h, &app.layers());
        let path = if which_many(args) {
            let name = app.preset().name.replace(' ', "");
            match out.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem}-{name}.{ext}"),
                None => format!("{out}-{name}.png"),
            }
        } else {
            out.to_string()
        };
        write_png(&path, w, h, &px);
        eprintln!("{path}: {}x{} px, пресет {}, расчёт сетки на CPU {draw_ms:.1} мс", w, h, app.preset().name);
    }
}

fn which_many(args: &[String]) -> bool {
    arg(args, "--preset") == Some("all")
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png");
    wr.write_image_data(rgba).expect("png");
}
