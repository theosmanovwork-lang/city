//! Локация 1 «Сон. Переулок».
//!
//! Герой стоит в начале тёмного переулка, в 5 м позади — человек с пистолетом.
//! Впереди фонарь. Пока герой идёт шагом, фонарь не приближается (сон).
//! Когда он побежит, стены начинают сходиться; последние метры — почти впритык,
//! время замедляется. У фонаря всё уходит в белое — пробуждение, локация заново.
//! Если стоять, идти слишком долго шагом или пятиться к стрелку — выстрел, сон сначала.
//!
//! Сюрреализм: стрелок всегда ровно в 5 м позади; окна у самой земли; двери на высоте
//! второго этажа; предметы у стен «уходят» в стены, когда переулок сужается;
//! огромная луна в просвете над фонарём.

use crate::build::{Builder, Facade, Hair, Look, Pose, pal};
use crate::logic::{Ctx, Flow, Logic};
use crate::sdf::{Group, Mat, Scene, V3, facade, mat};
use crate::subtitles::{Line, line};
use std::ops::Range;

// ---------------------------------------------------------------- размеры, м
const HW: f32 = 1.6; // полуширина переулка
const HW_MIN: f32 = 0.37; // в самом узком месте: плечи проходят впритык
const SEC: f32 = 2.2; // длина секции стены (кратна шагу окон)
const Z_BEGIN: f32 = -13.2; // стены начинаются за спиной
const N_SEC: usize = 92;
const END0: f32 = 38.0; // фонарь при старте
const FUNNEL: f32 = 15.0; // за сколько метров до конца стены сходятся
const SQUEEZE: f32 = 3.0; // последние метры — самое узкое место
const SHOOTER_DIST: f32 = 5.0;
const HIDE_Y: f32 = -60.0; // секции за фонарём уходят под землю
// ---------------------------------------------------------------- время, с
const FADE_IN: f32 = 2.0;
const IDLE_SHOT: f32 = 9.0;
const IDLE_HINT: f32 = 4.5;
const STOP_SHOT: f32 = 2.5;
const WALK_LIMIT: f32 = 30.0; // м шагом без бега — сон не отпускает
const CLOSE_RAMP: f32 = 2.5;
const SLOWMO: f32 = 0.32;
const SHOT_HOLD: f32 = 1.8;
const END_WHITE: f32 = 1.6;
const END_HOLD: f32 = 6.5;

const BRICK_NIGHT: Facade =
    Facade { style: facade::BRICK_DARK, floor_h: 3.1, win_spacing: 2.2, lit: 0.18, ground_h: 3.4 };
const LAMP_WARM: V3 = [1.0, 0.86, 0.62];

/// Секция стены: стена (двигается к середине) и предметы у неё (уходят в стену).
struct Section {
    side: f32,
    zc: f32,
    wall: Range<usize>,
    decor: Range<usize>,
    depth: f32,
    /// уже применённые сдвиги: стена по x, предметы по x, по y (скрытие)
    applied: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum St {
    Walk,
    Chase,
    Shot,
    End,
}

pub struct DreamAlley {
    st: St,
    t: f32,
    total: f32,
    end_z: f32,
    end_applied: f32,
    end_prims: Range<usize>,
    end_light: usize,
    shooter: Range<usize>,
    shooter_applied: f32,
    muzzle: usize,
    sections: Vec<Section>,
    last_z: f32,
    max_z: f32,
    walked: f32,
    idle: f32,
    close: f32,
    said: u32,
    attempt: u32,
}

/// Простой детерминированный генератор (одинаковый сон каждый раз).
struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
}

