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

    // ---------------------------------------------------------------- время суток
    /// Скорость хода времени по клавише T, игровых часов в секунду.
    pub fast_time_hours_per_s: f32,
    /// Шаг времени клавишами [ и ], часов.
    pub time_step_hours: f32,
    /// Увеличение панелей на отладочном экране шрифтов (вторая строка панелей).
    pub debug_zoom: u32,

    // ---------------------------------------------------------------- камера и рендер сцены
    /// Вертикальный угол обзора, градусы.
    pub fov_y_deg: f32,
    /// Дальность прорисовки, м.
    pub max_dist: f32,
    /// Максимум шагов луча.
    pub max_steps: u32,
    /// Размер ячейки сетки ускорения, м.
    pub accel_cell: f32,
    /// Лучей на ячейку по стороне = round(высота ячейки / это значение), 1..=4.
    pub supersample_cell_px: f32,
    /// Свечение вокруг ярких источников (bloom) на уровне ячеек: сила и порог яркости.
    pub bloom: f32,
    pub bloom_threshold: f32,
    /// Радиус свечения: ячеек на каждые 100 колонок сетки.
    pub bloom_radius_per_100: f32,
    /// Косинус углового радиуса диска луны/солнца по умолчанию.
    pub moon_disc_cos: f32,

    // ---------------------------------------------------------------- игрок (реальный масштаб)
    /// Высота глаз над полом, м (рост ~1.75 м).
    pub eye_height: f32,
    /// Шаг, м/с.
    pub walk_speed: f32,
    /// Бег, м/с.
    pub run_speed: f32,
    /// Чувствительность мыши, рад на пиксель.
    pub mouse_sens: f32,
    /// Предел наклона головы, градусы.
    pub max_pitch_deg: f32,
    /// Радиус тела для коллизий, м.
    pub body_radius: f32,
    /// Максимальная высота ступени, м.
    pub step_height: f32,
    pub gravity: f32,
    pub collision_iters: u32,
    /// Покачивание камеры при ходьбе.
    pub head_bob: bool,
    /// Длина шага (полупериод покачивания), м.
    pub stride: f32,
    /// Амплитуда покачивания вверх-вниз и вбок, м.
    pub bob_height: f32,
    pub bob_side: f32,

    // ---------------------------------------------------------------- субтитры
    /// Субтитры — шрифт 5×8 с увеличением round(высота окна / это значение).
    pub subtitle_px_per_scale: f32,
    pub subtitle_fade_s: f32,
    /// Пауза между репликами, с.
    pub subtitle_gap_s: f32,
    /// Ширина строки субтитров, доля экрана.
    pub subtitle_width: f32,
    /// Отступ снизу, строк.
    pub subtitle_bottom_rows: u32,
    pub subtitle_fg: Rgb,
    pub subtitle_speaker: Rgb,
    /// Непрозрачность тёмной плашки под текстом.
    pub subtitle_plate_alpha: f32,
    /// Непрозрачность плашки отладочного HUD (F3).
    pub hud_plate_alpha: f32,

    // ---------------------------------------------------------------- HUD
    pub hud_fg: Rgb,
    pub hud_accent: Rgb,
    pub hud_dim: Rgb,

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

            fast_time_hours_per_s: 0.6,
            time_step_hours: 1.0,
            debug_zoom: 3,

            fov_y_deg: 58.0,
            max_dist: 900.0,
            max_steps: 220,
            accel_cell: 2.0,
            supersample_cell_px: 6.0,
            bloom: 0.9,
            bloom_threshold: 0.55,
            bloom_radius_per_100: 1.0,
            moon_disc_cos: 0.99985,

            eye_height: 1.65,
            walk_speed: 1.4,
            run_speed: 3.5,
            mouse_sens: 0.0022,
            max_pitch_deg: 80.0,
            body_radius: 0.3,
            step_height: 0.32,
            gravity: 9.81,
            collision_iters: 4,
            head_bob: true,
            stride: 0.75,
            bob_height: 0.03,
            bob_side: 0.018,

            subtitle_px_per_scale: 540.0,
            subtitle_fade_s: 0.35,
            subtitle_gap_s: 0.4,
            subtitle_width: 0.6,
            subtitle_bottom_rows: 3,
            subtitle_fg: [0.95, 0.94, 0.90],
            subtitle_speaker: [1.0, 0.78, 0.30],
            subtitle_plate_alpha: 0.45,
            hud_plate_alpha: 0.6,

            hud_fg: [0.92, 0.92, 0.88],
            hud_accent: [1.0, 0.78, 0.25],
            hud_dim: [0.55, 0.57, 0.6],

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
