//! Строитель сцен: параметрические детали города, описанные кодом.
//! Единицы — метры; Y вверх; земля (асфальт) — плоскость y = 0.

use crate::sdf::{
    Group, Mat, NO_COLLIDE, PointLight, Prim, SHAPE_BOX, SHAPE_CAP, SHAPE_CYL, Scene, Trigger, V3, facade, mat,
};
use crate::subtitles::Line;
use std::ops::Range;

// ---------------------------------------------------------------- размеры (реальные)
pub const SIDEWALK_H: f32 = 0.15;
pub const CURB_W: f32 = 0.25;

/// Цвета сдержанной палитры.
pub mod pal {
    use crate::sdf::V3;
    pub const STEEL_DARK: V3 = [0.12, 0.13, 0.14];
    pub const FIRE_ESCAPE: V3 = [0.10, 0.11, 0.10];
    pub const LAMP_POST: V3 = [0.16, 0.20, 0.17];
    pub const SODIUM: V3 = [1.0, 0.62, 0.30];
    pub const WARM_BULB: V3 = [1.0, 0.78, 0.48];
    pub const CREAM: V3 = [0.78, 0.72, 0.58];
    pub const DINER_RED: V3 = [0.62, 0.08, 0.07];
    pub const MINT: V3 = [0.45, 0.68, 0.60];
    pub const WOOD_DARK: V3 = [0.25, 0.14, 0.08];
    pub const DOOR_GREEN: V3 = [0.08, 0.20, 0.14];
    pub const TANK_WOOD: V3 = [0.36, 0.25, 0.16];
    pub const YELLOW_PAINT: V3 = [0.85, 0.65, 0.12];
    pub const WHITE_PAINT: V3 = [0.80, 0.80, 0.78];
}

/// Параметры фасада.
#[derive(Clone, Copy)]
pub struct Facade {
    pub style: u32,
    pub floor_h: f32,
    pub win_spacing: f32,
    pub lit: f32,
    pub ground_h: f32,
}

impl Facade {
    pub fn brick() -> Facade {
        Facade { style: facade::BRICK, floor_h: 3.1, win_spacing: 2.2, lit: 0.45, ground_h: 3.6 }
    }
    pub fn brick_dark() -> Facade {
        Facade { style: facade::BRICK_DARK, ..Facade::brick() }
    }
    pub fn limestone() -> Facade {
        Facade { style: facade::LIMESTONE, floor_h: 3.4, win_spacing: 2.0, lit: 0.35, ground_h: 4.5 }
    }
    pub fn office() -> Facade {
        Facade { style: facade::OFFICE, floor_h: 3.6, win_spacing: 1.6, lit: 0.5, ground_h: 4.5 }
    }
    pub fn glass() -> Facade {
        Facade { style: facade::GLASS, floor_h: 3.8, win_spacing: 1.5, lit: 0.55, ground_h: 5.0 }
    }
    fn mat(&self) -> Mat {
        Mat { id: mat::FACADE, p: [self.floor_h, self.win_spacing, self.lit, self.ground_h], style: self.style }
    }
    /// Высота здания с данным числом этажей.
    pub fn height(&self, floors: u32) -> f32 {
        self.ground_h + (floors.max(1) - 1) as f32 * self.floor_h
    }
}

pub struct Builder {
    scene: Scene,
    seed: u32,
}

impl Builder {
    pub fn new(name: &'static str) -> Builder {
        Builder {
            scene: Scene {
                name,
                prims: Vec::new(),
                colliders: Vec::new(),
                lights: Vec::new(),
                spawn: ([0.0, 0.0, 0.0], 0.0),
                time_of_day: 0.8,
                fog: 0.004,
                triggers: Vec::new(),
                envelopes: Vec::new(),
                moon: None,
            },
            seed: 1,
        }
    }

    pub fn finish(self) -> Scene {
        self.scene
    }

