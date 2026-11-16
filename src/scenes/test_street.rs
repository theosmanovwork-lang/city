//! Тестовая сцена этапа 2: отрезок улицы ~70 м, вечер, ясно.
//!
//! Улица идёт вдоль +Z. Проезжая часть x ∈ [-6, 6], тротуары по 4 м,
//! фасады на x = ±10. Слева — два кирпичных дома с пожарными лестницами и
//! известняковое здание; справа — кирпичный дом, дайнер с залом внутри и
//! стеклянная башня. В конце — перекрёсток с авеню, дальше — силуэт даунтауна.

use crate::build::{Builder, Facade, Hair, Look, Pose, SIDEWALK_H, pal};
use crate::sdf::{Group, Mat, Scene, mat};
use crate::subtitles::line;

pub fn build() -> Scene {
    let mut b = Builder::new("Тестовая улица");
    b.time_of_day(0.80); // 19:12
    b.fog(0.0045);
    b.spawn([-7.6, SIDEWALK_H, 2.0], 8.0);

    // ------------------------------------------------------------ улица
    let (z0, z1) = (-20.0, 72.0);
    b.sidewalk(-10.0, -6.0, z0, z1, 1.0);
    b.sidewalk(6.0, 10.0, z0, z1, -1.0);
    b.center_line(0.0, z0, z1);
    // перекрёсток с авеню (z 72..92): тротуары за ним и «зебры»
    b.crosswalk(-6.0, 6.0, 73.0, 76.0);
    b.sidewalk(-10.0, -6.0, 92.0, 140.0, 1.0);
    b.sidewalk(6.0, 10.0, 92.0, 140.0, -1.0);
    b.crosswalk(-6.0, 6.0, 88.0, 91.0);
    for z in [8.0, 27.0, 46.0, 65.0] {
        b.street_lamp(-6.55, z, 1.0);
    }
    for z in [17.0, 36.0, 55.0, 96.0] {
        b.street_lamp(6.55, z, -1.0);
    }

    // ------------------------------------------------------------ левая сторона
    let fa = Facade::brick();
    let ha = b.building([-24.0, -2.0], [-10.0, 18.0], fa, 5);
    b.fire_escape(-10.0, 1.0, 5.0, 11.0, fa, 5);
    b.stoop(-10.0, 1.0, 14.5, 1.3);
    b.water_tower(-17.0, ha, 6.0);

    let fb = Facade::brick_dark();
    let hb = b.building([-24.0, 18.3], [-10.0, 34.0], fb, 4);
    b.fire_escape(-10.0, 1.0, 23.0, 29.0, fb, 4);
    b.stoop(-10.0, 1.0, 20.5, 1.3);
    b.water_tower(-18.0, hb, 27.0);

    let fc = Facade::limestone();
    let hc = b.building([-32.0, 34.3], [-10.0, 72.0], fc, 9);
    b.tier([-30.0, 38.0], [-15.0, 68.0], hc, fc, 4);

    // ------------------------------------------------------------ правая сторона
    let fd = Facade::brick();
    let hd = b.building([10.0, -2.0], [24.0, 15.0], fd, 6);
    b.fire_escape(10.0, -1.0, 4.0, 10.0, fd, 6);
    b.water_tower(17.0, hd, 8.0);

    diner(&mut b, 10.0, 15.3, 30.0);

    let podium = Facade::limestone();
    let hp = b.building([10.0, 30.3], [36.0, 72.0], podium, 4);
    let glass = Facade::glass();
    let ht = b.tier([14.0, 34.0], [32.0, 68.0], hp, glass, 22);
    b.tier([17.0, 38.0], [29.0, 64.0], ht, glass, 4);

    // ------------------------------------------------------------ за перекрёстком
    let fo = Facade::office();
    b.building([-34.0, 92.0], [-10.0, 140.0], fo, 14);
    let hg = b.building([10.0, 92.0], [34.0, 140.0], Facade::limestone(), 11);
    b.tier([13.0, 96.0], [31.0, 136.0], hg, Facade::limestone(), 8);

    // ------------------------------------------------------------ силуэт даунтауна
    let towers: [(f32, f32, f32, f32, f32, Facade, f32); 10] = [
        (-60.0, 210.0, 34.0, 34.0, 190.0, Facade::glass(), 40.0),
        (-15.0, 260.0, 40.0, 40.0, 260.0, Facade::limestone(), 70.0),
        (40.0, 230.0, 36.0, 30.0, 160.0, Facade::office(), 0.0),
        (85.0, 300.0, 44.0, 44.0, 220.0, Facade::glass(), 30.0),
        (-110.0, 320.0, 50.0, 40.0, 150.0, Facade::office(), 0.0),
        (20.0, 380.0, 50.0, 50.0, 300.0, Facade::glass(), 60.0),
        (-70.0, 420.0, 40.0, 40.0, 200.0, Facade::limestone(), 45.0),
        (120.0, 400.0, 40.0, 40.0, 180.0, Facade::office(), 25.0),
        (-150.0, 260.0, 40.0, 40.0, 120.0, Facade::limestone(), 0.0),
        (150.0, 220.0, 30.0, 30.0, 110.0, Facade::glass(), 0.0),
    ];
    for (x, z, w, d, h, f, spire) in towers {
        b.far_tower((x, z), (w, d), h, f, spire);
    }

    // ------------------------------------------------------------ реплики
    b.trigger(
        [-12.0, -1.0, -5.0],
        [12.0, 5.0, 6.0],
        vec![line("МАРК", "Сказал же, в дайнере на углу.", 3.2), line("МАРК", "Если опять опоздает, ухожу.", 3.0)],
    );
    b.trigger(
        [10.3, -1.0, 15.6],
        [23.7, 5.0, 29.7],
        vec![line("ОФИЦИАНТКА", "Садитесь куда хотите, милый. Кофе?", 3.4), line("МАРК", "Чёрный. Я жду друга.", 2.6)],
    );
    b.trigger([-12.0, -1.0, 68.0], [12.0, 5.0, 74.0], vec![line("", "Дальше авеню. Пока сюда не нужно.", 3.0)]);

    b.finish()
}

