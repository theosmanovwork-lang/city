//! ASCII City — окно, цикл с фиксированным шагом, ввод.
//!
//! Режим без окна: `ascii_city --screenshot out.png [--preset all|<имя>|<номер>]
//! [--size 1920x1080] [--time 0.8] [--camera x,y,z,yaw,pitch] [--say "ИМЯ|текст"]
//! [--fonts] [--hud] [--no-gap] [--no-bg] [--scene alley|street] [--sim run:8,walk:2,idle:1,back:1]`.
//! Игра: `ascii_city [--scene alley|street]` (по умолчанию — локация 1 «Сон. Переулок»).

mod app;
mod build;
mod config;
mod daylight;
mod font;
mod grid;
mod logic;
mod player;
mod renderer;
mod scenes;
mod sdf;
mod style;
mod subtitles;

use app::{Action, App, Move};
use config::Config;
use renderer::Renderer;
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

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
    let scene = arg(&args, "--scene").unwrap_or(scenes::NAMES[0]);
    let mut handler = Handler::new(App::new(Config::default(), scene));
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
    grabbed: bool,
}

impl Handler {
    fn new(app: App) -> Handler {
        Handler { app, window: None, renderer: None, last: Instant::now(), acc: 0.0, fps: 60.0, grabbed: false }
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
        self.app.grab_hint = !self.grabbed;

        self.app.draw();
        if let Some(r) = &mut self.renderer {
            r.update_prims(&self.app.scene.prims);
            r.render(self.app.scene_pass(), &self.app.layers());
        }
    }

    fn set_grab(&mut self, on: bool) {
        let Some(w) = &self.window else { return };
        if on {
            let ok = w
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| w.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            w.set_cursor_visible(!ok);
            self.grabbed = ok;
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
            self.grabbed = false;
            self.app.release_all();
        }
    }

