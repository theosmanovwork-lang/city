//! Сетка символов: индекс глифа, цвет символа и цвет фона на каждую ячейку.
//!
//! Цвет фона: RGB — цвет поверхности, альфа — режим.
//!   `a = 0`   — подсветка: шейдер умножает фон на bgFill (клавиша B его выключает);
//!   `a = 255` — сплошной фон (плашки HUD): рисуется как есть.

use crate::font::glyph_index;

pub type Rgba8 = [u8; 4];

pub const BLACK: Rgba8 = [0, 0, 0, 0];

/// Цвет символа.
pub fn rgb(c: [f32; 3]) -> Rgba8 {
    let [r, g, b] = to_u8(c);
    [r, g, b, 255]
}

pub fn to_u8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

#[derive(Default)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    pub glyph: Vec<u16>,
    pub fg: Vec<Rgba8>,
    pub bg: Vec<Rgba8>,
}

impl Grid {
    pub fn new(cols: u32, rows: u32) -> Grid {
        let mut g = Grid::default();
        g.resize(cols, rows);
        g
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        self.cols = cols;
        self.rows = rows;
        let n = (cols * rows) as usize;
        self.glyph = vec![0; n];
        self.fg = vec![BLACK; n];
        self.bg = vec![BLACK; n];
    }

    #[inline]
    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.cols as i32 || y >= self.rows as i32 {
            return None;
        }
        Some((y as u32 * self.cols + x as u32) as usize)
    }

    /// Записать ячейку. Координаты вне сетки игнорируются.
    #[inline]
    pub fn put(&mut self, x: i32, y: i32, glyph: u16, fg: Rgba8, bg: Rgba8) {
        if let Some(i) = self.index(x, y) {
            self.glyph[i] = glyph;
            self.fg[i] = fg;
            self.bg[i] = bg;
        }
    }

    /// Строка текста: меняет символ и цвет символа, фон не трогает.
    pub fn text(&mut self, x: i32, y: i32, s: &str, fg: Rgba8) {
        for (k, c) in s.chars().enumerate() {
            if let Some(i) = self.index(x + k as i32, y) {
                self.glyph[i] = glyph_index(c);
                self.fg[i] = fg;
            }
        }
    }

    /// Залить прямоугольник фоном (символы стираются).
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, bg: Rgba8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, 0, BLACK, bg);
            }
        }
    }

    /// Байты для загрузки в текстуры.
    pub fn glyph_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.glyph)
    }
    pub fn fg_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.fg)
    }
    pub fn bg_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.bg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_text_clip() {
        let mut g = Grid::new(4, 2);
        g.put(-1, 0, 5, BLACK, BLACK);
        g.put(4, 0, 5, BLACK, BLACK);
        g.text(2, 1, "abc", [1, 2, 3, 255]);
        assert_eq!(g.glyph[6], glyph_index('a'));
        assert_eq!(g.glyph[7], glyph_index('b'));
        assert!(g.glyph[..4].iter().all(|&v| v == 0));
    }
}
