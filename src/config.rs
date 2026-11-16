//! Все настройки в одном месте. Никаких «магических чисел» по коду.

use crate::font::FontId;

/// Пресет плотности: шрифт и целочисленное увеличение.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub name: &'static str,
    pub font: FontId,
    pub scale: u32,
}

/// Цвет 0..1 (sRGB-значения, как их видит монитор).
pub type Rgb = [f32; 3];

pub struct Config {
    // ---------------------------------------------------------------- сетка
    pub presets: Vec<Preset>,
    pub default_preset: usize,
    /// Зазор между ячейками в физических пикселях (при scale = 1).
    pub gap_px: u32,
    /// Зазор растёт вместе с глифом: ×2 → 2 px, ×3 → 3 px.
    pub gap_scales_with_glyph: bool,
    /// Цвет зазора.
    pub gap_color: Rgb,
    /// Подсветка фона ячейки цветом поверхности (как bgFill в эталоне).
    pub bg_fill: f32,
    /// Подсветка фона плавно уходит в 0 на мелких ячейках:
    /// множитель 0 при высоте ячейки `bg_fade_cell_h.0` px и 1 при `bg_fade_cell_h.1` px.
    pub bg_fade_cell_h: (f32, f32),

    // ---------------------------------------------------------------- правило стиля
    /// Яркость ниже порога → пустая ячейка (рампа начинается с пробела).
    pub ramp_black: f32,
    /// Растяжка яркости перед выбором символа рампы.
    pub ramp_gain: f32,
    /// Тёмные цвета символа усиливаются не больше чем в столько раз
    /// (иначе тёмная поверхность стала бы яркими символами).
    pub fg_max_gain: f32,
    /// Ниже этой яркости ячейка считается чёрной.
    pub black_level: f32,

    // ---------------------------------------------------------------- цикл
    /// Частота фиксированного шага обновления, Гц.
    pub update_hz: f64,
    /// Не больше стольких шагов обновления за кадр (защита от «спирали смерти»).
    pub max_updates_per_frame: u32,
    /// Вертикальная синхронизация. false — видно реальный FPS.
    pub vsync: bool,
    /// Сглаживание счётчика FPS (0..1, больше — плавнее).
    pub fps_smoothing: f64,

    // ---------------------------------------------------------------- тестовая сцена
    /// Длина полного цикла суток в тестовой сцене, секунды.
    pub day_length_s: f32,
    /// Ускорение времени по клавише T.
    pub fast_time_mul: f32,
    /// Время суток при старте (0 — полночь, 0.5 — полдень).
    pub start_time_of_day: f32,
    /// Увеличение панелей на отладочном экране шрифтов (вторая строка панелей).
    pub debug_zoom: u32,

    // ---------------------------------------------------------------- HUD
    pub hud_fg: Rgb,
    pub hud_accent: Rgb,
    pub hud_dim: Rgb,
    pub hud_plate: Rgb,

    // ---------------------------------------------------------------- окно
    pub window_title: &'static str,
    pub window_size: (u32, u32),
}

impl Default for Config {
    fn default() -> Self {
        Config {
            presets: vec![
                Preset { name: "3x5", font: FontId::Tiny, scale: 1 },
                Preset { name: "4x6", font: FontId::Small, scale: 1 },
                Preset { name: "5x8", font: FontId::Normal, scale: 1 },
                Preset { name: "5x8 x2", font: FontId::Normal, scale: 2 },
                Preset { name: "5x8 x3", font: FontId::Normal, scale: 3 },
            ],
            default_preset: 0,
            gap_px: 1,
            gap_scales_with_glyph: true,
            gap_color: [0.0, 0.0, 0.0],
            bg_fill: 0.17,
            bg_fade_cell_h: (5.0, 9.0),

            ramp_black: 0.022,
            ramp_gain: 1.1,
            fg_max_gain: 4.5,
            black_level: 0.003,

            update_hz: 60.0,
            max_updates_per_frame: 5,
            vsync: true,
            fps_smoothing: 0.95,

            day_length_s: 60.0,
            fast_time_mul: 10.0,
            start_time_of_day: 0.70,
            debug_zoom: 3,

            hud_fg: [0.92, 0.92, 0.88],
            hud_accent: [1.0, 0.78, 0.25],
            hud_dim: [0.55, 0.57, 0.6],
            hud_plate: [0.03, 0.035, 0.045],

            window_title: "ASCII City",
            window_size: (1600, 900),
        }
    }
}

impl Config {
    /// Размер ячейки (с зазором) в физических пикселях для пресета.
    pub fn cell_size(&self, p: &Preset, glyph: (u32, u32)) -> (u32, u32) {
        let gap = self.gap_for(p);
        (glyph.0 * p.scale + gap, glyph.1 * p.scale + gap)
    }

    pub fn gap_for(&self, p: &Preset) -> u32 {
        if self.gap_scales_with_glyph { self.gap_px * p.scale } else { self.gap_px }
    }

    /// Подсветка фона для ячейки данной высоты (с плавным уходом в 0 на мелких).
    pub fn bg_gain_for(&self, cell_h: u32) -> f32 {
        let (a, b) = self.bg_fade_cell_h;
        let t = ((cell_h as f32 - a) / (b - a)).clamp(0.0, 1.0);
        self.bg_fill * t
    }
}