    fn key(&mut self, event_loop: &ActiveEventLoop, ev: &KeyEvent) {
        // служебные клавиши распознаём и по логическому значению: на части систем
        // (виртуальные X-серверы, удалённый доступ) у F-клавиш нет физического кода
        let named = match &ev.logical_key {
            Key::Named(NamedKey::F1) => Some(KeyCode::F1),
            Key::Named(NamedKey::F3) => Some(KeyCode::F3),
            Key::Named(NamedKey::F11) => Some(KeyCode::F11),
            Key::Named(NamedKey::Escape) => Some(KeyCode::Escape),
            _ => None,
        };
        let code = match (named, ev.physical_key) {
            (Some(c), _) => c,
            (None, PhysicalKey::Code(c)) => c,
            _ => return,
        };
        let down = ev.state == ElementState::Pressed;
        let mv = match code {
            KeyCode::KeyW | KeyCode::ArrowUp => Some(Move::Forward),
            KeyCode::KeyS | KeyCode::ArrowDown => Some(Move::Back),
            KeyCode::KeyA => Some(Move::Left),
            KeyCode::KeyD => Some(Move::Right),
            KeyCode::ShiftLeft | KeyCode::ShiftRight => Some(Move::Run),
            _ => None,
        };
        if let Some(m) = mv {
            self.app.hold(m, down);
            return;
        }
        if !down || ev.repeat {
            return;
        }
        let action = match code {
            KeyCode::Equal | KeyCode::NumpadAdd => Some(Action::DensityUp),
            KeyCode::Minus | KeyCode::NumpadSubtract => Some(Action::DensityDown),
            KeyCode::KeyG => Some(Action::ToggleGap),
            KeyCode::KeyB => Some(Action::ToggleBg),
            KeyCode::F1 => Some(Action::ToggleFontScreen),
            KeyCode::F3 => Some(Action::ToggleHud),
            KeyCode::KeyT => Some(Action::ToggleFastTime),
            KeyCode::BracketLeft => Some(Action::TimeBack),
            KeyCode::BracketRight => Some(Action::TimeForward),
            KeyCode::F11 => {
                if let Some(w) = &self.window {
                    let full = w.fullscreen().is_some();
                    w.set_fullscreen(if full { None } else { Some(Fullscreen::Borderless(None)) });
                }
                None
            }
            KeyCode::Escape => {
                if self.grabbed {
                    self.set_grab(false);
                } else {
                    event_loop.exit();
                }
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
        let mut renderer = pollster::block_on(Renderer::for_window(window.clone(), cfg.vsync, &atlas));
        renderer.load_scene(&self.app.scene, &self.app.accel);
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
            WindowEvent::Focused(false) => self.set_grab(false),
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. }
                if !self.grabbed =>
            {
                self.set_grab(true);
            }
            WindowEvent::KeyboardInput { event, .. } => self.key(event_loop, &event),
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event
            && self.grabbed
        {
            self.app.look(delta.0 as f32, delta.1 as f32);
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
    let mut app = App::new(Config::default(), arg(args, "--scene").unwrap_or(scenes::NAMES[0]));
    let (w, h) = arg(args, "--size")
        .and_then(|s| s.split_once('x'))
        .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)))
        .unwrap_or((1920, 1080));
    if let Some(t) = arg(args, "--time").and_then(|s| s.parse().ok()) {
        app.day_t = t;
    }
    if let Some(c) = arg(args, "--camera") {
        let v: Vec<f32> = c.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if v.len() >= 4 {
            app.player.pos = [v[0], v[1], v[2]];
            app.player.yaw = v[3].to_radians();
            app.player.pitch = v.get(4).copied().unwrap_or(0.0).to_radians();
        }
    }
    if let Some(s) = arg(args, "--say") {
        let (who, text) = s.split_once('|').unwrap_or(("", s));
        let line = subtitles::Line {
            speaker: if who.is_empty() { None } else { Some(Box::leak(who.to_string().into_boxed_str())) },
            text: Box::leak(text.to_string().into_boxed_str()),
            secs: 10.0,
        };
        app.say(&[line]);
        app.subtitles.update(0.0, 0.0);
        app.subtitles.update(1.0, 0.0);
    }
    // прогнать сценарий с заданным вводом: "run:8,walk:2,idle:1,back:1,turn:1"
    if let Some(sim) = arg(args, "--sim") {
        let step = 1.0 / app.cfg.update_hz;
        for part in sim.split(',') {
            let (what, secs) = part.split_once(':').unwrap_or((part, "1"));
            let secs: f64 = secs.parse().unwrap_or(1.0);
            app.release_all();
            match what {
                "run" => {
                    app.hold(Move::Forward, true);
                    app.hold(Move::Run, true);
                }
                "walk" => app.hold(Move::Forward, true),
                "back" => app.hold(Move::Back, true),
                "turn" => app.look(1400.0, 0.0),
                _ => {}
            }
            for _ in 0..(secs / step) as u32 {
                app.update(step);
            }
        }
        app.release_all();
        eprintln!("после --sim: попытка {}, игрок {:?}", app.attempt, app.player.pos);
    }
    app.font_screen = args.iter().any(|a| a == "--fonts");
    app.hud = args.iter().any(|a| a == "--hud");
    app.gap_on = !args.iter().any(|a| a == "--no-gap");
    app.bg_on = !args.iter().any(|a| a == "--no-bg");
    app.set_size(w, h);
    let mut renderer = pollster::block_on(Renderer::headless(&app.atlas()));
    renderer.load_scene(&app.scene, &app.accel);
    eprintln!("GPU: {}", renderer.adapter_info);

    let n = app.cfg.presets.len();
    let many = arg(args, "--preset") == Some("all");
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
        let t0 = Instant::now();
        app.draw();
        renderer.update_prims(&app.scene.prims);
        let px = renderer.render_offscreen(w, h, app.scene_pass(), &app.layers());
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let path = if many {
            let name = app.preset().name.replace(' ', "");
            match out.rsplit_once('.') {
                Some((stem, ext)) => format!("{stem}-{name}.{ext}"),
                None => format!("{out}-{name}.png"),
            }
        } else {
            out.to_string()
        };
        write_png(&path, w, h, &px);
        eprintln!("{path}: {w}x{h} px, пресет {}, кадр {ms:.0} мс", app.preset().name);
    }
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().expect("png");
    wr.write_image_data(rgba).expect("png");
}
