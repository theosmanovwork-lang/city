//! Сцена из параметрических форм (SDF) — без 3D-моделей.
//!
//! Формы: коробка (со скруглением) и вертикальный цилиндр. Группы:
//!   `Solid`  — стены, дома, земля: объединение;
//!   `Cut`    — вырезы (комнаты, проёмы): вычитаются из Solid, поверхность выреза
//!              получает материал выреза (штукатурка комнаты, рама проёма);
//!   `Detail` — мебель, перила, фонари: добавляются после вырезов;
//!   `Far`    — дальний силуэт: проверяется всегда, вне сетки ускорения.
//! Итоговое расстояние: `min(max(min(земля, Far, Solid), -Cut), Detail)`.
//!
//! Сетка ускорения (XZ): для каждой ячейки — списки форм каждой группы,
//! чьи габариты её задевают. Луч в ячейке проверяет только их и не шагает
//! дальше границы ячейки. Тот же расчёт расстояния есть на CPU (`Scene::sdf`) — для коллизий.

use crate::subtitles::Line;

pub type V3 = [f32; 3];

/// Форма для GPU: 64 байта, совпадает со `struct Prim` в march.wgsl.
/// a.xyz — центр, a.w — скругление; b.xyz — полуразмеры (цилиндр: b.x радиус, b.y полувысота);
/// m — параметры материала; k = [форма | группа << 8, материал, стиль, зерно].
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Prim {
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub m: [f32; 4],
    pub k: [u32; 4],
}

pub const SHAPE_BOX: u32 = 0;
pub const SHAPE_CYL: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Solid = 0,
    Cut = 1,
    Detail = 2,
    Far = 3,
}

/// Материалы (номера совпадают с march.wgsl).
pub mod mat {
    /// Асфальт — материал земли (плоскость y = 0) в шейдере.
    #[allow(dead_code)]
    pub const ASPHALT: u32 = 0;
    pub const SIDEWALK: u32 = 1;
    pub const CONCRETE: u32 = 2;
    /// m = (высота этажа, шаг окон, доля горящих окон, высота первого этажа); стиль — `facade::*`.
    pub const FACADE: u32 = 3;
    /// m.rgb — цвет.
    pub const PLASTER: u32 = 4;
    pub const TILE: u32 = 5;
    /// m.rgb — цвет.
    pub const METAL: u32 = 6;
    pub const CHROME: u32 = 7;
    /// m.rgb — цвет.
    pub const VINYL: u32 = 8;
    /// m.rgb — цвет полос (вторая полоса — кремовая).
    pub const AWNING: u32 = 9;
    /// m.rgb — цвет, m.w — яркость.
    pub const EMISSIVE: u32 = 10;
    /// m.rgb — цвет.
    pub const FORMICA: u32 = 11;
    /// m.rgb — цвет.
    pub const WOOD: u32 = 12;
    /// m.rgb — цвет (разметка).
    pub const PAINT: u32 = 13;
    /// m.rgb — цвет, m.w — яркость (световой короб вывески).
    pub const SIGN: u32 = 14;
}

/// Стили фасада.
pub mod facade {
    pub const BRICK: u32 = 0;
    pub const LIMESTONE: u32 = 1;
    pub const OFFICE: u32 = 2;
    pub const GLASS: u32 = 3;
    pub const BRICK_DARK: u32 = 4;
}

#[derive(Clone, Copy, Debug)]
pub struct Mat {
    pub id: u32,
    pub p: [f32; 4],
    pub style: u32,
}

