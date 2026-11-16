//! Игрок от первого лица: реальный масштаб и скорость, коллизии по SDF сцены,
//! подъём на ступени, падение, покачивание камеры при ходьбе.

use crate::config::Config;
use crate::sdf::{Scene, V3};

#[derive(Default, Clone, Copy)]
pub struct Input {
    pub forward: f32, // -1..1
    pub strafe: f32,  // -1..1
    pub run: bool,
}

pub struct Player {
    /// Ноги.
    pub pos: V3,
    pub yaw: f32,
    pub pitch: f32,
    vel_y: f32,
    bob_phase: f32,
    bob_amp: f32,
}

/// Сфера тела на высоте `h` над ногами.
const BODY: [f32; 3] = [0.55, 1.05, 1.55];

impl Player {
    pub fn new(pos: V3, yaw: f32) -> Player {
        Player { pos, yaw, pitch: 0.0, vel_y: 0.0, bob_phase: 0.0, bob_amp: 0.0 }
    }

    pub fn forward(&self) -> V3 {
        let (s, c) = self.yaw.sin_cos();
        let (ps, pc) = self.pitch.sin_cos();
        [s * pc, ps, c * pc]
    }

    /// Глаза с покачиванием.
    pub fn eye(&self, cfg: &Config) -> V3 {
        let (s, c) = self.yaw.sin_cos();
        let up = (self.bob_phase * 2.0).sin() * cfg.bob_height * self.bob_amp;
        let side = self.bob_phase.sin() * cfg.bob_side * self.bob_amp;
        [self.pos[0] + c * side, self.pos[1] + cfg.eye_height + up, self.pos[2] - s * side]
    }

    pub fn look(&mut self, dx: f32, dy: f32, cfg: &Config) {
        self.yaw += dx * cfg.mouse_sens;
        let lim = cfg.max_pitch_deg.to_radians();
        self.pitch = (self.pitch - dy * cfg.mouse_sens).clamp(-lim, lim);
    }

