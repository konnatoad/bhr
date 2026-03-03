use image::{ImageBuffer, Rgb};
use rayon::prelude::*;
use std::f64::consts::PI;

// 4k because i hate myself
const W: u32 = 3840;
const H: u32 = 2160;

// AA samples. 3 is “ok”. 4 is “why is my pc crying”
const SS: u32 = 3;

// fake units. i am not doing real physics.
const BH_SIZE: f64 = 1.0;

// “ring zone” where the cool lensing starts showing up
const RING_ZONE: f64 = 1.5;

// disk bounds (flat distance from center)
const DISK_IN: f64 = 3.0;
const DISK_OUT: f64 = 12.0;

// if this is too high you get BLUE. if too low you get sad mud.
const DISK_HEAT: f64 = 9000.0;

// planck constant combo thing. copied. don’t ask.
const HCK: f64 = 0.014388;

// camera setup
const CAM_POS: V3 = V3 {
    x: 0.0,
    y: 4.0,
    z: -15.0,
};
const CAM_LOOK: V3 = V3 {
    x: 0.0,
    y: -0.5,
    z: 0.0,
};
const FOV: f64 = 55.0;

// “make it not dark”. vibes only.
const EXPOSURE: f64 = 5.5;

// rgb wavelength samples. idk, some guy said these.
const WL_R: f64 = 700e-9;
const WL_G: f64 = 546e-9;
const WL_B: f64 = 435e-9;

fn main() {
    println!("rendering space noodles... or donut.. or whatever it ends up being...");
    println!("{}x{} ss={} -> {} rays/pixel", W, H, SS, SS * SS);
    println!("if it’s blue again i’m closing the editor");

    // yes this allocates everything first
    // yes pros would stream tiles
    // no i’m not rewriting it
    let rows: Vec<Vec<[u8; 3]>> = (0..H)
        .into_par_iter()
        .map(|y| (0..W).map(|x| spaghettifier(x, y)).collect())
        .collect();

    let mut img = ImageBuffer::new(W, H);
    for (y, row) in rows.iter().enumerate() {
        for (x, &c) in row.iter().enumerate() {
            img.put_pixel(x as u32, y as u32, Rgb(c));
        }
    }

    img.save("blackhole.png").unwrap();
    println!("done. i’m scared to open it.");
}

fn spaghettifier(x: u32, y: u32) -> [u8; 3] {
    // one pixel, SSxSS subpixels, average it. anti-jaggies.
    let mut r = 0.0_f64;
    let mut g = 0.0_f64;
    let mut b = 0.0_f64;

    for sy in 0..SS {
        for sx in 0..SS {
            let fx = x as f64 + (sx as f64 + 0.5) / SS as f64;
            let fy = y as f64 + (sy as f64 + 0.5) / SS as f64;

            let (rr, gg, bb) = yeet_ray(fx, fy);
            r += rr;
            g += gg;
            b += bb;
        }
    }

    let div = (SS * SS) as f64;
    [(r / div) as u8, (g / div) as u8, (b / div) as u8]
}

fn yeet_ray(px: f64, py: f64) -> (f64, f64, f64) {
    // shoot a ray from the camera through this subpixel
    let (pos, dir) = make_camera_ray(px, py);

    // march it. returns disk light + info for background
    let (dr, dg, db, ate, bg_dir, swirl) = hit(pos, dir);

    // “swirl” makes bg fade a bit. idk, looks less weird.
    let bg_fade = if ate { 0.0 } else { (-swirl * 0.5).exp() };

    // if we fell in, it’s just black
    let (sr, sg, sb) = if ate {
        (0.0, 0.0, 0.0)
    } else {
        space_bg(bg_dir)
    };

    // combine disk + bg, then exposure
    let r = (dr + sr * bg_fade) * EXPOSURE;
    let g = (dg + sg * bg_fade) * EXPOSURE;
    let b = (db + sb * bg_fade) * EXPOSURE;

    // tonemap to 0..255
    let [rr, gg, bb] = film_it(r, g, b);
    (rr as f64, gg as f64, bb as f64)
}

