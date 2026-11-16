//! Состояние игры без привязки к окну: сцена, игрок, субтитры, время суток,
//! пресеты плотности, переключатели, слои для рендерера.
//! Используется и окном (main.rs), и режимом --screenshot.

use crate::config::{Config, Preset};
use crate::daylight::{Daylight, clock};
use crate::font::{Atlas, Font, FontId, GLYPH_COUNT, glyph_char, glyph_index};
use crate::grid::{Grid, Rgba8, rgb};
use crate::player::{Input, Player};
use crate::renderer::{FrameUniform, Layer, MAX_LIGHTS, ScenePass, View};
use crate::sdf::{Accel, Scene};
use crate::style::Style;
use crate::subtitles::{Line, Subtitles};

/// Действия от клавиатуры (раскладку клавиш знает main.rs).
#[derive(Clone, Copy, Debug)]
pub enum Action {
    DensityUp,
    DensityDown,
    ToggleGap,
    ToggleBg,
    ToggleFontScreen,
    ToggleHud,
    ToggleFastTime,
    TimeBack,
    TimeForward,
}

/// Удерживаемые клавиши движения.
#[derive(Clone, Copy, Debug)]
pub enum Move {
    Forward,
    Back,
    Left,
    Right,
    Run,
}

pub struct App {
    pub cfg: Config,
    fonts: [Font; 3],
    pub preset: usize,
    pub gap_on: bool,
    pub bg_on: bool,
    pub font_screen: bool,
    pub hud: bool,
    pub fast: bool,
    /// Время суток 0..1 (0 — полночь).
    pub day_t: f32,
    /// Время с запуска, секунды.
    pub time: f64,
    /// FPS для HUD; None — не показывать (скриншоты).
    pub fps: Option<f64>,
    /// Показать подсказку «щёлкните, чтобы управлять мышью».
    pub grab_hint: bool,
    pub scene: Scene,
    pub accel: Accel,
    pub player: Player,
    held: [bool; 5],
    pub subtitles: Subtitles,
    size: (u32, u32),
    // кадр
    frame: FrameUniform,
    scene_grid: (u32, u32),
    scene_view: Option<View>,
    overlay: Grid,
    overlay_view: Option<View>,
    panels: Vec<(Grid, View)>,
}

impl App {
    pub fn new(cfg: Config, scene: Scene) -> App {
        let accel = Accel::build(&scene, cfg.accel_cell);
        log::info!(
            "сцена «{}»: {} форм, сетка ускорения {}x{}, до {} форм в ячейке",
            scene.name,
            scene.prims.len(),
            accel.nx,
            accel.nz,
            accel.max_per_cell()
        );
        let player = Player::new(scene.spawn.0, scene.spawn.1);
        App {
            fonts: FontId::ALL.map(Font::load),
            preset: cfg.default_preset,
            day_t: scene.time_of_day,
            cfg,
            gap_on: true,
            bg_on: true,
            font_screen: false,
            hud: false,
            fast: false,
            time: 0.0,
            fps: None,
            grab_hint: false,
            scene,
            accel,
            player,
            held: [false; 5],
            subtitles: Subtitles::default(),
            size: (1, 1),
            frame: bytemuck::Zeroable::zeroed(),
            scene_grid: (0, 0),
            scene_view: None,
            overlay: Grid::default(),
            overlay_view: None,
            panels: Vec::new(),
        }
    }

    pub fn atlas(&self) -> Atlas {
        Atlas::build(&self.fonts)
    }

    pub fn preset(&self) -> Preset {
        self.cfg.presets[self.preset]
    }

    fn font(&self, id: FontId) -> &Font {
        &self.fonts[FontId::ALL.iter().position(|&f| f == id).unwrap_or(0)]
    }

    pub fn set_size(&mut self, w: u32, h: u32) {
        self.size = (w.max(1), h.max(1));
    }

    pub fn action(&mut self, a: Action) {
        let n = self.cfg.presets.len();
        let step = self.cfg.time_step_hours / 24.0;
        match a {
            Action::DensityUp => self.preset = self.preset.saturating_sub(1),
            Action::DensityDown => self.preset = (self.preset + 1).min(n - 1),
            Action::ToggleGap => self.gap_on = !self.gap_on,
            Action::ToggleBg => self.bg_on = !self.bg_on,
            Action::ToggleFontScreen => self.font_screen = !self.font_screen,
            Action::ToggleHud => self.hud = !self.hud,
            Action::ToggleFastTime => self.fast = !self.fast,
            Action::TimeBack => self.day_t = (self.day_t - step).rem_euclid(1.0),
            Action::TimeForward => self.day_t = (self.day_t + step).rem_euclid(1.0),
        }
    }

