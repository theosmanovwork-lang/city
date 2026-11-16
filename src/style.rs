//! Правило стиля ячейки (аналог writeCell/rampG из эталона):
//! яркость → плотность символа (рампа шрифта), оттенок → цвет символа
//! (нормализованный), фон → слабая подсветка тем же цветом.

use crate::config::{Config, Rgb};
use crate::grid::{BLACK, Grid, Rgba8};

/// Параметры, нужные для перевода цвета в ячейку.
pub struct Style<'a> {
    pub ramp: &'a [u16],
    pub black: f32,
    pub gain: f32,
    pub fg_max_gain: f32,
    pub black_level: f32,
}

impl<'a> Style<'a> {
    pub fn new(cfg: &Config, ramp: &'a [u16]) -> Style<'a> {
        Style {
            ramp,
            black: cfg.ramp_black,
            gain: cfg.ramp_gain,
            fg_max_gain: cfg.fg_max_gain,
            black_level: cfg.black_level,
        }
    }

    /// Символ рампы для яркости `l` (0..1+). Квадратный корень даёт больше
    /// градаций в тенях, как в эталоне.
    #[inline]
    pub fn ramp_glyph(&self, l: f32) -> u16 {
        let t = (l - self.black) * self.gain;
        if t <= 0.0 {
            return self.ramp[0];
        }
        let n = self.ramp.len();
        let k = 1 + (t.sqrt() * (n - 1) as f32) as usize;
        self.ramp[k.min(n - 1)]
    }

    /// Цвет поверхности → (символ, цвет символа, фон).
    /// `glyph = None` — символ выбирается рампой по яркости.
    #[inline]
    pub fn cell(&self, c: Rgb, glyph: Option<u16>) -> (u16, Rgba8, Rgba8) {
        let [r, g, b] = c;
        let lum = 0.3 * r + 0.59 * g + 0.11 * b;
        let glyph = glyph.unwrap_or_else(|| self.ramp_glyph(lum));
        let m = r.max(g).max(b);
        if m <= self.black_level {
            return (glyph, BLACK, BLACK);
        }
        // Цвет символа нормализован по максимуму канала: яркость уже закодирована
        // плотностью, поэтому символ несёт чистый оттенок. Тёмные — усиливаются ограниченно.
        let gain = (1.0 / m).min(self.fg_max_gain);
        let q = |v: f32, k: f32| (v * k).clamp(0.0, 1.0) * 255.0 + 0.5;
        let fg = [q(r, gain) as u8, q(g, gain) as u8, q(b, gain) as u8, 255];
        // Фон: цвет поверхности как есть; шейдер умножит его на bgFill (альфа 0).
        let bg = [q(r, 1.0) as u8, q(g, 1.0) as u8, q(b, 1.0) as u8, 0];
        (glyph, fg, bg)
    }

    #[inline]
    pub fn put(&self, grid: &mut Grid, x: i32, y: i32, c: Rgb, glyph: Option<u16>) {
        let (g, fg, bg) = self.cell(c, glyph);
        grid.put(x, y, g, fg, bg);
    }
}