    pub fn update(&mut self, dt: f32, input: Input, scene: &Scene, cfg: &Config) {
        let (s, c) = self.yaw.sin_cos();
        let (fwd, right) = ([s, c], [c, -s]);
        let mut mx = fwd[0] * input.forward + right[0] * input.strafe;
        let mut mz = fwd[1] * input.forward + right[1] * input.strafe;
        let len = (mx * mx + mz * mz).sqrt();
        if len > 1.0 {
            mx /= len;
            mz /= len;
        }
        let speed = if input.run { cfg.run_speed } else { cfg.walk_speed };
        let on_ground = self.vel_y == 0.0;
        let (dx, dz) = (mx * speed * dt, mz * speed * dt);

        // горизонтальный шаг: подъём на ступень, затем выталкивание из стен
        let mut p = [self.pos[0] + dx, self.pos[1], self.pos[2] + dz];
        if let Some(fy) = floor_below(scene, p, cfg.step_height)
            && fy > p[1]
            && fy - p[1] <= cfg.step_height
        {
            p[1] = fy;
        }
        for _ in 0..cfg.collision_iters {
            let mut moved = false;
            for h in BODY {
                let c = [p[0], p[1] + h, p[2]];
                let d = scene.sdf(c) - cfg.body_radius;
                if d < 0.0 {
                    let n = scene.normal(c);
                    let l = (n[0] * n[0] + n[2] * n[2]).sqrt();
                    if l > 1e-3 {
                        p[0] -= n[0] / l * d;
                        p[2] -= n[2] / l * d;
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        // если после выталкивания всё ещё внутри — шаг отменяется
        if BODY.iter().any(|&h| scene.sdf([p[0], p[1] + h, p[2]]) < cfg.body_radius * 0.5) {
            p = self.pos;
        }

        // вертикаль: стоим на полу или падаем
        let floor = floor_below(scene, p, cfg.step_height).unwrap_or(p[1]);
        if p[1] - floor > 0.02 {
            self.vel_y -= cfg.gravity * dt;
            p[1] = (p[1] + self.vel_y * dt).max(floor);
            if p[1] <= floor {
                self.vel_y = 0.0;
            }
        } else {
            p[1] = floor;
            self.vel_y = 0.0;
        }

        // покачивание: фаза идёт по пройденному пути
        let moved = ((p[0] - self.pos[0]).powi(2) + (p[2] - self.pos[2]).powi(2)).sqrt();
        if on_ground {
            self.bob_phase += moved / cfg.stride * std::f32::consts::PI;
        }
        let target = if cfg.head_bob && moved > 1e-4 { if input.run { 1.6 } else { 1.0 } } else { 0.0 };
        self.bob_amp += (target - self.bob_amp) * (dt * 6.0).min(1.0);
        self.pos = p;
    }
}

/// Высота пола под точкой: луч вниз от `p.y + step` (None — под ногами пусто глубже 50 м).
fn floor_below(scene: &Scene, p: V3, step: f32) -> Option<f32> {
    let mut y = p[1] + step + 0.05;
    if scene.sdf([p[0], y, p[2]]) < 0.0 {
        return Some(p[1]); // над головой ступени — твёрдое: не поднимаемся
    }
    for _ in 0..64 {
        let d = scene.sdf([p[0], y, p[2]]);
        if d < 0.002 {
            return Some(y);
        }
        y -= d.max(0.002);
        if y < p[1] - 50.0 {
            return None;
        }
    }
    Some(y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::test_street;

    /// Пройти `secs` секунд с вводом `input` из точки `pos` с направлением `yaw_deg`.
    fn walk(pos: V3, yaw_deg: f32, input: Input, secs: f32) -> Player {
        let scene = test_street::build();
        let cfg = Config::default();
        let mut p = Player::new(pos, yaw_deg.to_radians());
        let dt = 1.0 / 60.0;
        for _ in 0..(secs / dt) as u32 {
            p.update(dt, input, &scene, &cfg);
        }
        p
    }
    const FWD: Input = Input { forward: 1.0, strafe: 0.0, run: false };

    #[test]
    fn walks_at_real_speed() {
        let p = walk([-7.6, 0.15, 0.0], 0.0, FWD, 2.0);
        assert!((p.pos[2] - 2.8).abs() < 0.1, "за 2 с шагом ~2.8 м, а прошли {}", p.pos[2]);
        assert!((p.pos[1] - 0.15).abs() < 0.01, "идём по тротуару");
        let r = walk([-7.6, 0.15, 0.0], 0.0, Input { run: true, ..FWD }, 2.0);
        assert!((r.pos[2] - 7.0).abs() < 0.2, "бег ~3.5 м/с, а прошли {}", r.pos[2]);
    }

    #[test]
    fn walls_block() {
        let p = walk([-8.0, 0.15, 1.0], -90.0, FWD, 3.0);
        assert!(p.pos[0] > -10.0 + 0.25 && p.pos[0] < -9.5, "упёрлись в фасад: x = {}", p.pos[0]);
    }

    #[test]
    fn climbs_curb_from_road() {
        let p = walk([-4.0, 0.0, 3.0], -90.0, FWD, 2.0);
        assert!(p.pos[0] < -6.2, "вышли на тротуар: x = {}", p.pos[0]);
        assert!((p.pos[1] - 0.15).abs() < 0.01, "поднялись на бордюр: y = {}", p.pos[1]);
    }

    #[test]
    fn enters_diner_through_door() {
        let p = walk([8.0, 0.15, 22.65], 90.0, FWD, 3.0);
        assert!(p.pos[0] > 11.0, "вошли в дайнер: x = {}", p.pos[0]);
        assert!((p.pos[1] - 0.15).abs() < 0.02, "стоим на полу зала: y = {}", p.pos[1]);
    }

    #[test]
    fn shop_window_glass_blocks() {
        let p = walk([8.0, 0.15, 18.0], 90.0, FWD, 3.0);
        assert!(p.pos[0] < 10.0, "стекло витрины не пускает: x = {}", p.pos[0]);
    }
}