    pub fn hold(&mut self, m: Move, down: bool) {
        self.held[m as usize] = down;
    }

    pub fn release_all(&mut self) {
        self.held = [false; 5];
    }

    pub fn look(&mut self, dx: f32, dy: f32) {
        self.player.look(dx, dy, &self.cfg);
    }

    pub fn say(&mut self, lines: &[Line]) {
        self.subtitles.push(lines);
    }

    /// Шаг обновления с фиксированным dt.
    pub fn update(&mut self, dt: f64) {
        self.time += dt;
        let dtf = dt as f32;
        if self.fast {
            self.day_t = (self.day_t + dtf * self.cfg.fast_time_hours_per_s / 24.0).rem_euclid(1.0);
        }
        let h = &self.held;
        let input = Input {
            forward: h[Move::Forward as usize] as i32 as f32 - h[Move::Back as usize] as i32 as f32,
            strafe: h[Move::Right as usize] as i32 as f32 - h[Move::Left as usize] as i32 as f32,
            run: h[Move::Run as usize],
        };
        self.player.update(dtf, input, &self.scene, &self.cfg);
        let p = self.player.pos;
        let mut fired = Vec::new();
        for t in &mut self.scene.triggers {
            if !t.fired && t.contains(p) {
                t.fired = true;
                fired.extend(t.lines.iter().cloned());
            }
        }
        self.subtitles.push(&fired);
        self.subtitles.update(dtf, self.cfg.subtitle_gap_s);
    }

    /// Параметры вывода для пресета в прямоугольнике rect (сетка по центру).
    fn view_for(&self, p: &Preset, rect: (u32, u32, u32, u32)) -> (u32, u32, View) {
        let f = self.font(p.font);
        let cell = self.cfg.cell_size(p, (f.w, f.h));
        let cols = rect.2 / cell.0;
        let rows = rect.3 / cell.1;
        let origin = (((rect.2 - cols * cell.0) / 2) as i32, ((rect.3 - rows * cell.1) / 2) as i32);
        let view = View {
            rect,
            origin,
            font: FontId::ALL.iter().position(|&id| id == p.font).unwrap_or(0),
            glyph: (f.w, f.h),
            scale: p.scale,
            cell,
            gap_on: self.gap_on,
            gap_color: self.cfg.gap_color,
            bg_gain: if self.bg_on { self.cfg.bg_gain_for(cell.1) } else { 0.0 },
            overlay: false,
        };
        (cols, rows, view)
    }

    /// Подготовить кадр: параметры для GPU и сетки слоёв.
    pub fn draw(&mut self) {
        if self.font_screen {
            self.draw_font_screen();
        } else {
            self.prepare_scene();
            self.draw_overlay();
        }
    }