    /// Текущее число форм (начало диапазона для `since`).
    pub fn mark(&self) -> usize {
        self.scene.prims.len()
    }
    /// Диапазон форм, добавленных после `mark`.
    pub fn since(&self, mark: usize) -> Range<usize> {
        mark..self.scene.prims.len()
    }
    /// Формы будут двигаться: расширить их габариты для сетки ускорения на сдвиги `lo..hi`.
    pub fn moving(&mut self, r: Range<usize>, lo: V3, hi: V3) {
        for i in r {
            let (a, b) = self.scene.prims[i].aabb();
            let mn = [0, 1, 2].map(|k| a[k] + lo[k].min(0.0));
            let mx = [0, 1, 2].map(|k| b[k] + hi[k].max(0.0));
            self.scene.envelopes.push((i, mn, mx));
        }
    }
    /// Формы не участвуют в коллизиях.
    pub fn no_collide(&mut self, r: Range<usize>) {
        for i in r {
            self.scene.prims[i].k[0] |= NO_COLLIDE;
        }
    }
    pub fn moon(&mut self, dir: V3, radius_deg: f32) {
        let l = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
        self.scene.moon = Some((dir.map(|v| v / l), radius_deg.to_radians()));
    }

    pub fn spawn(&mut self, feet: V3, yaw_deg: f32) {
        self.scene.spawn = (feet, yaw_deg.to_radians());
    }
    pub fn time_of_day(&mut self, t: f32) {
        self.scene.time_of_day = t;
    }
    pub fn fog(&mut self, density: f32) {
        self.scene.fog = density;
    }
    pub fn light(&mut self, pos: V3, color: V3, radius: f32, power: f32) {
        self.scene.lights.push(PointLight { pos, radius, color, power });
    }
    pub fn trigger(&mut self, min: V3, max: V3, lines: Vec<Line>) {
        self.scene.triggers.push(Trigger { min, max, lines, fired: false });
    }

    // ------------------------------------------------------------ формы
    fn prim(&mut self, g: Group, shape: u32, center: V3, half: V3, round: f32, m: Mat) -> Prim {
        self.seed = self.seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        Prim {
            a: [center[0], center[1], center[2], round],
            b: [half[0], half[1], half[2], 0.0],
            m: m.p,
            k: [shape | ((g as u32) << 8), m.id, m.style, self.seed >> 8],
        }
    }

