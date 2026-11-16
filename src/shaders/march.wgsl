// Ray marching сцены из SDF-форм на каждую ячейку сетки символов.
// Луч → поверхность → материал → свет (солнце/луна с мягкой тенью, небо с учётом
// закрытости, точечные источники с тенью, AO) → туман → экспозиция →
// правило стиля (яркость → символ рампы, оттенок → цвет символа, фон → подсветка).

struct Prim { a: vec4f, b: vec4f, m: vec4f, k: vec4u };
struct PLight { pos: vec4f, col: vec4f };

struct Frame {
    cam_pos: vec4f,    // w: время, с
    cam_fwd: vec4f,    // w: tan(fov_x / 2)
    cam_right: vec4f,  // w: tan(fov_y / 2)
    cam_up: vec4f,     // w: дальность прорисовки
    grid: vec4u,       // колонки, строки, лучей на ячейку по стороне, число источников
    sun_dir: vec4f,    // w: сила прямого света
    sun_col: vec4f,    // w: ночь 0..1
    sky_top: vec4f,    // w: экспозиция
    sky_hz: vec4f,     // w: плотность тумана
    ambient: vec4f,    // w: доля горящих окон
    style: vec4f,      // порог чёрного, усиление рампы, макс. усиление цвета символа, уровень чёрного
    ramp_info: vec4u,  // длина рампы, глиф звезды, глиф яркой звезды, макс. шагов луча
    ramp: array<vec4u, 8>,
    accel_o: vec4f,    // начало сетки x, z; размер ячейки; верх сцены
    accel_n: vec4u,    // nx, nz, начало и число дальних форм
    lights: array<PLight, 16>,
};

@group(0) @binding(0) var<uniform> F: Frame;
@group(0) @binding(1) var<storage, read> prims: array<Prim>;
@group(0) @binding(2) var<storage, read> ranges: array<u32>;
@group(0) @binding(3) var<storage, read> list: array<u32>;
@group(0) @binding(4) var out_glyph: texture_storage_2d<r32uint, write>;
@group(0) @binding(5) var out_fg: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(6) var out_bg: texture_storage_2d<rgba8unorm, write>;

const GROUND: u32 = 0xffffffffu;
const MISS: u32 = 0xfffffffeu;
const BIG: f32 = 1e9;
// доля окружающего света, приходящая отражённой от соседних поверхностей
const BOUNCE: f32 = 0.3;

// материалы (совпадают с sdf.rs)
const M_ASPHALT: u32 = 0u;
const M_SIDEWALK: u32 = 1u;
const M_CONCRETE: u32 = 2u;
const M_FACADE: u32 = 3u;
const M_PLASTER: u32 = 4u;
const M_TILE: u32 = 5u;
const M_METAL: u32 = 6u;
const M_CHROME: u32 = 7u;
const M_VINYL: u32 = 8u;
const M_AWNING: u32 = 9u;
const M_EMISSIVE: u32 = 10u;
const M_FORMICA: u32 = 11u;
const M_WOOD: u32 = 12u;
const M_PAINT: u32 = 13u;
const M_SIGN: u32 = 14u;

// ---------------------------------------------------------------- утилиты
fn hash3(a: i32, b: i32, c: i32) -> f32 {
    var h = (a * 374761393) ^ (b * 668265263) ^ (c * 1274126177);
    h = (h ^ i32(u32(h) >> 13u)) * 1274126177;
    h = h ^ i32(u32(h) >> 16u);
    return f32(u32(h)) / 4294967296.0;
}

fn lum(c: vec3f) -> f32 { return dot(c, vec3f(0.3, 0.59, 0.11)); }

