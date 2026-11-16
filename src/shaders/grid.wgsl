// Вывод сетки символов. Один полноэкранный треугольник, без сэмплеров:
// все чтения через textureLoad, поэтому фильтрации нет физически.
//
// Пиксель → ячейка → (глиф, fg, bg) → маска глифа из атласа → mix(bg, fg, mask).
// Глиф увеличивается целочисленно (scale), зазор — правая колонка и нижняя строка ячейки.

struct Params {
    origin: vec2<i32>,     // левый верхний угол сетки в физических пикселях цели
    grid: vec2<u32>,       // колонки, строки
    cell: vec2<u32>,       // размер ячейки с зазором, px
    glyph: vec2<u32>,      // размер глифа без масштаба, px
    scale: u32,            // целочисленное увеличение глифа
    atlas_cols: u32,       // глифов в ряду атласа
    atlas_y: u32,          // смещение шрифта в атласе по Y
    gap_on: u32,           // 1 — рисовать зазор цветом gap_color, 0 — фоном ячейки
    gap_color: vec4<f32>,
    bg_gain: f32,          // bgFill для фона-подсветки (альфа фона = 0)
    srgb_out: u32,         // 1 — цель sRGB: переводим наши значения в линейные
    overlay: u32,          // 1 — слой поверх сцены: альфа fg — символ, альфа bg — плашка
    _pad1: u32,
};

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var t_glyph: texture_2d<u32>;
@group(0) @binding(2) var t_fg: texture_2d<f32>;
@group(0) @binding(3) var t_bg: texture_2d<f32>;
@group(0) @binding(4) var t_atlas: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // (-1,-1), (3,-1), (-1,3) — покрывает весь экран
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn out_rgba(c: vec3<f32>, a: f32) -> vec4<f32> {
    if (P.srgb_out == 1u) {
        return vec4<f32>(to_linear(c), a);
    }
    return vec4<f32>(c, a);
}

fn out_color(c: vec3<f32>) -> vec4<f32> {
    return out_rgba(c, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(pos.xy)) - P.origin;
    let ov = P.overlay == 1u;
    if (p.x < 0 || p.y < 0) {
        return out_rgba(vec3<f32>(0.0), select(1.0, 0.0, ov));
    }
    let pu = vec2<u32>(p);
    let c = pu / P.cell;
    if (c.x >= P.grid.x || c.y >= P.grid.y) {
        return out_rgba(vec3<f32>(0.0), select(1.0, 0.0, ov));
    }
    let ci = vec2<i32>(c);
    let bgt = textureLoad(t_bg, ci, 0);
    if (ov) {
        // оверлей: плашка с непрозрачностью bg.a, символ с непрозрачностью fg.a
        let inc_o = pu - c * P.cell;
        let gsz_o = P.glyph * P.scale;
        if (inc_o.x >= gsz_o.x || inc_o.y >= gsz_o.y) {
            return out_rgba(bgt.rgb, bgt.a);
        }
        let g_o = textureLoad(t_glyph, ci, 0).r;
        let fgt = textureLoad(t_fg, ci, 0);
        let slot_o = vec2<u32>(g_o % P.atlas_cols, g_o / P.atlas_cols);
        let m_o = textureLoad(t_atlas, vec2<i32>(slot_o * P.glyph + inc_o / P.scale + vec2<u32>(0u, P.atlas_y)), 0).r;
        if (m_o > 0.5 && g_o != 0u) {
            return out_rgba(fgt.rgb, fgt.a);
        }
        return out_rgba(bgt.rgb, bgt.a);
    }
    // альфа фона: 0 — подсветка (× bg_gain), 1 — сплошная плашка
    let bg = bgt.rgb * mix(P.bg_gain, 1.0, bgt.a);

    let inc = pu - c * P.cell;             // пиксель внутри ячейки
    let gsz = P.glyph * P.scale;
    if (inc.x >= gsz.x || inc.y >= gsz.y) { // зазор
        if (P.gap_on == 1u) {
            return out_color(P.gap_color.rgb);
        }
        return out_color(bg);
    }
    let g = textureLoad(t_glyph, ci, 0).r;
    let fg = textureLoad(t_fg, ci, 0).rgb;
    let slot = vec2<u32>(g % P.atlas_cols, g / P.atlas_cols);
    let ap = slot * P.glyph + inc / P.scale + vec2<u32>(0u, P.atlas_y);
    let m = textureLoad(t_atlas, vec2<i32>(ap), 0).r;
    return out_color(mix(bg, fg, m));
}