/// Одноэтажный дайнер с маркизой и залом внутри. Фасад на x = `fx`, зал вглубь (+X).
fn diner(b: &mut Builder, fx: f32, z0: f32, z1: f32) {
    let depth = 14.0;
    let h = 4.6;
    let shell = Facade { style: crate::sdf::facade::BRICK_DARK, ..Facade::brick() };
    b.cube(
        Group::Solid,
        [fx, 0.0, z0],
        [fx + depth, h, z1],
        Mat { id: mat::FACADE, p: [h + 1.0, 50.0, 0.0, h + 1.0], style: shell.style },
    );
    // парапет
    b.cube(Group::Detail, [fx - 0.2, h, z0], [fx + depth, h + 0.6, z1], Mat::new(mat::CONCRETE));

    // зал: всё внутреннее пространство — один вырез; пол — плитка
    let (ix0, ix1) = (fx + 0.3, fx + depth - 0.3);
    let (iz0, iz1) = (z0 + 0.3, z1 - 0.3);
    b.cube(Group::Cut, [ix0, 0.02, iz0], [ix1, 4.2, iz1], Mat::rgb(mat::PLASTER, pal::CREAM));
    b.cube(Group::Detail, [ix0, 0.0, iz0], [ix1, SIDEWALK_H, iz1], Mat::new(mat::TILE));

    // проёмы: дверь и две витрины; рамы — металл
    let frame = Mat::rgb(mat::METAL, [0.55, 0.56, 0.58]);
    let zd = (z0 + z1) * 0.5;
    // проёмы режем с запасом по глубине: расстояние max(стена, −вырез) занижено у граней выреза,
    // и узкий вырез давал бы «невидимую стенку» для коллизий
    b.cube(Group::Cut, [fx - 1.0, SIDEWALK_H, zd - 0.7], [fx + 1.0, 2.4, zd + 0.7], frame);
    for (wa, wb) in [(z0 + 0.6, zd - 1.2), (zd + 1.2, z1 - 0.6)] {
        b.cube(Group::Cut, [fx - 1.0, 0.9, wa], [fx + 1.0, 2.8, wb], frame);
        b.collider([fx - 0.05, 0.0, wa], [fx + 0.1, 3.0, wb]);
        // импосты
        let n = 3;
        for k in 0..=n {
            let z = wa + (wb - wa) * k as f32 / n as f32;
            b.cube(Group::Detail, [fx + 0.05, 0.9, z - 0.04], [fx + 0.12, 2.8, z + 0.04], frame);
        }
        b.cube(Group::Detail, [fx + 0.05, 2.2, wa], [fx + 0.12, 2.26, wb], frame);
    }

    // маркиза над витриной (нависает над тротуаром) и фестон
    let aw = Mat::rgb(mat::AWNING, pal::DINER_RED);
    b.cube(Group::Detail, [fx - 1.9, 3.0, z0 + 0.2], [fx, 3.12, z1 - 0.2], aw);
    b.cube(Group::Detail, [fx - 1.95, 2.7, z0 + 0.2], [fx - 1.88, 3.12, z1 - 0.2], aw);
    // световой короб вывески над маркизой
    b.cube(
        Group::Detail,
        [fx - 0.35, 3.3, zd - 4.0],
        [fx, 4.4, zd + 4.0],
        Mat { id: mat::SIGN, p: [0.9, 0.25, 0.18, 1.6], style: 0 },
    );

    // стойка, хромированная кромка, табуреты
    let cx = fx + 9.4;
    b.cube(Group::Detail, [cx, SIDEWALK_H, z0 + 1.6], [cx + 0.9, 1.08, z1 - 1.6], Mat::rgb(mat::FORMICA, pal::MINT));
    b.cube(Group::Detail, [cx - 0.12, 1.08, z0 + 1.5], [cx + 1.0, 1.13, z1 - 1.5], Mat::new(mat::CHROME));
    let mut z = z0 + 2.3;
    while z < z1 - 2.0 {
        b.cyl(Group::Detail, [cx - 0.7, SIDEWALK_H, z], 0.04, 0.62, Mat::new(mat::CHROME));
        b.cyl(Group::Detail, [cx - 0.7, 0.74, z], 0.21, 0.09, Mat::rgb(mat::VINYL, pal::DINER_RED));
        z += 1.25;
    }
    // задняя стойка и полки
    let wood = Mat::rgb(mat::WOOD, pal::WOOD_DARK);
    b.cube(Group::Detail, [ix1 - 0.7, SIDEWALK_H, z0 + 1.0], [ix1, 1.0, z1 - 1.0], wood);
    for y in [1.7, 2.3] {
        b.cube(Group::Detail, [ix1 - 0.35, y, z0 + 1.5], [ix1, y + 0.05, z1 - 1.5], wood);
    }
    // кабинки у витрин: стол и два дивана
    for zc in [z0 + 2.7, z1 - 2.7] {
        b.cube(
            Group::Detail,
            [ix0 + 0.3, 0.72, zc - 0.45],
            [ix0 + 1.3, 0.78, zc + 0.45],
            Mat::rgb(mat::FORMICA, pal::MINT),
        );
        b.cyl(Group::Detail, [ix0 + 0.8, SIDEWALK_H, zc], 0.06, 0.6, Mat::new(mat::CHROME));
        for dz in [-1.0, 1.0] {
            let s = zc + dz;
            b.rounded(
                Group::Detail,
                [ix0 + 0.1, SIDEWALK_H, s - 0.3],
                [ix0 + 1.5, 0.48, s + 0.3],
                0.05,
                Mat::rgb(mat::VINYL, pal::DINER_RED),
            );
            let back = s + dz * 0.25;
            b.rounded(
                Group::Detail,
                [ix0 + 0.1, 0.48, back - 0.08],
                [ix0 + 1.5, 1.15, back + 0.08],
                0.04,
                Mat::rgb(mat::VINYL, pal::DINER_RED),
            );
        }
    }
    // люди: официантка за стойкой и двое посетителей
    let waitress = Look {
        female: true,
        skin: [0.62, 0.45, 0.36],
        hair: Hair::Long,
        hair_color: [0.35, 0.16, 0.06],
        top: [0.55, 0.62, 0.66],
        pants: [0.12, 0.12, 0.14],
        shoes: [0.9, 0.9, 0.88],
    };
    b.person([cx + 1.4, SIDEWALK_H, zd + 1.0], -90f32.to_radians(), &waitress, Pose::Stand);
    let guest = Look {
        female: false,
        skin: [0.45, 0.30, 0.22],
        hair: Hair::Short,
        hair_color: [0.05, 0.04, 0.03],
        top: [0.30, 0.18, 0.10],
        pants: [0.10, 0.14, 0.25],
        shoes: [0.08, 0.06, 0.05],
    };
    b.person([cx - 1.1, SIDEWALK_H, z0 + 4.8], 90f32.to_radians(), &guest, Pose::Stand);
    let old = Look { hair: Hair::Bald, skin: [0.70, 0.55, 0.45], top: [0.25, 0.27, 0.22], ..guest };
    b.person([ix0 + 2.2, SIDEWALK_H, z1 - 4.6], -90f32.to_radians(), &old, Pose::Stand);

    // светильники и свет в зале
    for zl in [z0 + 3.5, zd, z1 - 3.5] {
        for xl in [fx + 4.0, fx + 9.0] {
            b.cyl(Group::Detail, [xl, 3.9, zl], 0.28, 0.12, Mat::glow(pal::WARM_BULB, 2.5));
            b.cyl(Group::Detail, [xl, 4.02, zl], 0.02, 0.2, Mat::new(mat::CHROME));
        }
        b.light([fx + 6.5, 3.5, zl], pal::WARM_BULB, 11.0, 10.0);
    }
}