    pub fn scene_pass(&self) -> Option<ScenePass<'_>> {
        if self.font_screen {
            return None;
        }
        self.scene_view.map(|view| ScenePass {
            frame: &self.frame,
            cols: self.scene_grid.0,
            rows: self.scene_grid.1,
            view,
        })
    }

    /// Слои с CPU для рендерера.
    pub fn layers(&self) -> Vec<Layer<'_>> {
        if self.font_screen {
            self.panels.iter().map(|(grid, view)| Layer { grid, view: *view }).collect()
        } else {
            self.overlay_view.iter().map(|v| Layer { grid: &self.overlay, view: *v }).collect()
        }
    }

    // ------------------------------------------------------------ сцена
    fn prepare_scene(&mut self) {
        let p = self.preset();
        let (cols, rows, view) = self.view_for(&p, (0, 0, self.size.0, self.size.1));
        self.scene_grid = (cols, rows);
        self.scene_view = Some(view);
        let c = &self.cfg;
        let d = Daylight::at(self.day_t);
        let eye = self.player.eye(c);
        let fwd = self.player.forward();
        let (s, co) = self.player.yaw.sin_cos();
        let right = [co, 0.0, -s];
        // up = fwd × right (экран: x вправо, y вверх)
        let up = [
            fwd[1] * right[2] - fwd[2] * right[1],
            fwd[2] * right[0] - fwd[0] * right[2],
            fwd[0] * right[1] - fwd[1] * right[0],
        ];
        let tan_y = (c.fov_y_deg.to_radians() * 0.5).tan();
        let aspect = (cols * view.cell.0) as f32 / (rows.max(1) * view.cell.1) as f32;
        let tan_x = tan_y * aspect;
        let ss = ((view.cell.1 as f32 / c.supersample_cell_px).round() as u32).clamp(1, 4);
        let font = self.font(p.font);
        let mut ramp = [[0u32; 4]; 8];
        for (k, &g) in font.ramp_glyphs.iter().take(32).enumerate() {
            ramp[k / 4][k % 4] = g as u32;
        }
        // ближайшие к игроку источники света
        let mut lights: Vec<_> = self.scene.lights.iter().collect();
        let dist2 = |l: &&crate::sdf::PointLight| (0..3).map(|i| (l.pos[i] - eye[i]).powi(2)).sum::<f32>();
        lights.sort_by(|a, b| dist2(a).total_cmp(&dist2(b)));
        lights.truncate(MAX_LIGHTS);
        let mut lu = [[0.0f32; 4]; 32];
        // ночью фонари ярче; днём почти выключены
        let lamp_k = 0.15 + 0.85 * d.night_k;
        for (k, l) in lights.iter().enumerate() {
            lu[k * 2] = [l.pos[0], l.pos[1], l.pos[2], l.radius];
            lu[k * 2 + 1] = [l.color[0] * lamp_k, l.color[1] * lamp_k, l.color[2] * lamp_k, l.power];
        }
        let a = &self.accel;
        let fog = self.scene.fog;
        self.frame = FrameUniform {
            cam_pos: [eye[0], eye[1], eye[2], self.time as f32],
            cam_fwd: [fwd[0], fwd[1], fwd[2], tan_x],
            cam_right: [right[0], right[1], right[2], tan_y],
            cam_up: [up[0], up[1], up[2], c.max_dist],
            grid: [cols, rows, ss, lights.len() as u32],
            sun_dir: [d.sun_dir[0], d.sun_dir[1], d.sun_dir[2], d.sun_k],
            sun_col: [d.sun_col[0], d.sun_col[1], d.sun_col[2], d.night_k],
            sky_top: [d.sky_top[0], d.sky_top[1], d.sky_top[2], d.exposure],
            sky_hz: [d.sky_hz[0], d.sky_hz[1], d.sky_hz[2], fog],
            ambient: [d.ambient[0], d.ambient[1], d.ambient[2], d.lit_frac],
            style: [c.ramp_black, c.ramp_gain, c.fg_max_gain, c.black_level],
            ramp_info: [
                font.ramp_glyphs.len().min(32) as u32,
                glyph_index('.') as u32,
                glyph_index('*') as u32,
                c.max_steps,
            ],
            ramp,
            accel_o: [a.origin[0], a.origin[1], a.cell, a.top],
            accel_n: [a.nx, a.nz, a.far.0, a.far.1],
            lights: lu,
        };
    }

    // ------------------------------------------------------------ оверлей: субтитры и HUD
    fn draw_overlay(&mut self) {
        let scale = ((self.size.1 as f32 / self.cfg.subtitle_px_per_scale).round() as u32).max(1);
        let p = Preset { name: "ui", font: FontId::Normal, scale };
        let (cols, rows, mut view) = self.view_for(&p, (0, 0, self.size.0, self.size.1));
        view.overlay = true;
        let mut g = std::mem::take(&mut self.overlay);
        if g.cols != cols || g.rows != rows {
            g.resize(cols, rows);
        } else {
            g.glyph.fill(0);
            g.fg.fill([0; 4]);
            g.bg.fill([0; 4]);
        }
        self.subtitles.draw(&mut g, &self.cfg);
        if self.grab_hint {
            let s = "Щёлкните мышью, чтобы управлять";
            let x = (cols as i32 - s.chars().count() as i32) / 2;
            g.text(x, 1, s, rgb(self.cfg.hud_dim));
        }
        if self.hud {
            self.draw_hud(&mut g);
        }
        self.overlay = g;
        self.overlay_view = Some(view);
    }

    fn draw_hud(&self, g: &mut Grid) {
        let c = &self.cfg;
        let p = self.preset();
        let f = self.font(p.font);
        let (cols, rows) = self.scene_grid;
        let cell = self.scene_view.map_or((0, 0), |v| v.cell);
        let fps = self.fps.map_or("--".to_string(), |f| format!("{f:.0}"));
        let pos = self.player.pos;
        let lines = [
            format!(" ASCII CITY | этап 2 | {fps} к/с "),
            format!(
                " сетка {cols}x{rows} = {} | клетка {}x{} px (глиф {}x{} x{} + зазор {}) | пресет {} ",
                cols * rows,
                cell.0,
                cell.1,
                f.w,
                f.h,
                p.scale,
                if self.gap_on { c.gap_for(&p) } else { 0 },
                p.name
            ),
            format!(
                " {} | {}{} | x {:.1} y {:.2} z {:.1} ",
                self.scene.name,
                clock(self.day_t),
                if self.fast { " >>" } else { "" },
                pos[0],
                pos[1],
                pos[2]
            ),
            " WASD, мышь, Shift - бег | +/- плотность | G зазор | B фон | [ ] время | T ход времени ".to_string(),
            " F1 шрифты | F3 скрыть | F11 экран | Esc мышь/выход ".to_string(),
        ];
        let plate: Rgba8 = [0, 0, 0, (c.hud_plate_alpha * 255.0) as u8];
        for (k, s) in lines.iter().enumerate() {
            let y = k as i32;
            g.fill_rect(0, y, s.chars().count() as i32, 1, plate);
            g.text(
                0,
                y,
                s,
                rgb(if k == 0 {
                    c.hud_accent
                } else if k >= 3 {
                    c.hud_dim
                } else {
                    c.hud_fg
                }),
            );
        }
    }

    // ------------------------------------------------------------ отладочный экран шрифтов
    /// Три шрифта рядом: верхний ряд — в натуральную величину, нижний — увеличенный.
    fn draw_font_screen(&mut self) {
        let (w, h) = self.size;
        let zoom = self.cfg.debug_zoom.max(1);
        let pw = w / 3;
        let top_h = h * 3 / 10;
        let mut panels = Vec::new();
        for (row, scale, y, ph) in [(0, 1, 0, top_h), (1, zoom, top_h, h - top_h)] {
            for (k, id) in FontId::ALL.into_iter().enumerate() {
                let p = Preset { name: "", font: id, scale };
                let (cols, rows, view) = self.view_for(&p, (k as u32 * pw, y, pw, ph));
                let mut grid = Grid::new(cols, rows);
                self.draw_font_panel(&mut grid, id, scale, view.cell, row == 1);
                panels.push((grid, view));
            }
        }
        self.panels = panels;
    }

    fn draw_font_panel(&self, g: &mut Grid, id: FontId, scale: u32, cell: (u32, u32), zoomed: bool) {
        let c = &self.cfg;
        let f = self.font(id);
        let style = Style::new(c, &f.ramp_glyphs);
        let (fg, acc, dim) = (rgb(c.hud_fg), rgb(c.hud_accent), rgb(c.hud_dim));
        let cols = g.cols as i32;
        let mut y = 0;
        let head = format!("FONT {} x{}  cell {}x{}  ramp {} levels", f.name, scale, cell.0, cell.1, f.ramp.len());
        g.text(0, y, &head, acc);
        y += 2;
        let all: Vec<char> = (0..GLYPH_COUNT as u16).map(glyph_char).collect();
        for chunk in all.chunks(cols.max(1) as usize) {
            g.text(0, y, &chunk.iter().collect::<String>(), fg);
            y += 1;
        }
        y += 1;
        g.text(0, y, "ramp:", dim);
        let spaced: String = f.ramp.chars().flat_map(|ch| [ch, ' ']).collect();
        g.text(6, y, &spaced, fg);
        y += 1;
        g.text(6, y, f.ramp, fg);
        y += 2;
        let ramps: [(&str, [f32; 3]); 4] = [
            ("grey", [1.0, 1.0, 1.0]),
            ("warm", [1.0, 0.78, 0.45]),
            ("brick", [0.9, 0.42, 0.3]),
            ("sky", [0.45, 0.65, 1.0]),
        ];
        for (name, tint) in ramps {
            g.text(0, y, name, dim);
            let x0 = 6;
            let n = (cols - x0).max(1);
            for x in 0..n {
                let l = x as f32 / (n - 1).max(1) as f32;
                style.put(g, x0 + x, y, [tint[0] * l, tint[1] * l, tint[2] * l], None);
            }
            y += 1;
        }
        y += 1;
        let samples = [
            "The quick brown fox jumps over the lazy dog.",
            "0123456789 +-*/=<> (){}[] @#$%&",
            "DINER  DELI  PHARMACY  WALK  DON'T WALK",
            "Съешь же ещё этих мягких французских булок,",
            "да выпей чаю. ЖЁЛТЫЙ ЩИТ, ЪЫЭЮЯ, шщъыьэюя.",
            "МАРК: Сказал же - в дайнере на углу.",
        ];
        for s in samples {
            g.text(0, y, s, fg);
            y += 1;
        }
        if zoomed {
            g.text(0, g.rows as i32 - 1, "(zoomed)", dim);
        }
    }
}
