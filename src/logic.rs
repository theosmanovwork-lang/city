//! Сценарий локации: каждая сцена может иметь свою логику (состояния, события,
//! движущуюся геометрию) и управлять эффектами кадра.

use crate::player::{Input, Player};
use crate::sdf::Scene;
use crate::subtitles::Subtitles;

/// Эффекты кадра, которыми управляет сценарий. Применяются на GPU после ray marching.
#[derive(Clone, Copy, Debug)]
pub struct Fx {
    /// Затемнение в чёрное 0..1.
    pub fade: f32,
    /// Засветка в белое 0..1.
    pub white: f32,
    /// Виньетка (затемнение краёв) 0..1.
    pub vignette: f32,
    /// Вспышка (выстрел) — добавка яркости.
    pub flash: f32,
    /// Множитель хода времени (замедление).
    pub time_scale: f32,
    /// Множитель угла обзора.
    pub fov_mul: f32,
}

impl Default for Fx {
    fn default() -> Fx {
        Fx { fade: 0.0, white: 0.0, vignette: 0.0, flash: 0.0, time_scale: 1.0, fov_mul: 1.0 }
    }
}

/// Что делать после шага сценария.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    /// Начать локацию заново (перезагрузить сцену).
    Restart,
}

pub struct Ctx<'a> {
    pub scene: &'a mut Scene,
    pub player: &'a mut Player,
    pub subs: &'a mut Subtitles,
    pub fx: &'a mut Fx,
    pub input: Input,
    /// Реальный шаг времени (без замедления), с.
    pub dt: f32,
}

pub trait Logic {
    fn update(&mut self, ctx: &mut Ctx) -> Flow;
    /// Название текущего состояния (для HUD).
    fn state(&self) -> String {
        String::new()
    }
}

/// Сцена без сценария (только зоны-триггеры).
pub struct NoLogic;

impl Logic for NoLogic {
    fn update(&mut self, _ctx: &mut Ctx) -> Flow {
        Flow::Continue
    }
}