// ---------------------------------------------------------------- SDF
fn sd(i: u32, p: vec3f) -> f32 {
    let pr = prims[i];
    let q = p - pr.a.xyz;
    if ((pr.k.x & 0xffu) == 1u) {
        let d = vec2f(length(q.xz) - pr.b.x, abs(q.y) - pr.b.y);
        return min(max(d.x, d.y), 0.0) + length(max(d, vec2f(0.0)));
    }
    let r = pr.a.w;
    let d = abs(q) - pr.b.xyz + vec3f(r);
    return length(max(d, vec3f(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0) - r;
}

struct H { d: f32, id: u32 };

fn cell_of(p: vec3f) -> i32 {
    let c = floor((p.xz - F.accel_o.xy) / F.accel_o.z);
    if (c.x < 0.0 || c.y < 0.0 || c.x >= f32(F.accel_n.x) || c.y >= f32(F.accel_n.y)) { return -1; }
    return i32(c.y) * i32(F.accel_n.x) + i32(c.x);
}

fn map(p: vec3f, ci: i32) -> H {
    var hs = H(p.y, GROUND);
    for (var j = 0u; j < F.accel_n.w; j++) {
        let i = list[F.accel_n.z + j];
        let d = sd(i, p);
        if (d < hs.d) { hs = H(d, i); }
    }
    if (ci < 0) { return hs; }
    let base = u32(ci) * 6u;
    var o = ranges[base];
    var n = ranges[base + 1u];
    for (var j = 0u; j < n; j++) {
        let i = list[o + j];
        let d = sd(i, p);
        if (d < hs.d) { hs = H(d, i); }
    }
    o = ranges[base + 2u];
    n = ranges[base + 3u];
    var hc = H(BIG, 0u);
    for (var j = 0u; j < n; j++) {
        let i = list[o + j];
        let d = sd(i, p);
        if (d < hc.d) { hc = H(d, i); }
    }
    if (-hc.d > hs.d) { hs = H(-hc.d, hc.id); }
    o = ranges[base + 4u];
    n = ranges[base + 5u];
    for (var j = 0u; j < n; j++) {
        let i = list[o + j];
        let d = sd(i, p);
        if (d < hs.d) { hs = H(d, i); }
    }
    return hs;
}

// расстояние вдоль луча до выхода из ячейки сетки
fn cell_exit(p: vec3f, rd: vec3f) -> f32 {
    let cs = F.accel_o.z;
    let c = floor((p.xz - F.accel_o.xy) / cs);
    let lo = F.accel_o.xy + c * cs;
    let hi = lo + vec2f(cs);
    var t = BIG;
    if (rd.x > 1e-6) { t = min(t, (hi.x - p.x) / rd.x); } else if (rd.x < -1e-6) { t = min(t, (lo.x - p.x) / rd.x); }
    if (rd.z > 1e-6) { t = min(t, (hi.y - p.z) / rd.z); } else if (rd.z < -1e-6) { t = min(t, (lo.y - p.z) / rd.z); }
    return max(t, 0.0);
}

fn step_len(p: vec3f, rd: vec3f, d: f32, ci: i32) -> f32 {
    if (ci < 0) { return d; }
    return min(d, cell_exit(p, rd) + 0.01);
}

struct Ray { t: f32, id: u32 };

fn march(ro: vec3f, rd: vec3f, maxd: f32) -> Ray {
    var t = 0.0;
    for (var i = 0u; i < F.ramp_info.w; i++) {
        let p = ro + rd * t;
        if (p.y > F.accel_o.w && rd.y >= 0.0) { break; }
        let ci = cell_of(p);
        let h = map(p, ci);
        if (h.d < 0.0006 * t + 0.002) { return Ray(t, h.id); }
        t += step_len(p, rd, h.d, ci);
        if (t > maxd) { break; }
    }
    return Ray(t, MISS);
}

// мягкая тень / видимость: 1 — свободно, 0 — закрыто
fn soft_vis(ro: vec3f, rd: vec3f, maxt: f32, k: f32) -> f32 {
    var res = 1.0;
    var t = 0.04;
    for (var i = 0; i < 40; i++) {
        let p = ro + rd * t;
        if (p.y > F.accel_o.w && rd.y >= 0.0) { break; }
        let ci = cell_of(p);
        let h = map(p, ci).d;
        res = min(res, k * h / t);
        if (res < 0.02) { return 0.0; }
        t += clamp(step_len(p, rd, h, ci), 0.03, 4.0);
        if (t > maxt) { break; }
    }
    return clamp(res, 0.0, 1.0);
}

fn normal_at(p: vec3f, eps: f32) -> vec3f {
    let ci = cell_of(p);
    let k = vec2f(1.0, -1.0);
    return normalize(
        k.xyy * map(p + k.xyy * eps, ci).d + k.yyx * map(p + k.yyx * eps, ci).d +
        k.yxy * map(p + k.yxy * eps, ci).d + k.xxx * map(p + k.xxx * eps, ci).d);
}

fn ao_at(p: vec3f, n: vec3f) -> f32 {
    let ci = cell_of(p);
    var occ = 0.0;
    var w = 1.0;
    for (var i = 1; i <= 5; i++) {
        let h = 0.06 * f32(i);
        occ += (h - map(p + n * h, ci).d) * w;
        w *= 0.7;
    }
    return clamp(1.0 - occ * 2.2, 0.0, 1.0);
}

// ---------------------------------------------------------------- небо
fn sky(rd: vec3f) -> vec3f {
    let s = pow(clamp(rd.y * 1.4, 0.0, 1.0), 0.55);
    var c = mix(F.sky_hz.rgb, F.sky_top.rgb, s);
    // солнце и ореол (только днём: ночью sun_dir — луна, рисуем диск луны тусклее)
    let cs = dot(rd, F.sun_dir.xyz);
    let night = F.sun_col.w;
    if (cs > 0.0) {
        c += F.sun_col.rgb * (pow(cs, 300.0) * 0.6 + pow(cs, 12.0) * 0.08) * (1.0 - night);
        if (cs > 0.99985) { c += F.sun_col.rgb * mix(2.5, 1.2, night); }
    }
    return c;
}

// ---------------------------------------------------------------- материалы
struct Surf { alb: vec3f, emit: vec3f, gloss: f32 };

fn facade_color(style: u32) -> vec3f {
    switch style {
        case 0u: { return vec3f(0.42, 0.20, 0.14); }  // кирпич
        case 1u: { return vec3f(0.60, 0.56, 0.48); }  // известняк
        case 2u: { return vec3f(0.46, 0.46, 0.45); }  // бетон офиса
        case 3u: { return vec3f(0.14, 0.18, 0.22); }  // стекло (импосты)
        default: { return vec3f(0.30, 0.15, 0.11); }  // тёмный кирпич
    }
}

fn surface(p: vec3f, n: vec3f, rd: vec3f, id: u32) -> Surf {
    var s = Surf(vec3f(0.2), vec3f(0.0), 0.0);
    if (id == GROUND) {
        let h = hash3(i32(floor(p.x * 2.0)), i32(floor(p.z * 2.0)), 7);
        s.alb = vec3f(0.13, 0.13, 0.14) * (0.92 + 0.16 * h);
        s.gloss = 0.03;
        return s;
    }
    let pr = prims[id];
    let mid = pr.k.y;
    let hx = select(pr.b.xyz, vec3f(pr.b.x, pr.b.y, pr.b.x), (pr.k.x & 0xffu) == 1u);
    let lo = pr.a.xyz - hx;
    let l = p - lo;
    let side = abs(n.y) < 0.5;
    let u = select(l.x, l.z, abs(n.x) > 0.5);
    switch mid {
        case M_SIDEWALK: {
            let seam = fract(p.x / 1.5) < 0.025 || fract(p.z / 1.5) < 0.025;
            s.alb = select(vec3f(0.36, 0.35, 0.33), vec3f(0.22, 0.21, 0.2), seam);
        }
        case M_CONCRETE: { s.alb = vec3f(0.42, 0.41, 0.39); }
        case M_PLASTER, M_METAL, M_VINYL, M_FORMICA, M_PAINT: {
            s.alb = pr.m.rgb;
            if (mid == M_METAL) { s.gloss = 0.25; }
            if (mid == M_VINYL) { s.gloss = 0.35; }
            if (mid == M_FORMICA) { s.gloss = 0.3; }
        }
        case M_TILE: {
            let c = (i32(floor(p.x / 0.3)) + i32(floor(p.z / 0.3))) & 1;
            s.alb = select(vec3f(0.06), vec3f(0.7, 0.68, 0.64), c == 0);
            s.gloss = 0.4;
        }
        case M_CHROME: { s.alb = vec3f(0.35); s.gloss = 0.9; }
        case M_AWNING: {
            let st = fract(u / 0.45) < 0.5;
            s.alb = select(vec3f(0.75, 0.70, 0.60), pr.m.rgb, st);
        }
        case M_EMISSIVE: { s.alb = vec3f(0.0); s.emit = pr.m.rgb * pr.m.w; }
        case M_WOOD: {
            let g = 0.85 + 0.3 * hash3(i32(floor(l.y * 40.0)), i32(floor(u * 3.0)), 3);
            s.alb = pr.m.rgb * g;
        }
        case M_SIGN: {
            // световой короб: светлая панель с тёмной рамкой
            let ext = hx * 2.0;
            let v = l.y / ext.y;
            let w = u / select(ext.x, ext.z, abs(n.x) > 0.5);
            let border = v < 0.12 || v > 0.88 || w < 0.03 || w > 0.97;
            if (border) { s.alb = vec3f(0.08); } else { s.emit = pr.m.rgb * pr.m.w; s.alb = vec3f(0.0); }
        }
        case M_FACADE: { s = facade(p, n, rd, pr, l, u, side); }
        default: { s.alb = vec3f(0.3); }
    }
    return s;
}

fn facade(p: vec3f, n: vec3f, rd: vec3f, pr: Prim, l: vec3f, u: f32, side: bool) -> Surf {
    var s = Surf(facade_color(pr.k.z), vec3f(0.0), 0.0);
    let style = pr.k.z;
    let seed = i32(pr.k.w);
    if (!side) {
        // крыша — гудрон; низ — тёмный
        s.alb = select(vec3f(0.05), vec3f(0.11, 0.11, 0.12), n.y > 0.0);
        return s;
    }
    let floor_h = pr.m.x;
    let spacing = pr.m.y;
    let lit_k = pr.m.z;
    let ground_h = pr.m.w;
    let brick = style == 0u || style == 4u;
    if (brick) {
        // кирпичная кладка: ряды 7.5 см, ложок 22 см, перевязка
        let row = floor(l.y / 0.075);
        let bu = fract(u / 0.22 + 0.5 * (row - 2.0 * floor(row * 0.5)));
        let mortar = fract(l.y / 0.075) < 0.18 || bu < 0.06;
        let v = 0.8 + 0.35 * hash3(i32(row), i32(floor(u / 0.22)), seed);
        s.alb = select(s.alb * v, vec3f(0.30, 0.28, 0.26), mortar);
    }
    if (l.y < ground_h) {
        // первый этаж: цоколь
        if (l.y < 0.6) { s.alb = vec3f(0.28, 0.27, 0.26); }
        return s;
    }
    let fz = (l.y - ground_h) / floor_h;
    let fi = floor(fz);
    let ffz = fz - fi;
    let fx = u / spacing;
    let ci = floor(fx);
    let ffx = fx - ci;
    var wx0 = 0.28; var wx1 = 0.72; var wz0 = 0.22; var wz1 = 0.86;
    if (style == 2u) { wx0 = 0.12; wx1 = 0.88; wz0 = 0.25; wz1 = 0.9; }
    if (style == 3u) {
        // навесная стена: импосты, межэтажные полосы, остальное — стекло
        if (ffx < 0.06 || ffz < 0.08) { return s; }
        wx0 = 0.06; wx1 = 1.0; wz0 = 0.08; wz1 = 1.0;
    }
    let in_win = ffx >= wx0 && ffx < wx1 && ffz >= wz0 && ffz < wz1;
    if (!in_win) {
        // перемычка и подоконник у кирпича и известняка
        if (style != 3u && ffx >= wx0 - 0.04 && ffx < wx1 + 0.04 && ((ffz >= wz1 && ffz < wz1 + 0.07) || (ffz >= wz0 - 0.05 && ffz < wz0))) {
            s.alb = vec3f(0.55, 0.53, 0.48);
        }
        return s;
    }
    // окно: рама, стекло с отражением неба, ночью — свет комнаты
    let wu = (ffx - wx0) / (wx1 - wx0);
    let wv = (ffz - wz0) / (wz1 - wz0);
    if (style != 3u && (wu < 0.07 || wu > 0.93 || wv < 0.06 || wv > 0.94 || abs(wv - 0.5) < 0.03)) {
        s.alb = vec3f(0.5, 0.49, 0.46);
        return s;
    }
    let face = select(select(2, 3, n.z < 0.0), select(0, 1, n.x < 0.0), abs(n.x) > 0.5);
    let hv = hash3(seed + i32(ci) * 7, i32(fi), face);
    s.alb = vec3f(0.02, 0.025, 0.03);
    s.gloss = select(0.5, 0.85, style == 3u);
    if (hv < lit_k * F.ambient.w) {
        let kind = hash3(i32(ci), i32(fi), seed + 1);
        var c = vec3f(1.0, 0.76, 0.45);
        if (kind > 0.7 || (style >= 2u && kind > 0.35)) { c = vec3f(0.75, 0.85, 1.0); }
        if (kind > 0.93) { c = vec3f(0.4, 0.55, 1.0) * (0.6 + 0.4 * hash3(i32(ci), i32(fi), i32(F.cam_pos.w * 5.0))); }
        var k = 0.55 + 0.45 * hash3(i32(ci), i32(fi), seed + 2);
        // шторы/жалюзи: полосы в части окон
        let k2 = hash3(i32(ci), i32(fi), seed + 3);
        if (k2 < 0.25 && fract(wv * 8.0) < 0.4) { k *= 0.45; }
        if (k2 > 0.8 && (wu < 0.22 || wu > 0.78)) { k *= 0.35; }
        s.emit = c * k * (0.3 + 0.7 * F.sun_col.w);
    }
    return s;
}

// ---------------------------------------------------------------- свет
fn shade(ro: vec3f, rd: vec3f, r: Ray) -> vec3f {
    let p = ro + rd * r.t;
    let eps = max(0.0015, r.t * 0.0007);
    let n = normal_at(p, eps);
    let s = surface(p, n, rd, r.id);
    let po = p + n * 0.02;
    let near = r.t < 120.0;
    var sky_v = 1.0;
    var ao = 1.0;
    if (near) {
        ao = ao_at(p, n);
        // видимость неба: два луча — по нормали вверх и почти вертикально
        sky_v = 0.5 * soft_vis(po, normalize(n * 0.7 + vec3f(0.0, 1.0, 0.0)), 30.0, 1.5)
              + 0.5 * soft_vis(po, normalize(n * 0.2 + vec3f(0.0, 1.0, 0.0)), 30.0, 1.5);
    }
    // небо + грубая замена отражённого света (стены и асфальт подсвечивают друг друга)
    var light = F.ambient.rgb * ((0.2 + 0.8 * sky_v) * ao + BOUNCE);
    let ndl = dot(n, F.sun_dir.xyz);
    if (ndl > 0.0 && F.sun_dir.w > 0.0) {
        let sh = select(1.0, soft_vis(po, F.sun_dir.xyz, 150.0, 10.0), r.t < 250.0);
        light += F.sun_col.rgb * F.sun_dir.w * ndl * sh * 1.4;
    }
    if (near) {
        for (var i = 0u; i < F.grid.w; i++) {
            let L = F.lights[i];
            let v = L.pos.xyz - p;
            let d = length(v);
            if (d >= L.pos.w) { continue; }
            let ld = v / d;
            let nl = dot(n, ld);
            if (nl <= 0.0) { continue; }
            // обратные квадраты, плавно гаснет к радиусу; L.col.w — мощность
            let fall = 1.0 - d / L.pos.w;
            let att = fall * fall * L.col.w / (1.0 + d * d);
            let sh = soft_vis(po, ld, d - 0.3, 12.0);
            light += L.col.rgb * att * nl * sh;
        }
    }
    var col = s.alb * light + s.emit;
    // глянец: отражение неба по Френелю
    if (s.gloss > 0.0) {
        let fr = pow(1.0 - max(dot(-rd, n), 0.0), 4.0) * 0.9 + 0.1;
        col += sky(reflect(rd, n)) * fr * s.gloss * (0.25 + 0.75 * sky_v) * 0.5;
    }
    return col;
}

fn fog(col: vec3f, t: f32) -> vec3f {
    let f = exp(-t * F.sky_hz.w);
    return mix(F.sky_hz.rgb * 0.85, col, f);
}

// ---------------------------------------------------------------- точка входа
@compute @workgroup_size(8, 8)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    let cols = F.grid.x;
    let rows = F.grid.y;
    if (gid.x >= cols || gid.y >= rows) { return; }
    let ss = max(F.grid.z, 1u);
    var acc = vec3f(0.0);
    var star = false;
    var star_k = 0.0;
    for (var sy = 0u; sy < ss; sy++) {
        for (var sx = 0u; sx < ss; sx++) {
            let fx = (f32(gid.x) + (f32(sx) + 0.5) / f32(ss)) / f32(cols) * 2.0 - 1.0;
            let fy = (f32(gid.y) + (f32(sy) + 0.5) / f32(ss)) / f32(rows) * 2.0 - 1.0;
            let rd = normalize(F.cam_fwd.xyz + F.cam_right.xyz * fx * F.cam_fwd.w - F.cam_up.xyz * fy * F.cam_right.w);
            let ro = F.cam_pos.xyz;
            let r = march(ro, rd, F.cam_up.w);
            var c: vec3f;
            if (r.id == MISS) {
                c = sky(rd);
                // звёзды: привязаны к направлению, а не к экрану
                if (F.sun_col.w > 0.3 && sx == ss / 2u && sy == ss / 2u) {
                    let ang = 2.0 * F.cam_fwd.w / f32(cols);
                    let az = atan2(rd.x, rd.z);
                    let el = asin(clamp(rd.y, -1.0, 1.0));
                    let h = hash3(i32(floor(az / ang)), i32(floor(el / ang)), 99);
                    if (h < 0.0035 && rd.y > 0.05) { star = true; star_k = (0.5 + 80.0 * h) * F.sun_col.w; }
                }
            } else {
                c = fog(shade(ro, rd, r), r.t);
            }
            acc += c;
        }
    }
    var col = acc / f32(ss * ss) * F.sky_top.w;
    var glyph_override = 0u;
    if (star) {
        col += vec3f(0.9, 0.9, 1.0) * star_k;
        glyph_override = select(F.ramp_info.y, F.ramp_info.z, star_k > 1.2);
    }
    // ---------------- правило стиля (как style.rs)
    let L = lum(col);
    let n = F.ramp_info.x;
    var glyph = F.ramp[0].x;
    let t = (L - F.style.x) * F.style.y;
    if (t > 0.0) {
        let k = min(1u + u32(sqrt(t) * f32(n - 1u)), n - 1u);
        glyph = F.ramp[k / 4u][k % 4u];
    }
    if (glyph_override != 0u) { glyph = glyph_override; }
    let m = max(col.r, max(col.g, col.b));
    var fg = vec3f(0.0);
    var bg = vec3f(0.0);
    if (m > F.style.w) {
        fg = clamp(col * min(1.0 / m, F.style.z), vec3f(0.0), vec3f(1.0));
        bg = clamp(col, vec3f(0.0), vec3f(1.0));
    }
    let cell = vec2i(i32(gid.x), i32(gid.y));
    textureStore(out_glyph, cell, vec4u(glyph, 0u, 0u, 0u));
    textureStore(out_fg, cell, vec4f(fg, 1.0));
    textureStore(out_bg, cell, vec4f(bg, 0.0));
}
