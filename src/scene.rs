//! Тестовая сцена этапа 1: небо с циклом суток и силуэт небоскрёбов,
//! переведённые в символы по правилу стиля. Всё в «мировых» координатах экрана:
//! по вертикали 0 — верх, 1 — низ; по горизонтали 1 = высота экрана.
//! Поэтому на мелкой сетке видно больше деталей (окна, рамы, огни),
//! а на крупной они усредняются в цвет — как и задумано стилем.

use crate::config::Rgb;
use crate::font::glyph_index;
use crate::grid::Grid;
use crate::style::Style;

// ---------------------------------------------------------------- параметры сцены
const HORIZON: f32 = 0.80; // линия земли, доля высоты экрана
const WORLD_W: f32 = 4.0; // ширина сгенерированного города в единицах высоты экрана
const FLOOR_H: f32 = 0.0085; // высота этажа
const WIN_SP: f32 = 0.0065; // шаг окон по горизонтали
const LAMP_SP: f32 = 0.23; // шаг фонарей
const LAMP_H: f32 = 0.055;
const STAR_DENSITY: f32 = 0.004;
const SEED: u32 = 1931;
const DAY_EXPOSURE_CUT: f32 = 0.45; // насколько днём «прикрывается диафрагма»

// ---------------------------------------------------------------- палитра (сдержанная)
const LIMESTONE: Rgb = [0.62, 0.58, 0.50];
const CONCRETE: Rgb = [0.50, 0.50, 0.49];
const BRICK: Rgb = [0.46, 0.22, 0.15];
const GLASS: Rgb = [0.16, 0.24, 0.32];
const STEEL: Rgb = [0.28, 0.30, 0.33];
const ASPHALT: Rgb = [0.16, 0.16, 0.17];
const SIDEWALK: Rgb = [0.36, 0.35, 0.33];
const WARM_WIN: Rgb = [1.0, 0.78, 0.45];
const COOL_WIN: Rgb = [0.72, 0.84, 1.0];
const LAMP: Rgb = [1.0, 0.72, 0.38];
const AVIATION_RED: Rgb = [2.2, 0.12, 0.06];

// Ключевые кадры неба по высоте солнца: (высота, цвет в зените, цвет у горизонта)
const SKY_KEYS: [(f32, Rgb, Rgb); 5] = [
    (-0.35, [0.006, 0.008, 0.025], [0.03, 0.04, 0.08]), // ночь
    (-0.12, [0.04, 0.05, 0.14], [0.26, 0.15, 0.22]),    // сумерки
    (0.00, [0.18, 0.24, 0.45], [0.95, 0.50, 0.25]),     // закат / рассвет
    (0.15, [0.30, 0.48, 0.80], [0.95, 0.76, 0.56]),     // золотой час
    (0.40, [0.24, 0.48, 0.90], [0.70, 0.82, 0.95]),     // день
];