impl Mat {
    pub const fn new(id: u32) -> Mat {
        Mat { id, p: [0.0; 4], style: 0 }
    }
    pub const fn rgb(id: u32, c: V3) -> Mat {
        Mat { id, p: [c[0], c[1], c[2], 1.0], style: 0 }
    }
    pub const fn glow(c: V3, k: f32) -> Mat {
        Mat { id: mat::EMISSIVE, p: [c[0], c[1], c[2], k], style: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub pos: V3,
    pub radius: f32,
    pub color: V3,
    /// Мощность: освещённость на 1 м ≈ power / 2.
    pub power: f32,
}

/// Зона: при первом входе игрока запускает реплики.
pub struct Trigger {
    pub min: V3,
    pub max: V3,
    pub lines: Vec<Line>,
    pub fired: bool,
}

impl Trigger {
    pub fn contains(&self, p: V3) -> bool {
        (0..3).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
}

pub struct Scene {
    pub name: &'static str,
    pub prims: Vec<Prim>,
    /// Невидимые препятствия (стёкла витрин): только для коллизий.
    pub colliders: Vec<Prim>,
    pub lights: Vec<PointLight>,
    /// Ноги игрока при старте и направление взгляда (рад, 0 — вдоль +Z).
    pub spawn: (V3, f32),
    /// Время суток при старте (0 — полночь, 0.5 — полдень).
    pub time_of_day: f32,
    /// Плотность тумана, 1/м.
    pub fog: f32,
    pub triggers: Vec<Trigger>,
}

impl Prim {
    pub fn group(&self) -> u32 {
        self.k[0] >> 8
    }
    pub fn shape(&self) -> u32 {
        self.k[0] & 0xff
    }

    /// Габариты (min, max).
    pub fn aabb(&self) -> (V3, V3) {
        let c = [self.a[0], self.a[1], self.a[2]];
        let h = if self.shape() == SHAPE_CYL {
            [self.b[0], self.b[1], self.b[0]]
        } else {
            [self.b[0], self.b[1], self.b[2]]
        };
        ([c[0] - h[0], c[1] - h[1], c[2] - h[2]], [c[0] + h[0], c[1] + h[1], c[2] + h[2]])
    }

    /// Расстояние со знаком от точки до формы.
    #[inline]
    pub fn sd(&self, p: V3) -> f32 {
        let q = [p[0] - self.a[0], p[1] - self.a[1], p[2] - self.a[2]];
        match self.shape() {
            SHAPE_CYL => {
                let dx = (q[0] * q[0] + q[2] * q[2]).sqrt() - self.b[0];
                let dy = q[1].abs() - self.b[1];
                dx.max(dy).min(0.0) + (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt()
            }
            _ => {
                let r = self.a[3];
                let d = [q[0].abs() - self.b[0] + r, q[1].abs() - self.b[1] + r, q[2].abs() - self.b[2] + r];
                let out = (d[0].max(0.0).powi(2) + d[1].max(0.0).powi(2) + d[2].max(0.0).powi(2)).sqrt();
                out + d[0].max(d[1]).max(d[2]).min(0.0) - r
            }
        }
    }
}

impl Scene {
    /// Расстояние до сцены (CPU, для коллизий): все группы + невидимые препятствия.
    pub fn sdf(&self, p: V3) -> f32 {
        let mut solid = p[1]; // земля
        let mut cut = f32::MAX;
        let mut detail = f32::MAX;
        for pr in &self.prims {
            let d = pr.sd(p);
            match pr.group() {
                1 => cut = cut.min(d),
                2 => detail = detail.min(d),
                _ => solid = solid.min(d),
            }
        }
        let mut d = solid.max(-cut).min(detail);
        for c in &self.colliders {
            d = d.min(c.sd(p));
        }
        d
    }

    /// Нормаль (градиент) расстояния.
    pub fn normal(&self, p: V3) -> V3 {
        let e = 0.01;
        let g = |dx: f32, dy: f32, dz: f32| self.sdf([p[0] + dx, p[1] + dy, p[2] + dz]);
        let n = [g(e, 0.0, 0.0) - g(-e, 0.0, 0.0), g(0.0, e, 0.0) - g(0.0, -e, 0.0), g(0.0, 0.0, e) - g(0.0, 0.0, -e)];
        let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
        [n[0] / l, n[1] / l, n[2] / l]
    }
}

/// Сетка ускорения для GPU.
pub struct Accel {
    pub origin: [f32; 2],
    pub cell: f32,
    pub nx: u32,
    pub nz: u32,
    /// На ячейку 6 чисел: (начало, число) для Solid, Cut, Detail.
    pub ranges: Vec<u32>,
    pub list: Vec<u32>,
    /// Индексы дальних форм (проверяются всегда).
    pub far: (u32, u32),
    /// Выше этой высоты ничего нет.
    pub top: f32,
}

impl Accel {
    pub fn build(scene: &Scene, cell: f32) -> Accel {
        // дальние формы — в начало списка, одним диапазоном
        let mut list: Vec<u32> = Vec::new();
        let far: Vec<u32> = (0..scene.prims.len() as u32).filter(|&i| scene.prims[i as usize].group() == 3).collect();
        let far_range = (0, far.len() as u32);
        list.extend(&far);
        let near: Vec<usize> = (0..scene.prims.len()).filter(|&i| scene.prims[i].group() != 3).collect();
        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
        let mut top: f32 = 0.0;
        for p in &scene.prims {
            let (a, b) = p.aabb();
            top = top.max(b[1]);
            if p.group() != 3 {
                lo = [lo[0].min(a[0]), lo[1].min(a[2])];
                hi = [hi[0].max(b[0]), hi[1].max(b[2])];
            }
        }
        if near.is_empty() {
            lo = [0.0; 2];
            hi = [cell; 2];
        }
        let origin = [lo[0] - cell, lo[1] - cell];
        let nx = ((hi[0] - origin[0]) / cell).ceil() as u32 + 1;
        let nz = ((hi[1] - origin[1]) / cell).ceil() as u32 + 1;
        let mut ranges = vec![0u32; (nx * nz * 6) as usize];
        let mut cells: Vec<[Vec<u32>; 3]> = (0..nx * nz).map(|_| [vec![], vec![], vec![]]).collect();
        for &i in &near {
            let p = &scene.prims[i];
            let (a, b) = p.aabb();
            let x0 = ((a[0] - origin[0]) / cell).floor().max(0.0) as u32;
            let x1 = (((b[0] - origin[0]) / cell).floor() as u32).min(nx - 1);
            let z0 = ((a[2] - origin[1]) / cell).floor().max(0.0) as u32;
            let z1 = (((b[2] - origin[1]) / cell).floor() as u32).min(nz - 1);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    cells[(z * nx + x) as usize][p.group() as usize].push(i as u32);
                }
            }
        }
        for (c, groups) in cells.iter().enumerate() {
            for (g, v) in groups.iter().enumerate() {
                ranges[c * 6 + g * 2] = list.len() as u32;
                ranges[c * 6 + g * 2 + 1] = v.len() as u32;
                list.extend(v);
            }
        }
        if list.is_empty() {
            list.push(0);
        }
        Accel { origin, cell, nx, nz, ranges, list, far: far_range, top: top + 1.0 }
    }

    /// Самая «тяжёлая» ячейка (для отладки производительности).
    pub fn max_per_cell(&self) -> u32 {
        self.ranges.chunks(6).map(|r| r[1] + r[3] + r[5]).max().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxp(c: V3, h: V3, group: u32) -> Prim {
        Prim { a: [c[0], c[1], c[2], 0.0], b: [h[0], h[1], h[2], 0.0], m: [0.0; 4], k: [group << 8, 0, 0, 0] }
    }

    #[test]
    fn box_distance_and_cut() {
        let wall = boxp([0.0, 2.0, 0.0], [2.0, 2.0, 2.0], 0);
        assert!((wall.sd([3.0, 2.0, 0.0]) - 1.0).abs() < 1e-5);
        let room = boxp([0.0, 2.0, 0.0], [1.5, 1.5, 1.5], 1);
        let s = Scene {
            name: "t",
            prims: vec![wall, room],
            colliders: vec![],
            lights: vec![],
            spawn: ([0.0; 3], 0.0),
            time_of_day: 0.5,
            fog: 0.0,
            triggers: vec![],
        };
        // внутри комнаты — свободно, до стены 1.5
        assert!((s.sdf([0.0, 2.0, 0.0]) - 1.5).abs() < 1e-5);
        let a = Accel::build(&s, 2.0);
        assert!(a.max_per_cell() >= 2);
    }
}
