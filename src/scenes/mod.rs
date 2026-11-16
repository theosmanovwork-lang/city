//! Сцены игры. Каждая сцена описана кодом (без 3D-моделей).

pub mod dream_alley;
pub mod test_street;

use crate::logic::{Logic, NoLogic};
use crate::sdf::Scene;

/// Названия сцен для `--scene`.
pub const NAMES: [&str; 2] = ["alley", "street"];

/// Загрузить сцену по имени. `attempt` — номер попытки (с 1), сценарий может его учитывать.
pub fn load(name: &str, attempt: u32) -> (Scene, Box<dyn Logic>) {
    match name {
        "street" => (test_street::build(), Box::new(NoLogic)),
        _ => {
            let (s, l) = dream_alley::build(attempt);
            (s, Box::new(l))
        }
    }
}