pub fn hash3(a: i32, b: i32, c: i32) -> f32 {
    let mut h = (a.wrapping_mul(374_761_393)) ^ (b.wrapping_mul(668_265_263)) ^ (c.wrapping_mul(1_274_126_177));
    h = (h ^ ((h as u32) >> 13) as i32).wrapping_mul(1_274_126_177);
    h ^= ((h as u32) >> 16) as i32;
    (h as u32) as f32 / 4_294_967_296.0
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}
fn mul(a: Rgb, b: Rgb) -> Rgb {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2]]
}
fn scale(a: Rgb, k: f32) -> Rgb {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn add(a: Rgb, b: Rgb) -> Rgb {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn frac(v: f32) -> f32 {
    v - v.floor()
}

// ---------------------------------------------------------------- освещение по времени суток
/// Всё, что зависит от времени суток, считается один раз за кадр.
pub struct Light {
    pub t: f32,
    sun_elev: f32,
    sun_x: f32,
    sky_top: Rgb,
    sky_hz: Rgb,
    sun_col: Rgb,
    sun_k: f32,
    night_k: f32,
    ambient: Rgb,
    lit_frac: f32,
    blink_on: bool,
    /// Экспозиция: днём кадр темнее, чтобы яркое небо не уходило целиком в самые плотные символы.
    exposure: f32,
}

impl Light {
    pub fn new(t: f32, time: f64, view_w: f32) -> Light {
        let ang = (t - 0.25) * std::f32::consts::TAU;
        let e = ang.sin();
        let mut k = 0;
        while k + 2 < SKY_KEYS.len() && e > SKY_KEYS[k + 1].0 {
            k += 1;
        }
        let (e0, t0, h0) = SKY_KEYS[k];
        let (e1, t1, h1) = SKY_KEYS[k + 1];
        let s = ((e - e0) / (e1 - e0)).clamp(0.0, 1.0);
        let sky_top = mix(t0, t1, s);
        let sky_hz = mix(h0, h1, s);
        let sun_k = smooth(-0.05, 0.25, e);
        let night_k = 1.0 - smooth(-0.2, 0.05, e);
        let evening = 0.6 + 0.4 * ((t - 0.85) * std::f32::consts::TAU).cos();
        Light {
            t,
            sun_elev: e,
            // солнце идёт слева направо: восход у левого края, закат у правого
            sun_x: view_w * (0.1 + 0.8 * ((t - 0.25) / 0.5).clamp(0.0, 1.0)),
            sky_top,
            sky_hz,
            sun_col: mix([1.0, 0.55, 0.3], [1.0, 0.97, 0.9], smooth(0.0, 0.3, e)),
            sun_k,
            night_k,
            ambient: scale(add(sky_top, sky_hz), 0.35),
            lit_frac: 0.06 + 0.55 * night_k * evening + 0.08 * (1.0 - sun_k),
            blink_on: (time * 0.8).fract() < 0.35,
            exposure: 1.0 - DAY_EXPOSURE_CUT * sun_k,
        }
    }

    /// Часы:минуты для HUD.
    pub fn clock(&self) -> String {
        let m = (self.t * 24.0 * 60.0) as u32 % (24 * 60);
        format!("{:02}:{:02}", m / 60, m % 60)
    }

    /// Небо в точке: `up` — высота над горизонтом (0..HORIZON), x — по горизонтали.
    fn sky(&self, x: f32, up: f32) -> Rgb {
        self.sky_sun(self.sky_gradient(up), x, up)
    }

    /// Градиент неба по высоте (без солнца) — зависит только от строки.
    fn sky_gradient(&self, up: f32) -> Rgb {
        let s = (up / HORIZON).clamp(0.0, 1.0).powf(0.6);
        mix(self.sky_hz, self.sky_top, s)
    }

    /// Добавить к градиенту солнце и ореол.
    fn sky_sun(&self, mut c: Rgb, x: f32, up: f32) -> Rgb {
        // солнце и ореол
        let sy = HORIZON * (0.08 + 0.85 * self.sun_elev.max(-0.2));
        let (dx, dy) = (x - self.sun_x, up - sy);
        let d = (dx * dx + dy * dy).sqrt();
        if self.sun_elev > -0.1 {
            let vis = smooth(-0.1, 0.02, self.sun_elev);
            c = add(c, scale(self.sun_col, vis * 0.3 * (-d * 30.0).exp()));
            if d < 0.028 {
                c = add(c, scale(self.sun_col, 1.6 * vis));
            }
        }
        c
    }
}

// ---------------------------------------------------------------- город
#[derive(Clone, Copy, PartialEq)]
enum Material {
    Limestone, // ар-деко, известняк
    Concrete,  // офисная башня
    Glass,     // стеклянная башня
    Brick,     // кирпичный дом 4–6 этажей
}

struct Tier {
    inset: f32,
    top: f32,
}

struct Building {
    x0: f32,
    x1: f32,
    tiers: Vec<Tier>, // от широкого нижнего к узкому верхнему (уступы)
    spire: f32,       // высота шпиля над верхним ярусом (0 — нет)
    mat: Material,
    seed: i32,
}

impl Building {
    /// Высота силуэта в точке x (0 — здания тут нет).
    fn height_at(&self, x: f32) -> f32 {
        let mut h = 0.0;
        for t in &self.tiers {
            if x >= self.x0 + t.inset && x < self.x1 - t.inset {
                h = t.top;
            }
        }
        if self.spire > 0.0 && (x - (self.x0 + self.x1) * 0.5).abs() < 0.0018 {
            h = self.top() + self.spire;
        }
        h
    }
    fn top(&self) -> f32 {
        self.tiers.last().map_or(0.0, |t| t.top)
    }
}

/// Три слоя застройки: дальний (силуэт даунтауна), средний и ближний.
pub struct City {
    layers: [Vec<Building>; 3],
}

impl City {
    pub fn generate() -> City {
        let mut seed = 0;
        let mut rnd = move || {
            seed += 1;
            hash3(seed, SEED as i32, 77)
        };
        // (мин/макс высота, мин/макс ширина, доля кирпичных домов)
        let specs = [(0.18, 0.62, 0.05, 0.12, 0.0), (0.10, 0.45, 0.06, 0.13, 0.25), (0.05, 0.28, 0.07, 0.16, 0.6)];
        let layers = specs.map(|(hmin, hmax, wmin, wmax, brick_p)| {
            let mut v = Vec::new();
            let mut x = -0.1;
            while x < WORLD_W {
                let w = wmin + (wmax - wmin) * rnd();
                let gap = 0.004 + 0.02 * rnd();
                let tall = rnd().powf(1.6);
                let mut h = hmin + (hmax - hmin) * tall;
                let mat = if rnd() < brick_p && h < 0.2 {
                    h = h.min(0.06 + 0.04 * rnd()); // 4–6 этажей
                    Material::Brick
                } else if rnd() < 0.35 {
                    Material::Glass
                } else if rnd() < 0.5 {
                    Material::Limestone
                } else {
                    Material::Concrete
                };
                let mut tiers = vec![Tier { inset: 0.0, top: h }];
                // небоскрёб = подиум + башня (+ уступы)
                if h > 0.2 && mat != Material::Glass {
                    let podium = 0.05 + 0.05 * rnd();
                    tiers = vec![Tier { inset: 0.0, top: podium }];
                    let n = 1 + (rnd() * 3.0) as usize;
                    for k in 0..n {
                        let inset = w * (0.08 + 0.1 * k as f32 + 0.04 * rnd());
                        let top = podium + (h - podium) * (k + 1) as f32 / n as f32;
                        tiers.push(Tier { inset: inset.min(w * 0.4), top });
                    }
                }
                let spire = if h > 0.3 && rnd() < 0.45 { 0.03 + 0.08 * rnd() } else { 0.0 };
                v.push(Building { x0: x, x1: x + w, tiers, spire, mat, seed: (rnd() * 1e6) as i32 });
                x += w + gap;
            }
            v
        });
        City { layers }
    }

    fn at(&self, layer: usize, x: f32) -> Option<&Building> {
        let v = &self.layers[layer];
        let k = v.partition_point(|b| b.x1 <= x);
        v.get(k).filter(|b| x >= b.x0)
    }
}

/// Цвет здания в точке. `lx` — x от левого края, `up` — высота над землёй.
fn shade_building(b: &Building, x: f32, up: f32, l: &Light, haze: f32) -> Rgb {
    let w = b.x1 - b.x0;
    let lx = x - b.x0;
    let top = b.top();
    // шпиль и авиационный огонь
    if up > top + 0.0005 && b.spire > 0.0 {
        let tip = top + b.spire;
        if up > tip - 0.004 {
            return scale(AVIATION_RED, if l.blink_on { 1.0 } else { 0.25 });
        }
        return light_surface(STEEL, 1.0, l, haze);
    }
    // огонь на крыше высоких зданий без шпиля
    if top > 0.32 && up > top - 0.004 && (lx - w * 0.5).abs() < 0.002 {
        return scale(AVIATION_RED, if l.blink_on { 1.0 } else { 0.25 });
    }
    let albedo = match b.mat {
        Material::Limestone => LIMESTONE,
        Material::Concrete => CONCRETE,
        Material::Glass => GLASS,
        Material::Brick => BRICK,
    };
    // объём: грань, отвёрнутая от солнца, темнее
    let sun_left = l.sun_x < (b.x0 + b.x1) * 0.5;
    let edge = w * 0.14;
    let face = if lx < edge && !sun_left || lx > w - edge && sun_left { 0.55 } else { 1.0 };
    // карниз на верхушках ярусов
    for t in &b.tiers {
        if up < t.top && up > t.top - 0.0025 && lx >= t.inset && lx < w - t.inset {
            return light_surface(scale(albedo, 1.25), face, l, haze);
        }
    }
    let fz = up / FLOOR_H;
    let fx = lx / WIN_SP;
    let (floor, col) = (fz.floor() as i32, fx.floor() as i32);
    let (ffz, ffx) = (frac(fz), frac(fx));
    // первый этаж: витрины
    if floor == 0 {
        let shop = ffx > 0.1 && ffx < 0.9 && ffz > 0.15 && ffz < 0.8;
        if shop {
            let on = hash3(b.seed, col, 1) < 0.7;
            let night = scale(WARM_WIN, 0.8 * on as i32 as f32);
            return mix(add(scale(GLASS, 0.3), mul(l.ambient, GLASS)), night, l.night_k);
        }
        return light_surface(scale(albedo, 0.8), face, l, haze);
    }
    let (wx0, wx1, wz0, wz1) = match b.mat {
        Material::Glass => (0.12, 1.0, 0.1, 1.0),
        Material::Brick => (0.3, 0.7, 0.25, 0.8),
        Material::Limestone => (0.28, 0.72, 0.2, 0.85),
        Material::Concrete => (0.15, 0.85, 0.22, 0.88),
    };
    let in_win = ffx >= wx0 && ffx < wx1 && ffz >= wz0 && ffz < wz1;
    if !in_win {
        let mut a = albedo;
        if b.mat == Material::Brick {
            a = scale(a, 0.85 + 0.3 * hash3((up * 900.0) as i32, (x * 300.0) as i32, b.seed));
        }
        if b.mat == Material::Glass {
            a = scale(STEEL, 1.2); // импосты стеклянной башни
        }
        return light_surface(a, face, l, haze);
    }
    // окно: ночью — свет комнаты, днём — отражение неба
    let hv = hash3(b.seed.wrapping_add(col * 7), floor, 3);
    let lit = hv < l.lit_frac;
    let refl = l.sky(x, (up * 0.5 + 0.25).min(HORIZON));
    let glass_k = if b.mat == Material::Glass { 0.75 } else { 0.45 };
    let mut c = scale(mix(GLASS, refl, glass_k), face * (0.4 + 0.6 * l.sun_k) + 0.1);
    if lit {
        let warm = hash3(col, floor, b.seed) < 0.7 && b.mat != Material::Glass;
        let wc = if warm { WARM_WIN } else { COOL_WIN };
        let k = (0.55 + 0.45 * hash3(col, floor, b.seed + 1)) * (0.25 + 0.75 * l.night_k);
        c = add(scale(c, 1.0 - l.night_k), scale(wc, k));
    }
    mix(c, l.sky_hz, haze)
}

fn light_surface(albedo: Rgb, face: f32, l: &Light, haze: f32) -> Rgb {
    let sun = scale(l.sun_col, l.sun_k * 0.9 * face);
    let light = add(scale(l.ambient, 0.35 + 0.35 * face), sun);
    mix(mul(albedo, light), l.sky_hz, haze)
}

fn shade_ground(x: f32, down: f32, l: &Light) -> Rgb {
    // down: 0 у горизонта, 1 внизу экрана
    let base = if down < 0.07 { SIDEWALK } else { ASPHALT };
    let mut c = mul(base, add(scale(l.ambient, 0.8), scale(l.sun_col, l.sun_k * 0.7)));
    // разметка: пунктир посередине дороги
    if (down - 0.45).abs() < 0.012 && frac(x * 9.0) < 0.5 {
        c = scale([0.9, 0.7, 0.2], 0.25 + 0.6 * l.sun_k);
    }
    // световые пятна фонарей
    if l.night_k > 0.0 {
        let lx = frac(x / LAMP_SP + 0.5) - 0.5;
        let pool = (-(lx * lx) * 60.0 - (down - 0.1) * (down - 0.1) * 40.0).exp();
        c = add(c, scale(LAMP, pool * 0.6 * l.night_k));
    }
    c
}

/// Фонарный столб в точке (если есть).
fn lamp_post(x: f32, up: f32, l: &Light) -> Option<Rgb> {
    let lx = (frac(x / LAMP_SP + 0.5) - 0.5) * LAMP_SP;
    if up > LAMP_H + 0.003 {
        return None;
    }
    if up > LAMP_H - 0.004 && lx.abs() < 0.006 {
        let on = 0.15 + 2.0 * l.night_k;
        return Some(scale(LAMP, on));
    }
    if lx.abs() < 0.0012 {
        return Some(scale(STEEL, 0.3 + 0.5 * l.sun_k));
    }
    None
}

/// Что стоит на данной вертикали: здание и высота силуэта в каждом из трёх слоёв.
/// Считается один раз на столбец, а не на каждую точку.
struct Column<'a> {
    x: f32,
    hits: [Option<(&'a Building, f32)>; 3],
}

impl<'a> Column<'a> {
    fn new(city: &'a City, x: f32) -> Column<'a> {
        let hits = [0, 1, 2].map(|k| city.at(k, x).map(|b| (b, b.height_at(x))));
        Column { x, hits }
    }
    /// Видно ли небо на высоте up (для звёзд).
    fn is_sky(&self, up: f32) -> bool {
        up > LAMP_H + 0.003 && self.hits.iter().all(|h| h.is_none_or(|(_, top)| up >= top))
    }
}

/// Цвет в точке столбца на высоте `up` над горизонтом (отрицательная — земля).
/// `sky_grad` — градиент неба для этой строки.
fn sample(col: &Column, l: &Light, up: f32, sky_grad: Rgb) -> Rgb {
    let x = col.x;
    if up < 0.0 {
        return shade_ground(x, -up / (1.0 - HORIZON), l);
    }
    if let Some(c) = lamp_post(x, up, l) {
        return c;
    }
    const HAZE: [f32; 3] = [0.0, 0.12, 0.28];
    // слои: ближний (2) перекрывает средний (1) и дальний (0)
    for layer in [2, 1, 0] {
        if let Some((b, top)) = col.hits[layer]
            && up < top
        {
            return shade_building(b, x, up, l, HAZE[2 - layer]);
        }
    }
    l.sky_sun(sky_grad, x, up)
}

/// Нарисовать сцену во всю сетку. `cell_px` — размер ячейки, нужен для пропорций.
pub fn draw(grid: &mut Grid, style: &Style, city: &City, l: &Light, cell_px: (u32, u32), pan: f32) {
    let (cols, rows) = (grid.cols, grid.rows);
    if cols == 0 || rows == 0 {
        return;
    }
    // единица мира = высота экрана; ширина ячейки в этих единицах
    let cell_h = 1.0 / rows as f32;
    let cell_w = cell_h * cell_px.0 as f32 / cell_px.1 as f32;
    // крупные ячейки усредняют несколько точек — детали превращаются в цвет
    let ss = ((cell_px.1 as f32 / 6.0).round() as u32).clamp(1, 4);
    let inv = 1.0 / (ss * ss) as f32;
    let star = glyph_index('.');
    let star2 = glyph_index('*');
    let n = ss as usize;
    // столбцы и градиент неба считаются один раз на кадр
    let columns: Vec<Column> =
        (0..cols as usize * n).map(|k| Column::new(city, pan + (k as f32 + 0.5) / ss as f32 * cell_w)).collect();
    let ups: Vec<f32> = (0..rows as usize * n).map(|k| HORIZON - (k as f32 + 0.5) / ss as f32 * cell_h).collect();
    let grads: Vec<Rgb> = ups.iter().map(|&up| l.sky_gradient(up)).collect();
    for r in 0..rows as usize {
        for c in 0..cols as usize {
            let mut acc = [0.0; 3];
            for sy in 0..n {
                let k = r * n + sy;
                for sx in 0..n {
                    acc = add(acc, sample(&columns[c * n + sx], l, ups[k], grads[k]));
                }
            }
            let col = scale(acc, inv * l.exposure);
            let (ci, ri) = (c as i32, r as i32);
            // звёзды: только в чистом небе, ночью
            let h = hash3(ci, ri, 99);
            if l.night_k > 0.3 && h < STAR_DENSITY && columns[c * n + n / 2].is_sky(ups[r * n + n / 2]) {
                let k = 0.5 + 60.0 * h;
                let glyph = if h < 0.001 { star2 } else { star };
                style.put(grid, ci, ri, add(col, scale([0.9, 0.9, 1.0], k * l.night_k)), Some(glyph));
                continue;
            }
            style.put(grid, ci, ri, col, None);
        }
    }
}