    /// Коробка по углам.
    pub fn cube(&mut self, g: Group, min: V3, max: V3, m: Mat) {
        let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        let h = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5, (max[2] - min[2]) * 0.5];
        let p = self.prim(g, SHAPE_BOX, c, h, 0.0, m);
        self.scene.prims.push(p);
    }

    /// Скруглённая коробка.
    pub fn rounded(&mut self, g: Group, min: V3, max: V3, r: f32, m: Mat) {
        let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        let h = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5, (max[2] - min[2]) * 0.5];
        let p = self.prim(g, SHAPE_BOX, c, h, r, m);
        self.scene.prims.push(p);
    }

    /// Вертикальный цилиндр: центр основания, радиус, высота.
    pub fn cyl(&mut self, g: Group, base: V3, r: f32, h: f32, m: Mat) {
        let p = self.prim(g, SHAPE_CYL, [base[0], base[1] + h * 0.5, base[2]], [r, h * 0.5, 0.0], 0.0, m);
        self.scene.prims.push(p);
    }

    /// Капсула от `a` до `b` радиуса `r` (при a = b — шар).
    pub fn capsule(&mut self, g: Group, a: V3, b: V3, r: f32, m: Mat) {
        self.seed = self.seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
        self.scene.prims.push(Prim {
            a: [a[0], a[1], a[2], r],
            b: [b[0], b[1], b[2], 0.0],
            m: m.p,
            k: [SHAPE_CAP | ((g as u32) << 8), m.id, m.style, self.seed >> 8],
        });
    }

    // ------------------------------------------------------------ люди
    /// Параметрический человек: ноги в `feet`, взгляд по `yaw` (рад, 0 — вдоль +Z).
    /// Возвращает диапазон форм (для движения и флагов).
    pub fn person(&mut self, feet: V3, yaw: f32, look: &Look, pose: Pose) -> Range<usize> {
        let start = self.mark();
        let (s, c) = yaw.sin_cos();
        // локальные координаты: x — вправо, y — вверх, z — вперёд
        let w = |p: V3| [feet[0] + p[0] * c + p[2] * s, feet[1] + p[1], feet[2] - p[0] * s + p[2] * c];
        let g = Group::Detail;
        let cloth = |col: V3| Mat::rgb(mat::VINYL, col);
        let matte = |col: V3| Mat::rgb(mat::PLASTER, col);
        let k = if look.female { 0.93 } else { 1.0 };
        let hip = 0.9 * k;
        let sh = 1.42 * k;
        let head = 1.65 * k;
        // ноги и обувь
        for x in [-0.1f32, 0.1] {
            let x = x * k;
            self.capsule(g, w([x, hip, 0.0]), w([x, 0.5 * k, 0.03]), 0.075, cloth(look.pants));
            self.capsule(g, w([x, 0.5 * k, 0.03]), w([x, 0.1, 0.0]), 0.06, cloth(look.pants));
            self.capsule(g, w([x, 0.06, -0.03]), w([x, 0.06, 0.12]), 0.055, matte(look.shoes));
        }
        // таз, торс, плечи
        self.capsule(g, w([-0.09 * k, hip + 0.04, 0.0]), w([0.09 * k, hip + 0.04, 0.0]), 0.11 * k, cloth(look.pants));
        let torso_r = if look.female { 0.145 } else { 0.17 };
        self.capsule(g, w([0.0, hip + 0.1, 0.0]), w([0.0, sh - 0.06, 0.0]), torso_r, cloth(look.top));
        self.capsule(g, w([-0.17 * k, sh, 0.0]), w([0.17 * k, sh, 0.0]), 0.075, cloth(look.top));
        // шея и голова
        self.capsule(g, w([0.0, sh + 0.04, 0.0]), w([0.0, head - 0.09, 0.0]), 0.05, matte(look.skin));
        self.capsule(g, w([0.0, head, 0.0]), w([0.0, head, 0.0]), 0.11, matte(look.skin));
        // причёска
        match look.hair {
            Hair::Bald => {}
            Hair::Short => self.capsule(
                g,
                w([0.0, head + 0.03, -0.02]),
                w([0.0, head + 0.03, -0.02]),
                0.112,
                matte(look.hair_color),
            ),
            Hair::Long => {
                self.capsule(
                    g,
                    w([0.0, head + 0.03, -0.02]),
                    w([0.0, head + 0.03, -0.02]),
                    0.115,
                    matte(look.hair_color),
                );
                self.capsule(g, w([0.0, head - 0.02, -0.07]), w([0.0, sh - 0.02, -0.09]), 0.1, matte(look.hair_color));
            }
            Hair::Hat => {
                self.cyl(g, w([0.0, head + 0.06, 0.0]), 0.18, 0.02, matte(look.hair_color));
                self.cyl(g, w([0.0, head + 0.06, 0.0]), 0.12, 0.13, matte(look.hair_color));
            }
        }
        // руки
        let arm = cloth(look.top);
        match pose {
            Pose::Stand => {
                for x in [-1.0f32, 1.0] {
                    let elbow = w([x * 0.22 * k, sh - 0.3, 0.02]);
                    self.capsule(g, w([x * 0.19 * k, sh, 0.0]), elbow, 0.055, arm);
                    self.capsule(g, elbow, w([x * 0.23 * k, sh - 0.6, 0.05]), 0.05, arm);
                }
            }
            Pose::Aim => {
                // правая рука вытянута вперёд, левая поддерживает
                let hand = [0.05, sh - 0.02, 0.62];
                self.capsule(g, w([0.19 * k, sh, 0.0]), w(hand), 0.055, arm);
                self.capsule(g, w([-0.19 * k, sh, 0.0]), w([0.0, sh - 0.06, 0.56]), 0.05, arm);
                self.capsule(g, w(hand), w(hand), 0.045, matte(look.skin));
                let steel = Mat::rgb(mat::METAL, [0.05, 0.05, 0.055]);
                self.capsule(
                    g,
                    w([hand[0], hand[1] + 0.05, hand[2] - 0.03]),
                    w([hand[0], hand[1] + 0.06, hand[2] + 0.17]),
                    0.028,
                    steel,
                );
                self.capsule(
                    g,
                    w([hand[0], hand[1] + 0.02, hand[2]]),
                    w([hand[0], hand[1] - 0.06, hand[2] - 0.02]),
                    0.025,
                    steel,
                );
            }
        }
        let r = self.since(start);
        let _ = start;
        r
    }

    /// Невидимое препятствие (стекло витрины).
    pub fn collider(&mut self, min: V3, max: V3) {
        let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
        let h = [(max[0] - min[0]) * 0.5, (max[1] - min[1]) * 0.5, (max[2] - min[2]) * 0.5];
        let p = self.prim(Group::Solid, SHAPE_BOX, c, h, 0.0, Mat::new(mat::CONCRETE));
        self.scene.colliders.push(p);
    }

    // ------------------------------------------------------------ улица
    /// Тротуар вдоль Z с бордюром со стороны дороги (`road_side` = +1 — дорога при x > x1).
    pub fn sidewalk(&mut self, x0: f32, x1: f32, z0: f32, z1: f32, road_side: f32) {
        self.cube(Group::Solid, [x0, 0.0, z0], [x1, SIDEWALK_H, z1], Mat::new(mat::SIDEWALK));
        let (c0, c1) = if road_side > 0.0 { (x1 - CURB_W, x1) } else { (x0, x0 + CURB_W) };
        self.cube(Group::Detail, [c0, 0.0, z0], [c1, SIDEWALK_H + 0.005, z1], Mat::new(mat::CONCRETE));
    }

    /// Двойная жёлтая осевая линия вдоль Z.
    pub fn center_line(&mut self, x: f32, z0: f32, z1: f32) {
        let m = Mat::rgb(mat::PAINT, pal::YELLOW_PAINT);
        for dx in [-0.12, 0.12] {
            self.cube(Group::Detail, [x + dx - 0.05, 0.0, z0], [x + dx + 0.05, 0.004, z1], m);
        }
    }

    /// «Зебра» поперёк дороги (полосы вдоль Z).
    pub fn crosswalk(&mut self, x0: f32, x1: f32, z0: f32, z1: f32) {
        let m = Mat::rgb(mat::PAINT, pal::WHITE_PAINT);
        let mut x = x0 + 0.3;
        while x + 0.5 < x1 {
            self.cube(Group::Detail, [x, 0.0, z0], [x + 0.5, 0.004, z1], m);
            x += 1.1;
        }
    }

    /// Уличный фонарь: столб на тротуаре, кронштейн над дорогой (`arm` = ±1 по X).
    pub fn street_lamp(&mut self, x: f32, z: f32, arm: f32) {
        let post = Mat::rgb(mat::METAL, pal::LAMP_POST);
        let y0 = SIDEWALK_H;
        self.cyl(Group::Detail, [x, y0, z], 0.12, 0.5, post);
        self.cyl(Group::Detail, [x, y0, z], 0.075, 7.2, post);
        let ax = x + arm * 2.2;
        self.cube(Group::Detail, [x.min(ax), y0 + 7.0, z - 0.05], [x.max(ax), y0 + 7.1, z + 0.05], post);
        self.rounded(Group::Detail, [ax - 0.35, y0 + 6.85, z - 0.2], [ax + 0.35, y0 + 7.08, z + 0.2], 0.05, post);
        self.cube(
            Group::Detail,
            [ax - 0.28, y0 + 6.8, z - 0.14],
            [ax + 0.28, y0 + 6.86, z + 0.14],
            Mat::glow(pal::SODIUM, 3.0),
        );
        self.light([ax, y0 + 6.4, z], pal::SODIUM, 18.0, 28.0);
    }

    // ------------------------------------------------------------ здания
    /// Здание-коробка с фасадом.
    pub fn building(&mut self, min: [f32; 2], max: [f32; 2], f: Facade, floors: u32) -> f32 {
        let h = f.height(floors);
        self.cube(Group::Solid, [min[0], 0.0, min[1]], [max[0], h, max[1]], f.mat());
        // карниз по верху
        self.cube(Group::Detail, [min[0] - 0.25, h - 0.4, min[1] - 0.25], [max[0] + 0.25, h, max[1] + 0.25], f.mat());
        h
    }

    /// Ярус (уступ) поверх здания: от высоты `y0`, `floors` этажей.
    pub fn tier(&mut self, min: [f32; 2], max: [f32; 2], y0: f32, f: Facade, floors: u32) -> f32 {
        let h = y0 + floors as f32 * f.floor_h;
        let m = Mat { p: [f.floor_h, f.win_spacing, f.lit, 0.0], ..f.mat() }; // без первого этажа
        self.cube(Group::Solid, [min[0], y0, min[1]], [max[0], h, max[1]], m);
        h
    }

    /// Дальняя башня (силуэт): подиум, уступы, шпиль с авиационным огнём.
    pub fn far_tower(&mut self, (x, z): (f32, f32), (w, d): (f32, f32), h: f32, f: Facade, spire: f32) {
        let mut m = f.mat();
        m.p[3] = 0.0;
        let tiers = [(0.0, 1.0), (0.12, 0.72), (0.24, 0.9)];
        let mut y = 0.0;
        for (k, (inset, frac)) in tiers.iter().enumerate() {
            let top = if k == 2 { h } else { h * frac * if k == 0 { 0.45 } else { 1.0 } };
            if top <= y {
                continue;
            }
            let (ix, iz) = (w * inset, d * inset);
            self.cube(
                Group::Far,
                [x - w / 2.0 + ix, y, z - d / 2.0 + iz],
                [x + w / 2.0 - ix, top, z + d / 2.0 - iz],
                m,
            );
            y = top;
        }
        if spire > 0.0 {
            self.cyl(Group::Far, [x, h, z], 0.4, spire, Mat::rgb(mat::METAL, pal::STEEL_DARK));
            self.cube(
                Group::Far,
                [x - 0.5, h + spire, z - 0.5],
                [x + 0.5, h + spire + 1.0, z + 0.5],
                Mat::glow([1.0, 0.08, 0.04], 6.0),
            );
        }
    }

    /// Пожарная лестница на фасаде x = `face_x`, выступает в сторону `out` (±1), по Z от z0 до z1.
    pub fn fire_escape(&mut self, face_x: f32, out: f32, z0: f32, z1: f32, f: Facade, floors: u32) {
        let m = Mat::rgb(mat::METAL, pal::FIRE_ESCAPE);
        let depth = 1.25;
        let xo = face_x + out * depth;
        let (xa, xb) = (face_x.min(xo), face_x.max(xo));
        for k in 1..floors {
            let y = f.ground_h + (k - 1) as f32 * f.floor_h + 0.05;
            // площадка
            self.cube(Group::Detail, [xa, y - 0.06, z0], [xb, y, z1], m);
            // перила: верх, середина, стойки
            for (ry, t) in [(1.0, 0.05), (0.5, 0.025)] {
                self.cube(Group::Detail, [xo - 0.03, y + ry - t, z0], [xo + 0.03, y + ry, z1], m);
                self.cube(Group::Detail, [xa, y + ry - t, z0 - 0.03], [xb, y + ry, z0 + 0.03], m);
                self.cube(Group::Detail, [xa, y + ry - t, z1 - 0.03], [xb, y + ry, z1 + 0.03], m);
            }
            let mut z = z0;
            while z <= z1 + 0.01 {
                self.cube(Group::Detail, [xo - 0.025, y, z - 0.025], [xo + 0.025, y + 1.0, z + 0.025], m);
                z += (z1 - z0) / 4.0;
            }
            // марш вниз к предыдущей площадке (последний — выдвижная лестница)
            let steps = 7;
            let rise = if k == 1 { 1.6 } else { f.floor_h };
            let run = (z1 - z0) * 0.6;
            let (sx0, sx1) = if out > 0.0 { (xa + 0.1, xa + 0.75) } else { (xb - 0.75, xb - 0.1) };
            for s in 0..steps {
                let t = s as f32 / steps as f32;
                let sy = y - 0.06 - rise * t;
                let sz = z0 + 0.2 + run * t;
                self.cube(Group::Detail, [sx0, sy - 0.03, sz], [sx1, sy, sz + run / steps as f32 + 0.05], m);
            }
            // тетива марша (боковины) — ступенчатая линия из тонких брусков
            for s in 0..steps {
                let t = s as f32 / steps as f32;
                let sy = y - 0.06 - rise * t;
                let sz = z0 + 0.2 + run * t;
                let x = if out > 0.0 { sx1 } else { sx0 };
                self.cube(Group::Detail, [x - 0.02, sy - 0.15, sz], [x + 0.02, sy, sz + run / steps as f32], m);
            }
        }
    }

    /// Водонапорный бак на крыше: опоры, деревянная бочка, коническая крыша (ступенями).
    pub fn water_tower(&mut self, x: f32, roof: f32, z: f32) {
        let steel = Mat::rgb(mat::METAL, pal::STEEL_DARK);
        let wood = Mat::rgb(mat::WOOD, pal::TANK_WOOD);
        for (dx, dz) in [(-1.1, -1.1), (1.1, -1.1), (-1.1, 1.1), (1.1, 1.1)] {
            self.cube(
                Group::Detail,
                [x + dx - 0.08, roof, z + dz - 0.08],
                [x + dx + 0.08, roof + 2.6, z + dz + 0.08],
                steel,
            );
        }
        self.cube(Group::Detail, [x - 1.5, roof + 2.5, z - 1.5], [x + 1.5, roof + 2.65, z + 1.5], steel);
        self.cyl(Group::Detail, [x, roof + 2.65, z], 1.55, 3.2, wood);
        for (k, r) in [1.45, 1.05, 0.65, 0.3].iter().enumerate() {
            self.cyl(Group::Detail, [x, roof + 5.85 + k as f32 * 0.22, z], *r, 0.22, wood);
        }
    }

    /// Вход в дом с крыльцом (stoop) на фасаде x = `face_x`, крыльцо в сторону `out`.
    pub fn stoop(&mut self, face_x: f32, out: f32, z: f32, door_w: f32) {
        let stone = Mat::new(mat::CONCRETE);
        let y0 = SIDEWALK_H;
        let n = 5;
        let step_h = 0.18;
        for s in 0..n {
            let len = 0.32 * (n - s) as f32;
            let x2 = face_x + out * len;
            let (a, b) = (face_x.min(x2), face_x.max(x2));
            self.cube(
                Group::Solid,
                [a, y0, z - door_w * 0.5 - 0.4],
                [b, y0 + step_h * (s + 1) as f32, z + door_w * 0.5 + 0.4],
                stone,
            );
        }
        // дверь (утоплена) и козырёк
        let top = y0 + step_h * n as f32;
        let d = Mat::rgb(mat::WOOD, pal::DOOR_GREEN);
        self.cube(
            Group::Cut,
            [face_x - 0.25, top, z - door_w * 0.5],
            [face_x + 0.25, top + 2.3, z + door_w * 0.5],
            Mat::rgb(mat::PLASTER, pal::CREAM),
        );
        let dx = face_x - out * 0.18;
        self.cube(Group::Detail, [dx - 0.04, top, z - door_w * 0.5], [dx + 0.04, top + 2.3, z + door_w * 0.5], d);
        self.cube(
            Group::Detail,
            [face_x.min(face_x + out * 0.5), top + 2.45, z - door_w * 0.5 - 0.3],
            [face_x.max(face_x + out * 0.5), top + 2.55, z + door_w * 0.5 + 0.3],
            Mat::new(mat::CONCRETE),
        );
        self.light([face_x + out * 0.4, top + 2.35, z], pal::WARM_BULB, 5.0, 3.0);
        self.cube(
            Group::Detail,
            [face_x.min(face_x + out * 0.12), top + 2.32, z - 0.08],
            [face_x.max(face_x + out * 0.12), top + 2.42, z + 0.08],
            Mat::glow(pal::WARM_BULB, 2.0),
        );
    }
}

// ---------------------------------------------------------------- внешность людей
#[derive(Clone, Copy)]
pub enum Hair {
    Bald,
    Short,
    Long,
    Hat,
}

#[derive(Clone, Copy)]
pub enum Pose {
    Stand,
    /// Целится вперёд (пистолет в вытянутой руке).
    Aim,
}

/// Внешность: пол, причёска, одежда. Люди отличаются только этим.
#[derive(Clone, Copy)]
pub struct Look {
    pub female: bool,
    pub skin: V3,
    pub hair: Hair,
    pub hair_color: V3,
    pub top: V3,
    pub pants: V3,
    pub shoes: V3,
}