pub fn build(attempt: u32) -> (Scene, DreamAlley) {
    let mut b = Builder::new("Сон. Переулок");
    b.time_of_day(0.0);
    b.fog(0.032);
    b.spawn([0.0, 0.0, 0.0], 0.0);
    b.moon([0.1, 0.3, 1.0], 5.0);
    let mut rng = Rng(0x5eed_1957);

    // ------------------------------------------------------------ стены секциями
    let mut sections = Vec::new();
    for k in 0..N_SEC {
        let z0 = Z_BEGIN + k as f32 * SEC;
        for side in [-1.0f32, 1.0] {
            let xa = side * HW;
            let xb = side * (HW + 6.0);
            let (x0, x1) = (xa.min(xb), xa.max(xb));
            let h = 12.0 + rng.f() * 6.0;
            let mut f = BRICK_NIGHT;
            if rng.f() < 0.3 {
                f.ground_h = 0.0; // окна у самой земли — во сне так бывает
            }
            f.lit = 0.08 + rng.f() * 0.2;
            let m0 = b.mark();
            b.cube(
                Group::Solid,
                [x0, 0.0, z0],
                [x1, h, z0 + SEC],
                Mat { id: mat::FACADE, p: [f.floor_h, f.win_spacing, f.lit, f.ground_h], style: f.style },
            );
            // двери: обычная или «в никуда» на высоте второго этажа
            let r = rng.f();
            if r < 0.24 && z0 > -8.0 {
                let y0 = if r < 0.08 { 3.6 } else { 0.0 };
                let dz = z0 + 0.5;
                // ниша двери: режем с запасом в сторону переулка (см. дайнер в test_street)
                let (ca, cb) = (side * (HW - 1.0), side * (HW + 0.3));
                b.cube(
                    Group::Cut,
                    [ca.min(cb), y0, dz],
                    [ca.max(cb), y0 + 2.2, dz + 1.1],
                    Mat::rgb(mat::PLASTER, [0.2, 0.18, 0.16]),
                );
                let dx = side * (HW + 0.16);
                b.cube(
                    Group::Detail,
                    [dx - 0.03, y0, dz],
                    [dx + 0.03, y0 + 2.2, dz + 1.1],
                    Mat::rgb(mat::WOOD, [0.12, 0.08, 0.06]),
                );
            }
            let wall = b.since(m0);
            let m1 = b.mark();
            let mut depth: f32 = 0.0;
            let inner = side * HW;
            let toward = -side; // направление к середине переулка
            let span = |a: f32, d: f32| (a.min(a + toward * d), a.max(a + toward * d));
            if z0 > -10.0 {
                let r = rng.f();
                if r < 0.2 {
                    // мусорный бак с крышкой
                    let (a, c) = span(inner, 1.1);
                    let z = z0 + 0.3;
                    b.cube(Group::Detail, [a, 0.0, z], [c, 1.15, z + 1.6], Mat::rgb(mat::METAL, [0.06, 0.14, 0.09]));
                    b.cube(
                        Group::Detail,
                        [a - 0.03, 1.15, z - 0.03],
                        [c + 0.03, 1.22, z + 1.63],
                        Mat::rgb(mat::METAL, [0.04, 0.05, 0.05]),
                    );
                    depth = 1.15;
                } else if r < 0.42 {
                    // мешки с мусором
                    for j in 0..3 {
                        let d = 0.35 + rng.f() * 0.35;
                        let (a, c) = span(inner, d);
                        let z = z0 + 0.2 + j as f32 * 0.6;
                        b.rounded(
                            Group::Detail,
                            [a, 0.0, z],
                            [c, 0.45 + rng.f() * 0.2, z + 0.55],
                            0.16,
                            Mat::rgb(mat::VINYL, [0.02, 0.02, 0.025]),
                        );
                        depth = depth.max(d);
                    }
                }
                if rng.f() < 0.3 {
                    // водосточная труба по всей высоте
                    let x = inner + toward * 0.1;
                    b.cyl(Group::Detail, [x, 0.0, z0 + 1.9], 0.07, h, Mat::rgb(mat::METAL, [0.2, 0.2, 0.21]));
                    depth = depth.max(0.2);
                }
                if rng.f() < 0.2 {
                    // площадка пожарной лестницы над головой
                    let y = 3.4 + 3.1 * (rng.f() * 3.0).floor();
                    let (a, c) = span(inner, 1.0);
                    let m = Mat::rgb(mat::METAL, pal::FIRE_ESCAPE);
                    b.cube(Group::Detail, [a, y - 0.06, z0], [c, y, z0 + SEC], m);
                    let rx = inner + toward * 1.0;
                    b.cube(Group::Detail, [rx - 0.03, y + 0.95, z0], [rx + 0.03, y + 1.0, z0 + SEC], m);
                    for z in [z0, z0 + SEC * 0.5, z0 + SEC] {
                        b.cube(Group::Detail, [rx - 0.025, y, z - 0.025], [rx + 0.025, y + 1.0, z + 0.025], m);
                    }
                    depth = depth.max(1.0);
                } else if rng.f() < 0.18 {
                    // кондиционер под окном
                    let (a, c) = span(inner, 0.5);
                    let y = 4.0 + 3.1 * (rng.f() * 2.0).floor();
                    b.cube(
                        Group::Detail,
                        [a, y, z0 + 0.6],
                        [c, y + 0.45, z0 + 1.4],
                        Mat::rgb(mat::METAL, [0.45, 0.45, 0.43]),
                    );
                    depth = depth.max(0.5);
                }
            }
            let decor = b.since(m1);
            // габариты движения: стена к середине на (HW − HW_MIN), предметы ещё и в стену, всё — вниз
            let sx = |v: f32| if side > 0.0 { [-v, 0.0] } else { [0.0, v] };
            let w = sx(HW - HW_MIN);
            b.moving(wall.clone(), [w[0], HIDE_Y, 0.0], [w[1], 0.0, 0.0]);
            let dd = if side > 0.0 { [w[0], depth] } else { [-depth, w[1]] };
            b.moving(decor.clone(), [dd[0], HIDE_Y, 0.0], [dd[1], 0.0, 0.0]);
            sections.push(Section { side, zc: z0 + SEC * 0.5, wall, decor, depth, applied: [0.0; 3] });
        }
    }

    // ------------------------------------------------------------ позади: выход на улицу, свет
    b.cube(Group::Solid, [-12.0, 0.0, -24.0], [12.0, 9.0, -23.5], Mat::glow(pal::SODIUM, 0.22));
    b.light([0.0, 4.5, -17.0], pal::SODIUM, 16.0, 22.0);

    // тусклые лампочки над дверями по пути (уходят в стены вместе с ними)
    for (x, z) in [(-1.35, 9.0), (1.35, 23.0)] {
        b.light([x, 2.6, z], [1.0, 0.7, 0.4], 6.0, 2.5);
    }

    // ------------------------------------------------------------ фонарь в конце (движется вместе с концом)
    let m = b.mark();
    let lz = END0 + 2.0;
    let post = Mat::rgb(mat::METAL, pal::LAMP_POST);
    b.cyl(Group::Detail, [0.0, 0.0, lz], 0.09, 4.3, post);
    b.cube(Group::Detail, [-0.05, 4.25, lz - 0.05], [0.05, 4.35, lz + 0.9], post);
    b.cyl(Group::Detail, [0.0, 3.95, lz + 0.85], 0.3, 0.3, post);
    b.capsule(Group::Detail, [0.0, 3.92, lz + 0.85], [0.0, 3.92, lz + 0.85], 0.17, Mat::glow(LAMP_WARM, 16.0));
    // светящаяся дымка далеко за фонарём — «свет в конце»
    b.cube(Group::Detail, [-14.0, 0.0, lz + 26.0], [14.0, 22.0, lz + 26.5], Mat::glow(LAMP_WARM, 3.0));
    let end_prims = b.since(m);
    b.moving(end_prims.clone(), [0.0, 0.0, 0.0], [0.0, 0.0, (N_SEC as f32) * SEC]);
    b.no_collide(end_prims.clone());
    b.light([0.0, 3.6, lz + 0.85], LAMP_WARM, 26.0, 70.0);

    // ------------------------------------------------------------ стрелок
    let look = Look {
        female: false,
        skin: [0.30, 0.22, 0.18],
        hair: Hair::Hat,
        hair_color: [0.03, 0.03, 0.035],
        top: [0.05, 0.05, 0.06],
        pants: [0.04, 0.04, 0.05],
        shoes: [0.02, 0.02, 0.02],
    };
    let shooter = b.person([0.0, 0.0, -SHOOTER_DIST], 0.0, &look, Pose::Aim);
    let muzzle = b.mark();
    b.capsule(
        Group::Detail,
        [0.05, 1.47, -SHOOTER_DIST + 0.84],
        [0.05, 1.47, -SHOOTER_DIST + 0.84],
        0.0005,
        Mat::glow([1.0, 0.85, 0.5], 12.0),
    );
    let shooter = shooter.start..b.mark();
    b.moving(shooter.clone(), [0.0, 0.0, 0.0], [0.0, 0.0, (N_SEC as f32) * SEC]);
    b.no_collide(shooter.clone());

    let mut scene = b.finish();
    let end_light = scene.lights.len() - 1;
    scene.triggers.clear();
    let logic = DreamAlley {
        st: St::Walk,
        t: 0.0,
        total: 0.0,
        end_z: END0,
        end_applied: END0,
        end_prims,
        end_light,
        shooter,
        shooter_applied: -SHOOTER_DIST,
        muzzle,
        sections,
        last_z: 0.0,
        max_z: 0.0,
        walked: 0.0,
        idle: 0.0,
        close: 0.0,
        said: 0,
        attempt,
    };
    (scene, logic)
}

