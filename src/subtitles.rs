//! Кинематографичные субтитры: очередь реплик, тайминг, плавное появление.
//! Рисуются в отдельный слой поверх сцены своим шрифтом (не зависит от плотности сцены).

use crate::config::Config;
use crate::grid::{Grid, Rgba8};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct Line {
    /// Имя говорящего (None — закадровый текст).
    pub speaker: Option<&'static str>,
    pub text: &'static str,
    /// Сколько секунд реплика на экране.
    pub secs: f32,
}

pub fn line(speaker: &'static str, text: &'static str, secs: f32) -> Line {
    Line { speaker: if speaker.is_empty() { None } else { Some(speaker) }, text, secs }
}

#[derive(Default)]
pub struct Subtitles {
    queue: VecDeque<Line>,
    current: Option<(Line, f32)>,
}

impl Subtitles {
    pub fn push(&mut self, lines: &[Line]) {
        self.queue.extend(lines.iter().cloned());
    }

    pub fn update(&mut self, dt: f32, gap: f32) {
        if let Some((l, t)) = &mut self.current {
            *t += dt;
            if *t >= l.secs + gap {
                self.current = None;
            }
        }
        if self.current.is_none()
            && let Some(l) = self.queue.pop_front()
        {
            self.current = Some((l, 0.0));
        }
    }

    /// Нарисовать текущую реплику внизу по центру.
    pub fn draw(&self, g: &mut Grid, cfg: &Config) {
        let Some((l, t)) = &self.current else { return };
        let fade = cfg.subtitle_fade_s;
        let a = (t / fade).min(1.0).min(((l.secs - t) / fade).max(0.0)).clamp(0.0, 1.0);
        if a <= 0.0 {
            return;
        }
        let width = ((g.cols as f32 * cfg.subtitle_width) as usize).max(10);
        let prefix = l.speaker.map(|s| format!("{s}: ")).unwrap_or_default();
        let lines = wrap(&format!("{prefix}{}", l.text), width);
        let bottom = g.rows as i32 - cfg.subtitle_bottom_rows as i32;
        let y0 = bottom - lines.len() as i32;
        let alpha = |k: f32| (k * a * 255.0) as u8;
        let fg = rgba(cfg.subtitle_fg, alpha(1.0));
        let name = rgba(cfg.subtitle_speaker, alpha(1.0));
        let plate: Rgba8 = [0, 0, 0, alpha(cfg.subtitle_plate_alpha)];
        for (k, s) in lines.iter().enumerate() {
            let n = s.chars().count() as i32;
            let x = (g.cols as i32 - n) / 2;
            let y = y0 + k as i32;
            g.fill_rect(x - 1, y, n + 2, 1, plate);
            g.text(x, y, s, fg);
            if k == 0 && !prefix.is_empty() {
                g.text(x, y, prefix.trim_end(), name);
            }
        }
    }
}

fn rgba(c: [f32; 3], a: u8) -> Rgba8 {
    let [r, g, b] = crate::grid::to_u8(c);
    [r, g, b, a]
}

/// Перенос по словам.
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for w in s.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + w.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_words() {
        assert_eq!(wrap("один два три четыре", 9), vec!["один два", "три", "четыре"]);
    }

    #[test]
    fn queue_plays_in_order() {
        let mut s = Subtitles::default();
        s.push(&[line("А", "раз", 1.0), line("", "два", 1.0)]);
        s.update(0.0, 0.2);
        assert_eq!(s.current.as_ref().unwrap().0.text, "раз");
        s.update(1.3, 0.2);
        s.update(0.0, 0.2);
        assert_eq!(s.current.as_ref().unwrap().0.text, "два");
    }
}