fn make_camera_ray(px: f64, py: f64) -> (V3, V3) {
    // screen coords -> camera ray
    let half = (FOV.to_radians() * 0.5).tan();
    let aspect = W as f64 / H as f64;

    let sx = (px / W as f64 * 2.0 - 1.0) * half * aspect;
    let sy = (1.0 - py / H as f64 * 2.0) * half;

    let fwd = make_unit(sub(CAM_LOOK, CAM_POS));
    let right = make_unit(cross(fwd, V3::UP));
    let up = cross(right, fwd);

    // if you flip sy by accident the whole world becomes cursed
    let dir = make_unit(add(add(scale(right, sx), scale(up, sy)), fwd));
    (CAM_POS, dir)
}

fn hit(mut pos: V3, mut dir: V3) -> (f64, f64, f64, bool, V3, f64) {
    // march the ray, bend it, check disk crossings
    let mut col = [0.0_f64; 3];
    let mut last_y = pos.y;
    let mut swirl = 0.0_f64;

    // 32k steps because i hate waiting AND i hate blurry rings
    for _ in 0..32_000 {
        let dist = len(pos);

        // fell into the hole
        if dist < BH_SIZE * 0.97 {
            return (col[0], col[1], col[2], true, dir, swirl);
        }

        // escaped to infinity (aka background)
        if dist > 120.0 {
            return (col[0], col[1], col[2], false, dir, swirl);
        }

        // step size “schedule”. scuffed but it works. i’m not touching it.
        let step = if dist < RING_ZONE * 1.10 {
            0.0012
        } else if dist < RING_ZONE * 1.40 {
            0.0025
        } else if dist < RING_ZONE * 2.0 {
            0.006
        } else if dist < RING_ZONE * 5.0 {
            0.02
        } else {
            0.05 * (dist / 8.0).min(2.5)
        };

        // bending. yes there’s a 1.5. no i’m not arguing with it.
        let to_center = scale(pos, 1.0 / dist);
        let inward = dot(dir, to_center);
        let sideways = sub(dir, scale(to_center, inward));

        let bend = 1.5 * BH_SIZE / (dist * dist); // dumb line that makes it look right. staying.
        dir = make_unit(add(dir, scale(sideways, -bend * step))); // sign matters. don’t “fix” it.
        swirl += bend * step;

        // disk crossing check (y=0 plane)
        let new_y = pos.y + dir.y * step;
        if last_y.signum() != new_y.signum() {
            let flat = (pos.x * pos.x + pos.z * pos.z).sqrt();
            if flat > DISK_IN && flat < DISK_OUT {
                let c = disk(flat, pos, dir);
                col[0] += c[0];
                col[1] += c[1];
                col[2] += c[2];
            }
        }

        last_y = new_y;
        pos = add(pos, scale(dir, step));

        // if this ever NaNs i’m going to bed
        // if !(pos.x.is_finite() && pos.y.is_finite() && pos.z.is_finite()) { break; }
    }

    (col[0], col[1], col[2], false, dir, swirl)
}

fn disk(flat: f64, pos: V3, dir: V3) -> [f64; 3] {
    // disk emission. takes flat radius + where we hit + ray dir.
    // returns RGB in “not clamped” space.

    let ratio = flat / DISK_IN;

    // temp profile thing. yes it’s math. yes i hate it.
    let heat = DISK_HEAT * (1.0 - ratio.powf(-0.5)).max(0.0).powf(0.25) * ratio.powf(-0.75);

    if heat < 500.0 {
        return [0.0; 3];
    }

    // orbital speed. capped because i refuse to debug infinite brightness.
    let speed = (BH_SIZE / (2.0 * flat)).sqrt().min(0.70);

    // tangential direction around center (in xz plane)
    let spin = make_unit(V3 {
        x: pos.z,
        y: 0.0,
        z: -pos.x,
    });

    // direction from hit point to camera
    let to_cam = make_unit(neg(dir));

    // doppler factor. extremely touchy. do not poke.
    let beta = dot(spin, to_cam) * speed;
    let gamma = 1.0 / (1.0 - speed * speed).sqrt();
    let dop = 1.0 / (gamma * (1.0 - beta)).max(0.01);

    // gravity dim. i don’t want to argue with it.
    let grav = (1.0 - BH_SIZE / flat).max(0.01).sqrt();

    // “observed” temp
    let seen = heat * dop * grav;

    // brightness. g^4 because everyone says g^4. ok.
    let bright = (seen / DISK_HEAT).powf(4.0) * dop.powf(4.0).min(10.0);

    // color from planck samples
    let (cr, cg, cb) = planck(seen);

    // tiny texture so it’s not a perfect toy donut
    let ang = pos.z.atan2(pos.x);
    let lumps =
        (1.0 + 0.12 * (flat * 1.9 - ang * 2.7).sin() + 0.07 * (flat * 4.1 + ang * 5.8).cos())
            .clamp(0.75, 1.25);

    [
        cr * bright * lumps,
        cg * bright * lumps,
        cb * bright * lumps,
    ]
}

