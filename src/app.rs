//! Состояние приложения без привязки к окну: пресеты, переключатели, время,
//! сетки и раскладка слоёв. Используется и окном (main.rs), и режимом --screenshot.

use crate::config::{Config, Preset};
use crate::font::{Atlas, Font, FontId, GLYPH_COUNT, glyph_char};
use crate::grid::{Grid, rgb, solid};
use crate::renderer::{Layer, View};
use crate::scene::{self, City, Light};
use crate::style::Style;

/// Действия от клавиатуры (раскладку клавиш знает main.rs).
#[derive(Clone, Copy, Debug)]
pub enum Action {
    DensityUp,
    DensityDown,
    ToggleGap,
    ToggleBg,
    ToggleFontScreen,
    ToggleFastTime,
    TogglePause,
}

pub struct App {
    pub cfg: Config,
    fonts: [Font; 3],
    pub preset: usize,
    pub gap_on: bool,
    pub bg_on: bool,
    pub font_screen: bool,
    pub paused: bool,
    pub fast: bool,
    /// Время суток 0..1 (0 — полночь).
    pub day_t: f32,
    /// Время с запуска, секунды (для мигания огней и т.п.).
    pub time: f64,
    /// FPS для HUD; None — не показывать (скриншоты).
    pub fps: Option<f64>,
    size: (u32, u32),
    city: City,
    main: Grid,
    main_view: Option<View>,
    panels: Vec<(Grid, View)>,
}

impl App {
    pub fn new(cfg: Config) -> App {
        App {
            fonts: FontId::ALL.map(Font::load),
            preset: cfg.default_preset,
            day_t: cfg.start_time_of_day,
            cfg,
            gap_on: true,
            bg_on: true,
            font_screen: false,
            paused: false,
            fast: false,
            time: 0.0,
            fps: None,
            size: (1, 1),
            city: City::generate(),
            main: Grid::default(),
            main_view: None,
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
        match a {
            Action::DensityUp => self.preset = self.preset.saturating_sub(1),
            Action::DensityDown => self.preset = (self.preset + 1).min(n - 1),
            Action::ToggleGap => self.gap_on = !self.gap_on,
            Action::ToggleBg => self.bg_on = !self.bg_on,
            Action::ToggleFontScreen => self.font_screen = !self.font_screen,
            Action::ToggleFastTime => self.fast = !self.fast,
            Action::TogglePause => self.paused = !self.paused,
        }
    }

    /// Шаг обновления с фиксированным dt.
    pub fn update(&mut self, dt: f64) {
        self.time += dt;
        if !self.paused {
            let mul = if self.fast { self.cfg.fast_time_mul } else { 1.0 };
            self.day_t = (self.day_t + dt as f32 * mul / self.cfg.day_length_s).rem_euclid(1.0);
        }
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
        };
        (cols, rows, view)
    }

    /// Перерисовать сетки текущего кадра.
    pub fn draw(&mut self) {
        if self.font_screen {
            self.draw_font_screen();
        } else {
            self.draw_main();
        }
    }

    /// Слои для рендерера.
    pub fn layers(&self) -> Vec<Layer<'_>> {
        if self.font_screen {
            self.panels.iter().map(|(grid, view)| Layer { grid, view: *view }).collect()
        } else {
            self.main_view.iter().map(|v| Layer { grid: &self.main, view: *v }).collect()
        }
    }

    // ------------------------------------------------------------ основной экран
    fn draw_main(&mut self) {
        let p = self.preset();
        let (cols, rows, view) = self.view_for(&p, (0, 0, self.size.0, self.size.1));
        if self.main.cols != cols || self.main.rows != rows {
            self.main.resize(cols, rows);
        }
        self.main_view = Some(view);
        let mut grid = std::mem::take(&mut self.main);
        let font = self.font(p.font);
        let style = Style::new(&self.cfg, &font.ramp_glyphs);
        let view_w = cols as f32 * view.cell.0 as f32 / (rows.max(1) as f32 * view.cell.1 as f32);
        let light = Light::new(self.day_t, self.time, view_w);
        scene::draw(&mut grid, &style, &self.city, &light, view.cell, 0.3);
        self.draw_overlay(&mut grid, &p, &view, &light);
        self.main = grid;
    }

    fn draw_overlay(&self, g: &mut Grid, p: &Preset, v: &View, light: &Light) {
        let c = &self.cfg;
        let (fg, acc, dim, plate) = (rgb(c.hud_fg), rgb(c.hud_accent), rgb(c.hud_dim), solid(c.hud_plate));
        let font = self.font(p.font);
        let fps = self.fps.map_or("--".to_string(), |f| format!("{f:.0}"));
        let line1 = format!(
            " ASCII CITY  stage 1 | {fps} fps | {}x{} = {} cells | cell {}x{} px (glyph {}x{} x{} + gap {}) | preset {} | {}{}{} ",
            g.cols,
            g.rows,
            g.cols * g.rows,
            v.cell.0,
            v.cell.1,
            font.w,
            font.h,
            p.scale,
            if self.gap_on { c.gap_for(p) } else { 0 },
            p.name,
            light.clock(),
            if self.paused { " paused" } else { "" },
            if self.fast { " x10" } else { "" },
        );
        let line2 = " +/- density  G gap  B bg  F1 fonts  F11 fullscreen  T fast time  Space pause  Esc quit ";
        g.fill_rect(0, 0, line1.len() as i32, 1, plate);
        g.text(0, 0, &line1, fg);
        g.text(1, 0, "ASCII CITY", acc);
        g.fill_rect(0, 1, line2.len() as i32, 1, plate);
        g.text(0, 1, line2, dim);

        // внизу: полоса всех глифов, рампа шрифта, образцы текста
        let all: String = (0..GLYPH_COUNT as u16).map(glyph_char).collect();
        let lines = [
            (format!(" glyphs: {all} "), fg),
            (format!(" ramp {}: [{}] ", font.name, font.ramp), acc),
            (" The quick brown fox jumps over the lazy dog. 0123456789 ".to_string(), fg),
            (" THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG! (){}[]<>/\\|@#$%&*+=-_~^`'\";:,.? ".to_string(), dim),
        ];
        let y0 = g.rows as i32 - lines.len() as i32;
        for (k, (s, col)) in lines.iter().enumerate() {
            let y = y0 + k as i32;
            g.fill_rect(0, y, s.chars().count() as i32, 1, plate);
            g.text(0, y, s, *col);
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
        // весь набор глифов с переносом по ширине панели
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
        g.text(0, y, "      ", dim);
        g.text(6, y, f.ramp, fg);
        y += 2;
        // градиенты по правилу стиля: серый и три «городских» оттенка
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
            "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG!",
            "0123456789 +-*/=<> (){}[] @#$%&",
            "DINER  DELI  PHARMACY  LAUNDROMAT  BAR",
            "WALK  DON'T WALK  ONE WAY  BROADWAY",
            "5th Ave & W 42nd St   Grand Central",
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
