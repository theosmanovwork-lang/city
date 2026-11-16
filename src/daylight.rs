//! Время суток → освещение: солнце (ночью — луна), цвета неба, окружающий свет,
//! доля горящих окон, экспозиция. Считается на CPU один раз за кадр.

pub type Rgb = [f32; 3];

// Ключевые кадры неба по высоте солнца: (sin высоты, цвет в зените, цвет у горизонта)
const SKY_KEYS: [(f32, Rgb, Rgb); 5] = [
    (-0.35, [0.008, 0.010, 0.030], [0.05, 0.05, 0.08]), // ночь (с подсветкой города)
    (-0.12, [0.04, 0.05, 0.14], [0.26, 0.15, 0.22]),    // сумерки
    (0.00, [0.18, 0.24, 0.45], [0.95, 0.50, 0.25]),     // закат / рассвет
    (0.15, [0.30, 0.48, 0.80], [0.95, 0.76, 0.56]),     // золотой час
    (0.40, [0.24, 0.48, 0.90], [0.70, 0.82, 0.95]),     // день
];

/// Насколько высоко поднимается солнце (широта Нью-Йорка, весна/осень ≈ 50°).
const MAX_SUN_ELEV_DEG: f32 = 50.0;
/// Днём кадр «прикрывает диафрагму», чтобы небо не уходило целиком в самые плотные символы.
const DAY_EXPOSURE_CUT: f32 = 0.3;
/// Ночью глаз адаптируется к темноте: экспозиция выше.
const NIGHT_EXPOSURE: f32 = 2.2;
const MOON: Rgb = [0.55, 0.62, 0.8];
const MOON_K: f32 = 0.10;

pub struct Daylight {
    /// Направление НА источник (солнце или луна), нормализовано.
    pub sun_dir: [f32; 3],
    pub sun_col: Rgb,
    /// Сила прямого света 0..1.
    pub sun_k: f32,
    /// 0 днём → 1 ночью.
    pub night_k: f32,
    pub sky_top: Rgb,
    pub sky_hz: Rgb,
    pub ambient: Rgb,
    pub lit_frac: f32,
    pub exposure: f32,
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Daylight {
    /// `t` — время суток 0..1 (0 — полночь, 0.5 — полдень).
    /// `moon` — направление ночного светила, если сцена его задаёт.
    pub fn at(t: f32, moon: Option<[f32; 3]>) -> Daylight {
        use std::f32::consts::TAU;
        let ang = (t - 0.25) * TAU; // 0 — восход, π/2 — полдень
        let e = ang.sin() * MAX_SUN_ELEV_DEG.to_radians().sin();
        let mut k = 0;
        while k + 2 < SKY_KEYS.len() && e > SKY_KEYS[k + 1].0 {
            k += 1;
        }
        let (e0, t0, h0) = SKY_KEYS[k];
        let (e1, t1, h1) = SKY_KEYS[k + 1];
        let s = ((e - e0) / (e1 - e0)).clamp(0.0, 1.0);
        let sky_top = mix(t0, t1, s);
        let sky_hz = mix(h0, h1, s);
        let sun_k = smooth(-0.03, 0.2, e);
        let night_k = 1.0 - smooth(-0.2, 0.05, e);
        // солнце идёт с востока (+X) через юг (−Z) на запад (−X); улица вдоль Z
        let az = ang;
        let el = e.asin();
        let sun = [az.cos() * el.cos(), el.sin(), -az.sin().abs() * el.cos() - 0.25];
        let len = (sun[0] * sun[0] + sun[1] * sun[1] + sun[2] * sun[2]).sqrt();
        let sun = sun.map(|v| v / len);
        let sun_col = mix([1.0, 0.55, 0.3], [1.0, 0.96, 0.88], smooth(0.0, 0.3, e));
        // ночью основной направленный свет — луна, высоко с другой стороны
        let (dir, col, kk) = if sun_k > 0.02 {
            (sun, sun_col, sun_k)
        } else {
            let m = moon.unwrap_or([-0.35, 0.8, 0.45]);
            let l = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
            (m.map(|v| v / l), MOON, MOON_K * night_k)
        };
        let evening = 0.6 + 0.4 * ((t - 0.85) * TAU).cos();
        Daylight {
            sun_dir: dir,
            sun_col: col,
            sun_k: kk,
            night_k,
            sky_top,
            sky_hz,
            ambient: mix(sky_top, sky_hz, 0.5).map(|v| v * 0.9 + 0.004),
            lit_frac: 0.06 + 0.6 * night_k * evening + 0.1 * (1.0 - sun_k),
            exposure: (1.0 - DAY_EXPOSURE_CUT * sun_k) + (NIGHT_EXPOSURE - 1.0) * night_k,
        }
    }
}

/// Часы:минуты.
pub fn clock(t: f32) -> String {
    let m = (t.rem_euclid(1.0) * 24.0 * 60.0) as u32 % (24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}