impl DreamAlley {
    /// Сказать реплику один раз (флаг — бит `id`).
    fn say(&mut self, ctx: &mut Ctx, id: u32, l: Line) {
        if self.said & (1 << id) == 0 {
            self.said |= 1 << id;
            ctx.subs.push(&[l]);
        }
    }

    fn goto(&mut self, st: St) {
        self.st = st;
        self.t = 0.0;
    }

    /// Расставить подвижную геометрию по текущему состоянию.
    fn place(&mut self, scene: &mut Scene) {
        // конец переулка с фонарём
        let dz = self.end_z - self.end_applied;
        if dz != 0.0 {
            for i in self.end_prims.clone() {
                scene.prims[i].translate([0.0, 0.0, dz]);
            }
            scene.lights[self.end_light].pos[2] += dz;
            self.end_applied = self.end_z;
        }
        // стрелок всегда в 5 м позади самой дальней точки, где был герой
        let sz = self.max_z - SHOOTER_DIST;
        let dz = sz - self.shooter_applied;
        if dz != 0.0 {
            for i in self.shooter.clone() {
                scene.prims[i].translate([0.0, 0.0, dz]);
            }
            self.shooter_applied = sz;
        }
        // стены сходятся к концу; предметы уходят в стены; секции за фонарём — под землю
        for s in &mut self.sections {
            let dist = self.end_z - s.zc;
            let f = smooth(FUNNEL, SQUEEZE, dist);
            let hw = (HW - (HW - HW_MIN) * self.close * f - 0.22 * self.close).max(HW_MIN);
            let sink = self.close * smooth(0.0, 0.45, f).max(self.close * 0.25);
            let wall_dx = s.side * (hw - HW);
            let decor_dx = wall_dx + s.side * s.depth * sink;
            let dy = if s.zc - SEC * 0.5 > self.end_z + 0.5 { HIDE_Y } else { 0.0 };
            let want = [wall_dx, decor_dx, dy];
            let d = [want[0] - s.applied[0], want[1] - s.applied[1], want[2] - s.applied[2]];
            if d != [0.0; 3] {
                for i in s.wall.clone() {
                    scene.prims[i].translate([d[0], d[2], 0.0]);
                }
                for i in s.decor.clone() {
                    scene.prims[i].translate([d[1], d[2], 0.0]);
                }
                s.applied = want;
            }
        }
    }
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn approach(v: &mut f32, target: f32, rate: f32, dt: f32) {
    *v += (target - *v) * (rate * dt).min(1.0);
}

impl Logic for DreamAlley {
    fn update(&mut self, ctx: &mut Ctx) -> Flow {
        let dt = ctx.dt;
        self.t += dt;
        self.total += dt;
        let pz = ctx.player.pos[2];
        let dz = pz - self.last_z;
        self.last_z = pz;
        let running = ctx.input.run && ctx.input.forward > 0.0;
        // плавный выход из чёрного
        if self.st != St::Shot {
            ctx.fx.fade = (1.0 - self.total / FADE_IN).max(0.0);
        }
        let (mut ts, mut vig, mut fov) = (1.0, 0.35, 1.0);

        // начальные реплики
        if self.total > 0.7 {
            if self.attempt > 1 {
                self.say(ctx, 0, line("", "Снова этот сон.", 2.4));
            }
            self.say(ctx, 1, line("", "Не оборачивайся.", 2.6));
        }
        // обернулся
        let fwd = ctx.player.forward();
        if self.total > 1.0 && fwd[2] < -0.6 {
            self.say(ctx, 2, line("", "Он всегда на одном расстоянии.", 3.0));
        }

        match self.st {
            St::Walk | St::Chase => {
                self.max_z = self.max_z.max(pz);
                // пятится к стрелку — выстрел
                if pz < self.max_z - (SHOOTER_DIST - 3.8) {
                    self.goto(St::Shot);
                }
            }
            _ => {}
        }

        match self.st {
            St::Walk => {
                if dz > 0.0 && !running {
                    // шагом фонарь не приближается
                    self.end_z += dz;
                    self.walked += dz;
                }
                if dz.abs() < 1e-4 {
                    self.idle += dt;
                } else {
                    self.idle = 0.0;
                }
                if self.idle > IDLE_HINT {
                    self.say(ctx, 3, line("", "Беги.", 2.0));
                }
                if running && dz > 0.0 {
                    self.idle = 0.0;
                    self.goto(St::Chase);
                } else if self.idle > IDLE_SHOT || self.walked > WALK_LIMIT {
                    self.goto(St::Shot);
                }
            }
            St::Chase => {
                self.close = (self.t / CLOSE_RAMP).min(1.0);
                if dz < 0.002 {
                    self.idle += dt;
                } else {
                    self.idle = 0.0;
                }
                let dist = self.end_z - pz;
                if dist < FUNNEL {
                    self.say(ctx, 4, line("", "Только не останавливайся.", 2.4));
                }
                if dist < SQUEEZE + 1.5 {
                    // впритык: время тянется
                    let k = smooth(SQUEEZE + 1.5, SQUEEZE * 0.5, dist);
                    ts = 1.0 - (1.0 - SLOWMO) * k;
                    vig = 0.35 + 0.45 * k;
                    fov = 1.0 - 0.12 * k;
                    self.say(ctx, 5, line("", "Ещё чуть-чуть...", 2.2));
                }
                if dist < 0.8 {
                    self.goto(St::End);
                } else if self.idle > STOP_SHOT {
                    self.goto(St::Shot);
                }
            }
            St::Shot => {
                let on = self.t < 0.12;
                let r = if on { 0.14 } else { 0.0005 };
                ctx.scene.prims[self.muzzle].a[3] = r;
                ctx.fx.flash = if on { 1.4 } else { 0.0 };
                if self.t > 0.12 {
                    ctx.fx.fade = 1.0;
                }
                if self.t > SHOT_HOLD {
                    return Flow::Restart;
                }
            }
            St::End => {
                ctx.fx.white = smooth(0.0, END_WHITE, self.t);
                vig = 0.0;
                if self.t > END_WHITE + 0.3 {
                    self.say(ctx, 6, line("", "Я проснулся.", 3.0));
                }
                if self.t > END_HOLD {
                    return Flow::Restart;
                }
            }
        }
        let fx = &mut *ctx.fx;
        approach(&mut fx.time_scale, ts, 4.0, dt);
        approach(&mut fx.vignette, vig, 3.0, dt);
        approach(&mut fx.fov_mul, fov, 3.0, dt);
        self.place(ctx.scene);
        Flow::Continue
    }

    fn state(&self) -> String {
        format!("{:?} {:.1}с | до фонаря {:.1} м | сжатие {:.2}", self.st, self.t, self.end_z - self.last_z, self.close)
    }
}