fn planck(temp: f64) -> (f64, f64, f64) {
    // sampled planck-ish rgb. don’t ask me to derive it.

    if temp < 200.0 {
        return (0.0, 0.0, 0.0);
    }

    let sample = |wl: f64| -> f64 {
        let x = HCK / (wl * temp);
        if x > 500.0 {
            0.0
        } else if x < 1e-4 {
            // tiny-x hack. recommended by “it stops exploding” people.
            wl.powf(-4.0) * temp
        } else {
            wl.powf(-5.0) / (x.exp() - 1.0)
        }
    };

    let r = sample(WL_R);
    let g = sample(WL_G);
    let b = sample(WL_B);

    let m = r.max(g).max(b).max(1e-300);
    (r / m, g / m, b / m)
}

fn space_bg(d: V3) -> (f64, f64, f64) {
    // fake background. minimal effort. still looks ok.

    let d = make_unit(d);

    let mut r = 0.0003_f64;
    let mut g = 0.0003_f64;
    let mut b = 0.0004_f64;

    let upness = d.y;
    let around = d.z.atan2(d.x);

    let band = (-upness * upness * 7.0).exp()
        * (0.4 + 0.3 * (around * 2.1).sin() + 0.2 * (around * 4.9 + 0.8).cos()).max(0.0);

    r += band * 0.010;
    g += band * 0.012;
    b += band * 0.014;

    // tiny warm tint so bg isn’t a blue LED strip
    r += (1.0 - upness.abs()).max(0.0) * 0.0004;

    (r, g, b)
}

fn film_it(r: f64, g: f64, b: f64) -> [u8; 3] {
    // tonemap + gamma. i copied it once. it works. moving on.

    let f = |x: f64| ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0);
    let gamma = |x: f64| x.max(0.0).powf(1.0 / 2.2);

    [
        (gamma(f(r)) * 255.0) as u8,
        (gamma(f(g)) * 255.0) as u8,
        (gamma(f(b)) * 255.0) as u8,
    ]
}

#[derive(Copy, Clone)]
struct V3 {
    x: f64,
    y: f64,
    z: f64,
}

impl V3 {
    const UP: V3 = V3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
}

fn add(a: V3, b: V3) -> V3 {
    V3 {
        x: a.x + b.x,
        y: a.y + b.y,
        z: a.z + b.z,
    }
}
fn sub(a: V3, b: V3) -> V3 {
    V3 {
        x: a.x - b.x,
        y: a.y - b.y,
        z: a.z - b.z,
    }
}
fn scale(a: V3, s: f64) -> V3 {
    V3 {
        x: a.x * s,
        y: a.y * s,
        z: a.z * s,
    }
}
fn neg(a: V3) -> V3 {
    V3 {
        x: -a.x,
        y: -a.y,
        z: -a.z,
    }
}
fn dot(a: V3, b: V3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

fn cross(a: V3, b: V3) -> V3 {
    V3 {
        x: a.y * b.z - a.z * b.y,
        y: a.z * b.x - a.x * b.z,
        z: a.x * b.y - a.y * b.x,
    }
}

fn len(a: V3) -> f64 {
    (a.x * a.x + a.y * a.y + a.z * a.z).sqrt()
}

// normalize vector. i refuse to name it vnorm, that sounds like a math degree.
fn make_unit(v: V3) -> V3 {
    let l = len(v);
    if l < 1e-12 {
        V3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        }
    } else {
        scale(v, 1.0 / l)
    }
}
