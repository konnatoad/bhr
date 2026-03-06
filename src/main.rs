// COSMIC PASTA SIMULATOR
//
// featuring:
// 15 million screaming noodles
// illegal gravity seasoning
// photon harassment
// accretion disk vomit
//
// what this cursed thing does:
// - summon a ridiculous amount of tiny space noodles
// - throw them near an angry gravity marble (black hole)
// - stir the noodles with questionable relativistic witchcraft
// - let angry marble math bend spacetime like overcooked spaghetti
// - convert the surviving pasta chaos into a density oracle grid
// - ray-march photons through warped space until they confess a picture
// - dump result into a png and pretend the math was intentional
//
// warning:
// some parts are actual physics.
// some parts are me poking constants until the cosmic pasta looks right.
// somewhere between those two lives the truth.
//
// if it breaks: spacetime shifted again.
// if it works: do not touch the haunted constants.

use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use image::{ImageBuffer, Rgb};
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;
use std::env;
use std::f64::consts::PI;
use wgpu::util::DeviceExt;

// this summons millions of cosmic noodles,
// bends spacetime slightly illegally,
// and then paints an accretion disk made of astrophysical pasta vomit.
// if it looks wrong: either relativity or the pasta spirits.

// WARNING:
// SETTINGS TWEAK AS YOU WISH
// most important knobs first.
// if the render looks cursed, start debugging up here.

// NOTE: Anti-aliasing / supersampling.
// 2 = sane
// 3 = very clean
// 4 = GPU starts sweating
const SS: u32 = 4; // SS4 goes hard

// NOTE: how many times each photon pokes spacetime.
// higher = more accuracy near the angry marble.
const MAX_STEPS: u32 = 50_000; // 50k is a good default.

// NOTE: main render resolution.
// bigger = prettier marble but slower.
const W: u32 = 7680; // use 7680x4320 for good marble. iters getting squashed anyways :3
const H: u32 = 4320; // use 3840x2160 for faster render

// WARNING:
// HDR / EXR render resolution.
// if you push this too high wgpu will start yelling about buffer sizes.
const HDR_W: u32 = 3840;
const HDR_H: u32 = 2160;

// NOTE: donut pixels (grid of suffering)
// this controls how detailed the accretion disk density field is.
const DR: usize = 768; // 256x1024 for fast render
const DA: usize = 3072; // 640x2560 for good sauce
// or try 768x3072 idfk cosmic winds decide this i suppose.

// WARNING:
// end of actually important settings.
// everything below here is mostly vibe tuning.

// NOTE: black hole properties.
const BH_MASS: f64 = 2.0; // 2.0 is "realistic"
const BH_SIZE: f64 = 0.85; // radius of "no refunds, no witnesses"
// put BH_MASS 2.5+ and BH_SIZE 1.0+ for more cinematic evil marble

// NOTE: cursed pasta ring bounds
const DISK_IN: f64 = 1.2; // 1.2 looks nice, and is believable
const DISK_OUT: f64 = 12.0; // 12.0 also looks believable

// NOTE: disk appearance sliders
// these were tuned until the disk stopped looking like wet lint.
const DISK_HEAT: f64 = 18000.0; // glow spell intensity
const ADISK_DENSITY_V: f64 = 5.0; // vertical squish curse
const ADISK_DENSITY_H: f64 = 3.2; // horizontal fade chant
const ADISK_HEIGHT: f64 = 0.014; // cosmic crepe thickness (must stay thin)
const ADISK_LIT: f64 = 0.46; // "pls show up on screen" rune
const ADISK_NOISE_SCALE: f64 = 2.2; // turbulence dial (fake trauma texture)

// NOTE: camera placement
// move this if you want to film the pasta ritual from another angle.
const CAM_POS: DVec3 = DVec3::new(0.0, 1.2, -14.0); // 0.0, 2.2, -14.0 is my defaults
const CAM_LOOK: DVec3 = DVec3::new(1.5, 1.5, 0.0); // 0.0, 0.0, 0.0 = looking directly at marble
const CAM_ROLL: f64 = -8.0; // degrees
const FOV: f64 = 65.0; // how far u watching the emo marble from

// NOTE: preview brightness for PNG output
const EXPOSURE: f64 = 0.46; // when the void is too emo

// HACK:
// ray march helper knob.
// currently doesn't do much unless stepping logic changes.
const RING_ZONE: f64 = 1.0; // arbitrary numbers that barely does anything

// PERF:
// number of particles in the disk simulation.
// higher = smoother density field but slower CPU phase.
const ROCKS: usize = 40_000_000; // honestly 8mil is fine... 3mil if want faster

// PERF:
// how long the particle simulation runs.
const STEPS: usize = 8_000; // 4k is pretty default. honestly no need to touch

// WARNING:
// simulation timestep.
// small so the universe doesn't instantly file a complaint.
const DT: f64 = 0.0018;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Png,
    Tiff,
    Exr,
    All,
}

// WARNING:
// output mode decides whether we go through the huge HDR path or the lighter LDR path.
// if png accidentally starts using hdr again, render time and memory usage both get cursed.
impl OutputFormat {
    fn needs_hdr(self) -> bool {
        matches!(
            self,
            OutputFormat::Tiff | OutputFormat::Exr | OutputFormat::All
        )
    }

    fn needs_ldr(self) -> bool {
        matches!(self, OutputFormat::Png | OutputFormat::All)
    }
}

fn parse_output_format() -> OutputFormat {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.iter().any(|a| a == "--all" || a == "-all") {
        OutputFormat::All
    } else if args.iter().any(|a| a == "--exr" || a == "-exr") {
        OutputFormat::Exr
    } else if args
        .iter()
        .any(|a| a == "--tiff" || a == "--tif" || a == "-tiff" || a == "-tif")
    {
        OutputFormat::Tiff
    } else {
        OutputFormat::Png
    }
}

fn filmic(x: f32) -> f32 {
    let x2 = (x - 0.004).max(0.0);
    (x2 * (6.2 * x2 + 0.5)) / (x2 * (6.2 * x2 + 1.7) + 0.06)
}

// WARNING:
// this is the full tone-map path for png preview.
// changing this affects the "final look" more than most disk sliders do.
fn hdr_to_u8_image(rgb: &[f32], w: u32, h: u32, exposure: f32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;

            let mut r = rgb[i] * exposure;
            let mut g = rgb[i + 1] * exposure;
            let mut b = rgb[i + 2] * exposure;

            r = r / (1.0 + r * 1.3);
            g = g / (1.0 + g * 1.3);
            b = b / (1.0 + b * 1.3);

            let rr = (filmic(r).clamp(0.0, 1.0) * 255.0) as u8;
            let gg = (filmic(g).clamp(0.0, 1.0) * 255.0) as u8;
            let bb = (filmic(b).clamp(0.0, 1.0) * 255.0) as u8;

            img.put_pixel(x, y, Rgb([rr, gg, bb]));
        }
    }

    img
}

// NOTE:
// tiff is intentionally not raw exr-style hdr.
// this is a softer "light hdr squeeze" so the file keeps more highlight detail
// without going full void goblin like exr does.
fn hdr_to_u16_tiff_image(
    rgb: &[f32],
    w: u32,
    h: u32,
    exposure: f32,
) -> ImageBuffer<Rgb<u16>, Vec<u16>> {
    let mut img: ImageBuffer<Rgb<u16>, Vec<u16>> = ImageBuffer::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;

            let mut r = rgb[i] * exposure;
            let mut g = rgb[i + 1] * exposure;
            let mut b = rgb[i + 2] * exposure;

            // light hdr squeeze instead of full void strangling
            r = r / (1.0 + r * 0.45);
            g = g / (1.0 + g * 0.45);
            b = b / (1.0 + b * 0.45);

            let rr = (filmic(r).clamp(0.0, 1.0) * 65535.0) as u16;
            let gg = (filmic(g).clamp(0.0, 1.0) * 65535.0) as u16;
            let bb = (filmic(b).clamp(0.0, 1.0) * 65535.0) as u16;

            img.put_pixel(x, y, Rgb([rr, gg, bb]));
        }
    }

    img
}

// PERF:
// this box downscale is simple and safe, but not cheap.
// huge hdr buffers will spend noticeable time here.
fn downscale_hdr_box(src: &[f32], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; (dw * dh * 3) as usize];

    let sx = sw as f32 / dw as f32;
    let sy = sh as f32 / dh as f32;

    for y in 0..dh {
        let y0 = (y as f32 * sy).floor() as u32;
        let y1 = ((y as f32 + 1.0) * sy).ceil() as u32;
        let y1 = y1.min(sh);

        for x in 0..dw {
            let x0 = (x as f32 * sx).floor() as u32;
            let x1 = ((x as f32 + 1.0) * sx).ceil() as u32;
            let x1 = x1.min(sw);

            let mut r = 0.0f32;
            let mut g = 0.0f32;
            let mut b = 0.0f32;
            let mut n = 0.0f32;

            for yy in y0..y1 {
                for xx in x0..x1 {
                    let si = ((yy * sw + xx) * 3) as usize;
                    r += src[si];
                    g += src[si + 1];
                    b += src[si + 2];
                    n += 1.0;
                }
            }

            let di = ((y * dw + x) * 3) as usize;
            if n > 0.0 {
                out[di] = r / n;
                out[di + 1] = g / n;
                out[di + 2] = b / n;
            }
        }
    }

    out
}

// NOTE:
// bloom is only applied to the ldr preview path right now.
// exr stays raw, tiff stays softly squeezed, png gets the pretty lying filter.
fn apply_bloom_u8(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>) {
    use image::imageops;

    let mut bright = img.clone();

    // extract bright pixels
    for p in bright.pixels_mut() {
        let v = (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0;
        if v < 215.0 {
            *p = image::Rgb([0, 0, 0]);
        }
    }

    // blur them
    let glow = imageops::blur(&bright, 4.0);

    // add glow back
    for (base, g) in img.pixels_mut().zip(glow.pixels()) {
        base[0] = base[0].saturating_add((g[0] as f32 * 0.20) as u8);
        base[1] = base[1].saturating_add((g[1] as f32 * 0.20) as u8);
        base[2] = base[2].saturating_add((g[2] as f32 * 0.20) as u8);
    }
}

fn rgb_u8_to_image(rgb: &[u8], w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);

    for y in 0..h as usize {
        for x in 0..w as usize {
            let i = (y * w as usize + x) * 3;
            img.put_pixel(x as u32, y as u32, Rgb([rgb[i], rgb[i + 1], rgb[i + 2]]));
        }
    }

    img
}

fn save_png(img: &ImageBuffer<Rgb<u8>, Vec<u8>>, path: &str) {
    img.save(path).unwrap();
    println!("saved {}", path);
}

fn save_tiff_u16(img: &ImageBuffer<Rgb<u16>, Vec<u16>>, path: &str) {
    img.save_with_format(path, image::ImageFormat::Tiff)
        .unwrap();
    println!("saved {}", path);
}

fn save_exr_from_f32(rgb: &[f32], w: u32, h: u32, path: &str) {
    use exr::prelude::*;

    write_rgb_file(path, w as usize, h as usize, |x, y| {
        let i = (y * w as usize + x) * 3;
        (rgb[i], rgb[i + 1], rgb[i + 2])
    })
    .unwrap();

    println!("saved {}", path);
}

fn main() {
    let output_format = parse_output_format();
    println!("output format: {:?}", output_format);

    println!("summoning {} space noodles...", ROCKS);
    let mut idiots = make_the_donut(ROCKS);

    println!("stirring gravity cauldron for {} steps...", STEPS);
    let_the_idiots_spin(&mut idiots, STEPS);

    println!(
        "distilling pasta fog into a grid from {} survivors...",
        idiots.len()
    );
    let field = build_field(&idiots);

    // WARNING:
    // png should stay on the ldr path.
    // if this starts calling hdr render again, memory use goes nuclear for no reason.
    if output_format.needs_ldr() {
        println!("gpu photon bullying ldr {}x{} ss={}...", W, H, SS);
        let rgb = pollster::block_on(render_wgpu_ldr(&field, W, H));
        println!("gpu ldr bullying finished.");

        println!("assembling png preview...");
        let mut img = rgb_u8_to_image(&rgb, W, H);

        println!("applying bloom...");
        apply_bloom_u8(&mut img);

        match output_format {
            OutputFormat::Png => {
                println!("saving png preview...");
                save_png(&img, "blackhole_sim.png");
            }
            OutputFormat::All => {
                println!("saving png preview...");
                save_png(&img, "blackhole_sim.png");
            }
            _ => {}
        }
    }

    // WARNING:
    // hdr path is expensive and buffer-size sensitive.
    // keep HDR_W / HDR_H sane or wgpu will publicly humiliate you.
    if output_format.needs_hdr() {
        println!("gpu photon bullying hdr {}x{} ss={}...", HDR_W, HDR_H, SS);
        let hdr = pollster::block_on(render_wgpu_hdr(&field, HDR_W, HDR_H));
        println!("gpu hdr bullying finished.");

        let hdr_out = if HDR_W != 3840 || HDR_H != 2160 {
            println!("downscaling hdr to 4k...");
            downscale_hdr_box(&hdr, HDR_W, HDR_H, 3840, 2160)
        } else {
            hdr
        };

        match output_format {
            OutputFormat::Tiff => {
                println!("building light-hdr tiff...");
                let tiff_img = hdr_to_u16_tiff_image(&hdr_out, 3840, 2160, 1.0);
                println!("saving tiff...");
                save_tiff_u16(&tiff_img, "blackhole_sim.tiff");
            }
            OutputFormat::Exr => {
                println!("saving 32-bit hdr exr...");
                save_exr_from_f32(&hdr_out, 3840, 2160, "blackhole_sim.exr");
            }
            OutputFormat::All => {
                println!("building light-hdr tiff...");
                let tiff_img = hdr_to_u16_tiff_image(&hdr_out, 3840, 2160, 1.0);
                save_tiff_u16(&tiff_img, "blackhole_sim.tiff");

                println!("saving 32-bit hdr exr...");
                save_exr_from_f32(&hdr_out, 3840, 2160, "blackhole_sim.exr");
            }
            _ => {}
        }
    }

    println!("done. if it’s ugly, blame spacetime. i’m just the pasta witch.");
}

#[derive(Clone)]
struct Grain {
    pos: DVec3,
    vel: DVec3,
    alive: bool, // vibe check flag. false means it got eaten by the void.
}

fn make_the_donut(n: usize) -> Vec<Grain> {
    let mut rng = SmallRng::seed_from_u64(0xdead_beef_cafe_babe); // deterministic curse
    let mut out = Vec::with_capacity(n);

    for _ in 0..n {
        // bias toward inner ring because the demon lives there
        let t: f64 = rng.random();
        let r = DISK_IN + (DISK_OUT - DISK_IN) * t * t;

        // random angle around the cosmic noodle
        let a: f64 = rng.random::<f64>() * 2.0 * PI;

        // tiny vertical wobble so the pasta cloud isn't a perfect lie
        let y = (rng.random::<f64>() - 0.5) * 0.18 * (r / DISK_OUT).powf(0.4);

        // actual position in 3d sadness
        let pos = DVec3::new(r * a.cos(), y, r * a.sin());

        // orbit speed (newton cosplay, don't tell einstein)
        let v_circ = (BH_MASS / r).sqrt();

        // wobble: inject chaos, as a treat
        let wobble = (rng.random::<f64>() - 0.5) * v_circ * 0.04;

        // tangent velocity so it spins like it’s possessed
        let vel = DVec3::new(
            -a.sin() * (v_circ + wobble),
            (rng.random::<f64>() - 0.5) * 0.008,
            a.cos() * (v_circ + wobble),
        );

        out.push(Grain {
            pos,
            vel,
            alive: true, // optimistic. foolish.
        });
    }

    out
}

fn let_the_idiots_spin(grains: &mut Vec<Grain>, steps: usize) {
    let report_every = steps / 10;

    for step in 0..steps {
        if report_every > 0 && step % report_every == 0 {
            println!(
                "  chant {}/{} alive={}",
                step,
                steps,
                grains.iter().filter(|g| g.alive).count()
            );
        }

        // parallel chaos because cpu go brrrr
        grains.par_iter_mut().for_each(|g| {
            if !g.alive {
                return; // already got vacuumed by the void
            }

            let r2 = g.pos.length_squared();
            let r = r2.sqrt();

            // touched the angry marble. deleted.
            if r < BH_SIZE {
                g.alive = false;
                return;
            }

            // gravity hex (i am not doing real GR, i’m summoning the vibe of it)
            let inv_r3 = 1.0 / (r2 * r);

            // this term is literally "i whispered general relativity at newton"
            let grav = g.pos * (-BH_MASS * inv_r3 * (1.0 + 3.0 * BH_MASS / r));

            // tiny damping so noodles don't become confetti
            let radial = g.pos / r;
            let goo = radial * (-g.vel.dot(radial) * 0.0003);

            // let gravity ruin everyone's day
            let a = grav + goo;

            // euler integration: cheap spellcasting
            g.vel += a * DT;
            g.pos += g.vel * DT;

            // horizon-ish cleanup because i refuse to debate edge cases at 3am
            if g.pos.length() < BH_SIZE * 1.05 {
                g.alive = false;
            }
        });
    }

    grains.retain(|g| g.alive);
    println!("  {} noodles survived (barely)", grains.len());
}

#[derive(Clone)]
struct DiskField {
    density: Vec<f32>,
    vtan: Vec<f32>,
    density_max: f32, // normalization so the shader doesn't go supernova
}

fn field_idx(ri: usize, ai: usize) -> usize {
    ri * DA + ai
}

fn build_field(grains: &[Grain]) -> DiskField {
    // density = how much cosmic pasta is packed here
    let mut density = vec![0.0_f64; DR * DA];

    // vtan = how fast the pasta is zooming around
    let mut vtan = vec![0.0_f64; DR * DA];

    // counts = how many noodles screamed into each cell
    let mut counts = vec![0_u32; DR * DA];

    for g in grains {
        // flatten to disk plane: y is cringe, we live in xz now
        let flat = (g.pos.x * g.pos.x + g.pos.z * g.pos.z).sqrt();
        if flat < DISK_IN || flat > DISK_OUT {
            continue;
        }

        // radial bucket (donut layers)
        let ri = ((flat - DISK_IN) / (DISK_OUT - DISK_IN) * DR as f64) as usize;
        let ri = ri.min(DR - 1);

        // angular bucket (donut slices)
        let ang = g.pos.z.atan2(g.pos.x) + PI;
        let ai = (ang / (2.0 * PI) * DA as f64) as usize % DA;

        let spin = DVec3::new(-g.pos.z / flat, 0.0, g.pos.x / flat);
        let vel = g.vel.dot(spin);

        // distribute contribution to nearby cells
        for dr in -1..=1 {
            for da in -1..=1 {
                let rr = (ri as isize + dr).clamp(0, (DR - 1) as isize) as usize;
                let aa = ((ai as isize + da).rem_euclid(DA as isize)) as usize;

                let weight = match (dr, da) {
                    (0, 0) => 1.0,
                    (0, _) | (_, 0) => 0.5,
                    _ => 0.25,
                };

                let idx = field_idx(rr, aa);

                density[idx] += weight;
                vtan[idx] += vel * weight;
                counts[idx] += weight as u32;
            }
        }
    }

    for i in 0..DR * DA {
        if counts[i] > 0 {
            vtan[i] /= counts[i] as f64;
        }
    }

    // pasta cosmetics: smooth it so it doesn't look like minecraft ravioli
    let density = blur_field(density);
    let max_d = density.iter().cloned().fold(0.0_f64, f64::max).max(1.0);

    DiskField {
        density: density.into_iter().map(|v| v as f32).collect(),
        vtan: vtan.into_iter().map(|v| v as f32).collect(),
        density_max: max_d as f32,
    }
}

fn blur_field(mut f: Vec<f64>) -> Vec<f64> {
    // 0..4 ideal for faster render. 0..8 for smoother pasta
    for _ in 0..8 {
        let src = f.clone();

        for ri in 0..DR {
            for ai in 0..DA {
                let l = (ai + DA - 1) % DA;
                let r = (ai + 1) % DA;

                let ru = ri.saturating_sub(1);
                let rd = (ri + 1).min(DR - 1);

                f[field_idx(ri, ai)] = (src[field_idx(ri, ai)] * 4.0
                    + src[field_idx(ri, l)]
                    + src[field_idx(ri, r)]
                    + src[field_idx(ru, ai)]
                    + src[field_idx(rd, ai)])
                    / 8.0;
            }
        }
    }

    f
}

// ── GPU renderer ──────────────────────────────────────────────────────────
//
// welcome to the photon torture chamber.
// wgsl uniform layout is a drama queen.
// align(16) so wgpu stops screaming at me.
// i do not fully know what i'm doing.
// if it breaks: spacetime did it.
// if it works: also spacetime. i just held the spoon.

// WARNING:
// uniform layout must match wgsl Params exactly.
// if this struct changes and shader struct does not, spacetime explodes immediately.
#[repr(C, align(16))]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Params {
    w: u32,
    h: u32,
    ss: u32,
    _pad0: u32,

    dr: u32,
    da: u32,
    _padx: [u32; 2],

    bh_size: f32,
    bh_mass: f32,
    disk_in: f32,
    disk_out: f32,

    disk_heat: f32,
    density_max: f32,
    ad_v: f32,
    ad_h: f32,

    ad_height: f32,
    ad_lit: f32,
    ad_noise: f32,
    exposure: f32,

    max_steps: u32,
    _pad1a: u32,
    _pad1b: u32,
    _pad1c: u32,

    cam_pos: [f32; 4],
    cam_look: [f32; 4],

    fov: f32,
    ring_zone: f32,
    roll: f32,
    _pad2a: f32,
}

// WARNING:
// this helper builds the exact param blob shared by cpu + shader.
// any mismatch here vs wgsl is instant gremlin behavior.
fn make_params(render_w: u32, render_h: u32, field: &DiskField) -> Params {
    Params {
        w: render_w,
        h: render_h,
        ss: SS,
        _pad0: 0,

        dr: DR as u32,
        da: DA as u32,
        _padx: [0; 2],

        bh_size: BH_SIZE as f32,
        bh_mass: BH_MASS as f32,
        disk_in: DISK_IN as f32,
        disk_out: DISK_OUT as f32,

        disk_heat: DISK_HEAT as f32,
        density_max: field.density_max.max(1.0),
        ad_v: ADISK_DENSITY_V as f32,
        ad_h: ADISK_DENSITY_H as f32,

        ad_height: ADISK_HEIGHT as f32,
        ad_lit: ADISK_LIT as f32,
        ad_noise: ADISK_NOISE_SCALE as f32,
        exposure: EXPOSURE as f32,

        max_steps: MAX_STEPS,
        _pad1a: 0,
        _pad1b: 0,
        _pad1c: 0,

        cam_pos: [CAM_POS.x as f32, CAM_POS.y as f32, CAM_POS.z as f32, 0.0],
        cam_look: [CAM_LOOK.x as f32, CAM_LOOK.y as f32, CAM_LOOK.z as f32, 0.0],

        fov: FOV as f32,
        ring_zone: RING_ZONE as f32,
        roll: CAM_ROLL as f32,
        _pad2a: 0.0,
    }
}

async fn render_wgpu_hdr(field: &DiskField, render_w: u32, render_h: u32) -> Vec<f32> {
    println!("Params size = {}", std::mem::size_of::<Params>());

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("no gpu (ritual failed)");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("bhr_hdr"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("no device (the void said no)");

    let mut packed = Vec::<f32>::with_capacity(DR * DA * 2);
    packed.extend_from_slice(&field.density);
    packed.extend_from_slice(&field.vtan);

    let field_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("field"),
        contents: bytemuck::cast_slice(&packed),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let params = make_params(render_w, render_h, field);

    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let out_len = (render_w as usize) * (render_h as usize);
    // WARNING:
    // hdr output uses vec4<f32> per pixel.
    // buffer size climbs insanely fast here.
    // this is the main reason hdr is capped lower than png preview.
    let out_bytes = (out_len * 16) as u64; // vec4<f32> per pixel

    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("out_hdr"),
        size: out_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb_hdr"),
        size: out_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bhr_hdr"),
        source: wgpu::ShaderSource::Wgsl(SHADER_HDR.into()),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bgl_hdr"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<Params>() as u64).unwrap(),
                    ),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg_hdr"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: field_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });

    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl_hdr"),
        bind_group_layouts: &[&bgl],
        immediate_size: 0,
    });

    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("pipe_hdr"),
        layout: Some(&pl),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("enc_hdr"),
    });

    {
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pass_hdr"),
            timestamp_writes: None,
        });

        cp.set_pipeline(&pipe);
        cp.set_bind_group(0, &bg, &[]);
        println!("dispatching hdr compute shader...");
        cp.dispatch_workgroups(render_w.div_ceil(16), render_h.div_ceil(16), 1);
    }

    enc.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, out_bytes);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();

    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).unwrap();
    });

    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::MAX),
    });

    rx.recv().unwrap().expect("map failed (alignment curse?)");

    let data = slice.get_mapped_range().to_vec();
    readback.unmap();

    let f32s: &[f32] = bytemuck::cast_slice(&data);

    let mut rgb = vec![0.0f32; out_len * 3];
    for i in 0..out_len {
        let o = i * 4;
        rgb[i * 3] = f32s[o];
        rgb[i * 3 + 1] = f32s[o + 1];
        rgb[i * 3 + 2] = f32s[o + 2];
    }

    rgb
}

async fn render_wgpu_ldr(field: &DiskField, render_w: u32, render_h: u32) -> Vec<u8> {
    println!("Params size = {}", std::mem::size_of::<Params>());

    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("no gpu (ritual failed)");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("bhr_ldr"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("no device (the void said no)");

    // PERF:
    // field packing is cheap compared to the render, but still large.
    // this happens every render pass, both ldr and hdr.
    let mut packed = Vec::<f32>::with_capacity(DR * DA * 2);
    packed.extend_from_slice(&field.density);
    packed.extend_from_slice(&field.vtan);

    let field_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("field"),
        contents: bytemuck::cast_slice(&packed),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let params = make_params(render_w, render_h, field);

    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let out_len = (render_w as usize) * (render_h as usize);
    let out_bytes = (out_len * 4) as u64; // packed rgba8 in u32

    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("out_ldr"),
        size: out_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb_ldr"),
        size: out_bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bhr_ldr"),
        source: wgpu::ShaderSource::Wgsl(SHADER_LDR.into()),
    });

    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bgl_ldr"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        std::num::NonZeroU64::new(std::mem::size_of::<Params>() as u64).unwrap(),
                    ),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg_ldr"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: field_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: out_buf.as_entire_binding(),
            },
        ],
    });

    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl_ldr"),
        bind_group_layouts: &[&bgl],
        immediate_size: 0,
    });

    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("pipe_ldr"),
        layout: Some(&pl),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("enc_ldr"),
    });

    {
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pass_ldr"),
            timestamp_writes: None,
        });

        cp.set_pipeline(&pipe);
        cp.set_bind_group(0, &bg, &[]);
        println!("dispatching ldr compute shader...");
        cp.dispatch_workgroups(render_w.div_ceil(16), render_h.div_ceil(16), 1);
    }

    enc.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, out_bytes);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();

    // WARNING:
    // if map_async / poll / unmap flow gets touched wrong,
    // readback either hangs or starts returning cursed garbage.
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).unwrap();
    });

    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::MAX),
    });

    rx.recv().unwrap().expect("map failed (alignment curse?)");

    let data = slice.get_mapped_range().to_vec();
    readback.unmap();

    let mut rgb = vec![0u8; out_len * 3];
    for i in 0..out_len {
        rgb[i * 3] = data[i * 4];
        rgb[i * 3 + 1] = data[i * 4 + 1];
        rgb[i * 3 + 2] = data[i * 4 + 2];
    }

    rgb
}

// ── WGSL compute shader ───────────────────────────────────────────────────
//
// shader jail:
// i absolutely do not fully understand half of this, i just know where it bites.
// photons get bullied until they draw cosmic pasta vomit.
// if a physicist sees this: no you didn't.

const SHADER_HDR: &str = r#"
struct Params {
  w: u32, h: u32, ss: u32, _pad0: u32,
  dr: u32, da: u32, _padx0: u32, _padx1: u32,

  bh_size: f32, bh_mass: f32, disk_in: f32, disk_out: f32,
  disk_heat: f32, density_max: f32, ad_v: f32, ad_h: f32,
  ad_height: f32, ad_lit: f32, ad_noise: f32, exposure: f32,

  max_steps: u32, _pad1a: u32, _pad1b: u32, _pad1c: u32,

  cam_pos: vec4<f32>,
  cam_look: vec4<f32>,

  fov: f32, ring_zone: f32, roll: f32, _pad2a: f32,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> field: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<vec4<f32>>;

fn make_unit(v: vec3<f32>) -> vec3<f32> {
  let l = length(v);
  if l < 1e-12 { return vec3<f32>(0.0, 0.0, 1.0); }
  return v / l;
}

fn filmic(x: f32) -> f32 {
  let x2 = max(x - 0.004, 0.0);
  return (x2 * (6.2*x2 + 0.5)) / (x2 * (6.2*x2 + 1.7) + 0.06);
}

fn rand2(x: f32, y: f32) -> vec2<f32> {
  let a = fract(sin(x*127.1 + y*311.7) * 43758.5453);
  let b = fract(sin(x*269.5 + y*183.3) * 43758.5453);
  return vec2<f32>(a, b);
}

fn hash3(p: vec3<f32>) -> f32 {
  var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
  q += dot(q, q.yzx + 19.19);
  return fract((q.x + q.y) * q.z);
}

fn hash2(x: f32, y: f32) -> f32 {
  return fract(sin(x*127.1 + y*311.7)*43758.5453);
}

fn noise2(x: f32, y: f32) -> f32 {
  let xi = floor(x); let yi = floor(y);
  let xf = x-xi; let yf = y-yi;
  let u = xf*xf*(3.0-2.0*xf);
  let v = yf*yf*(3.0-2.0*yf);
  return mix(mix(hash2(xi,yi), hash2(xi+1.0,yi), u),
             mix(hash2(xi,yi+1.0), hash2(xi+1.0,yi+1.0), u), v);
}

fn fbm(x: f32, y: f32) -> f32 {
  var v = 0.0; var a = 0.5;
  var xx = x; var yy = y;
  for (var i = 0; i < 3; i++) {
    v += a * noise2(xx, yy);
    xx *= 2.1; yy *= 2.1; a *= 0.5;
  }
  return v;
}

fn sample_field(flat: f32, px: f32, pz: f32) -> vec2<f32> {
  if flat < P.disk_in || flat > P.disk_out {
    return vec2<f32>(0.0);
  }

  let drf = f32(P.dr);
  let daf = f32(P.da);

  let fr = ((flat - P.disk_in) / (P.disk_out - P.disk_in)) * drf;
  let r0 = min(u32(floor(fr)), P.dr - 1u);
  let r1 = min(r0 + 1u, P.dr - 1u);
  let tr = fract(fr);

  let ang = atan2(pz, px) + 3.14159265;
  let fa = (ang / (2.0 * 3.14159265)) * daf;

  let a0 = u32(floor(fa)) % P.da;
  let a1 = (a0 + 1u) % P.da;
  let ta = fract(fa);

  let i00 = r0 * P.da + a0;
  let i10 = r1 * P.da + a0;
  let i01 = r0 * P.da + a1;
  let i11 = r1 * P.da + a1;

  let offset = P.dr * P.da;

  let d00 = field[i00];
  let d10 = field[i10];
  let d01 = field[i01];
  let d11 = field[i11];

  let v00 = field[i00 + offset];
  let v10 = field[i10 + offset];
  let v01 = field[i01 + offset];
  let v11 = field[i11 + offset];

  let d0 = mix(d00, d10, tr);
  let d1 = mix(d01, d11, tr);
  let density = mix(d0, d1, ta);

  let v0 = mix(v00, v10, tr);
  let v1 = mix(v01, v11, tr);
  let vtan = mix(v0, v1, ta);

  return vec2<f32>(density, vtan);
}

fn planck_sample(wl: f32, temp: f32) -> f32 {
  let x = 0.014388/(wl*temp);
  if x > 500.0 { return 0.0; }
  if x < 1e-4 { return pow(wl,-4.0)*temp; }
  return pow(wl,-5.0)/(exp(x)-1.0);
}

fn planck_rgb(temp: f32) -> vec3<f32> {
  if temp < 100.0 { return vec3<f32>(0.0); }
  let r = planck_sample(700e-9, temp);
  let g = planck_sample(546e-9, temp);
  let b = planck_sample(435e-9, temp);
  let m = max(max(r,g), max(b,1e-30));
  return vec3<f32>(r/m, g/m, b/m);
}

fn star_layer(dir: vec3<f32>, scale: f32, threshold: f32, intensity: f32) -> vec3<f32> {
  let p = dir * scale;
  let cell = floor(p);
  let local = fract(p) - vec3<f32>(0.5, 0.5, 0.5);

  let h = hash3(cell);
  if h < threshold {
    return vec3<f32>(0.0);
  }

  let center = vec3<f32>(
    hash3(cell + vec3<f32>(1.3, 0.0, 0.0)) - 0.5,
    hash3(cell + vec3<f32>(0.0, 2.1, 0.0)) - 0.5,
    hash3(cell + vec3<f32>(0.0, 0.0, 3.7)) - 0.5
  ) * 0.7;

  let d = length(local - center);
  let glow = exp(-d * d * 90.0) * intensity;
  let tint = 0.8 + 0.4 * hash3(cell + vec3<f32>(9.1, 4.7, 2.3));

  return vec3<f32>(
    glow * tint,
    glow * (0.9 + 0.1 * tint),
    glow
  );
}

fn stars(dir: vec3<f32>) -> vec3<f32> {
  let d = make_unit(dir);
  var col = vec3<f32>(0.0);

  col += star_layer(d, 180.0, 0.975, 0.12);
  col += star_layer(d, 260.0, 0.982, 0.18);

  col += star_layer(d, 420.0, 0.993, 0.65);
  col += star_layer(d, 700.0, 0.9975, 1.45);

  col += star_layer(d, 1100.0, 0.9990, 2.80);

  let lat = asin(clamp(d.y, -1.0, 1.0));
  let lon = atan2(d.z, d.x);

  let dust_noise = 0.5 * noise2(lon * 2.0, lat * 6.0)
                 + 0.5 * noise2(lon * 6.0, lat * 14.0);

  let dust = 0.004 + 0.004 * dust_noise;
  col += vec3<f32>(dust * 0.55, dust * 0.60, dust * 0.78);

  let band_noise = 0.6 * noise2(lon * 3.0, lat * 8.0)
                 + 0.4 * noise2(lon * 7.0, lat * 17.0);

  let band_shape = exp(-lat * lat * 14.0);
  let band_ripple = 0.85
    + 0.24 * sin(lon * 2.6 - 1.1)
    + 0.11 * sin(lon * 5.8 + 0.4)
    + 0.05 * sin(lon * 11.0 - 0.7);

  let band = band_shape * (0.010 + 0.010 * band_noise) * band_ripple;
  col += vec3<f32>(band * 0.55, band * 0.60, band * 0.85);

  return col;
}

fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
  let flat = sqrt(pos.x*pos.x + pos.z*pos.z);

  let h = pos.y / (flat * P.ad_height);
  let vert = exp(-h*h * P.ad_v);
  if vert < 0.001 { return vec3<f32>(0.0); }

  let rnorm = (flat - P.disk_in) / (P.disk_out - P.disk_in);
  let horiz = max(pow(1.0-rnorm, P.ad_h*0.72) * pow(rnorm+0.05, 0.17), 0.0);
  if horiz < 0.001 { return vec3<f32>(0.0); }

  let band = 1.0 + 0.18 * sin(flat * 2.8 - 1.2) + 0.08 * sin(flat * 6.1);

  let ang = atan2(pos.z, pos.x);
  let n = clamp(fbm(flat*P.ad_noise, ang*P.ad_noise*0.5), 0.0, 1.0);

  let sv = sample_field(flat, pos.x, pos.z);
  let dcell = sv.x;
  let vtan = sv.y;

  let dm = max(P.density_max, 1.0);
  let micro = 0.992 + 0.016 * hash2(pos.x * 16.0, pos.z * 16.0);
  let density = vert * horiz * band * (0.35 + 0.65*n) * clamp(dcell/dm, 0.0, 1.0) * micro;

  let ratio = flat / P.disk_in;
  let heat = P.disk_heat
    * pow(max(1.0 - pow(ratio, -0.5), 0.0), 0.25)
    * pow(ratio, -0.75);
  if heat < 200.0 { return vec3<f32>(0.0); }

  let spin = make_unit(vec3<f32>(-pos.z, 0.0, pos.x));
  let to_cam = make_unit(-dir);
  let speed = min(abs(vtan), 0.72);
  let beta = -dot(spin, to_cam) * sign(vtan) * speed;
  let gamma = 1.0 / sqrt(max(1.0 - speed*speed, 0.001));
  let dop = 1.0 / max(gamma*(1.0-beta), 0.01);

  var red_boost = 1.0;
  if dop < 1.0 {
    red_boost = 1.0 + clamp((1.0 - dop) * 0.8, 0.0, 0.55);
  }

  let beam = min(pow(dop, 2.8), 2.6);
  let grav = pow(max(1.0 - P.bh_size/flat, 0.01), 0.35);
  let seen = heat * dop * grav;

  let disk_normal = vec3<f32>(0.0, 1.0, 0.0);
  let view_angle = abs(dot(disk_normal, dir));
  let limb = pow(1.0 - view_angle, 0.6) + 0.4;

  var bright =
      density
    * pow(clamp(seen / P.disk_heat, 0.0, 1.2), 1.1)
    * beam
    * P.ad_lit
    * red_boost
    * limb;

  let ring_soft = smoothstep(P.bh_size * 1.04, P.bh_size * 1.34, flat);
  bright *= ring_soft;

  return planck_rgb(seen) * bright;
}

fn make_camera_ray(px: f32, py: f32) -> vec3<f32> {
  let half = tan(radians(P.fov)*0.5);
  let aspect = f32(P.w)/f32(P.h);
  let sx = (px/f32(P.w)*2.0 - 1.0)*half*aspect;
  let sy = (1.0 - py/f32(P.h)*2.0)*half;
  let fwd = make_unit(P.cam_look.xyz - P.cam_pos.xyz);
let world_up = vec3<f32>(0.0,1.0,0.0);

var right = make_unit(cross(fwd, world_up));
var up = cross(right, fwd);

// apply camera roll
let r = radians(P.roll);
let cr = cos(r);
let sr = sin(r);

let right2 = right * cr + up * sr;
let up2 = -right * sr + up * cr;

right = right2;
up = up2;
  return make_unit(right*sx + up*sy + fwd);
}

fn march(px: u32, py: u32, sx: u32, sy: u32) -> vec3<f32> {
  let j = rand2(f32(px)*13.0 + f32(sx), f32(py)*17.0 + f32(sy));

  let fx = f32(px) + (f32(sx) + j.x) / f32(P.ss);
  let fy = f32(py) + (f32(sy) + j.y) / f32(P.ss);

  var pos = P.cam_pos.xyz;
  var dir = make_camera_ray(fx, fy);
  var col = vec3<f32>(0.0);
  var swirl = 0.0;

  for (var i: u32 = 0u; i < P.max_steps; i++) {
    let dist = length(pos);

    if dist < P.bh_size * 0.97 {
      return col;
    }

    if dist > 120.0 {
      return col + stars(dir);
    }

    let flat = sqrt(pos.x*pos.x + pos.z*pos.z);

    var step = 0.02 + dist * 0.01;
    step = min(step, max(0.00012, (dist - P.bh_size) * 0.02));

    if flat > P.disk_in - 1.0 && flat < P.disk_out + 1.0 {
      let disk_half = max(flat * P.ad_height * 2.8, 0.02);
      let plane_dist = abs(pos.y);

      let plane_factor = clamp(plane_dist / (disk_half * 3.0), 0.15, 1.0);
      step *= plane_factor;

      let inner_dist = abs(flat - P.disk_in);
      let ring_factor = clamp(inner_dist * 3.0, 0.18, 1.0);
      step *= ring_factor;
    }

    let bend_scale = 1.5 * P.bh_size / max(dist * dist, 0.0001);
    step *= clamp(1.0 / (1.0 + bend_scale * 40.0), 0.12, 1.0);
    step = clamp(step, 0.00008, 0.05);

    let tc = pos / dist;
    let sideways = dir - tc * dot(dir, tc);

    // NOTE:
    // this is the fake-GR bend term.
    // extremely load-bearing.
    // touching it changes the entire lensing vibe instantly.
    let bend = 1.5 * P.bh_size / (dist * dist);
    dir = make_unit(dir + sideways * (-bend * step));
    swirl += bend * step;

    if flat > P.disk_in && flat < P.disk_out {
      col = min(col + disk_color(pos, dir), vec3<f32>(10.0));
    }

    pos += dir * step;
  }

  return col;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  if gid.x >= P.w || gid.y >= P.h { return; }

  var acc = vec3<f32>(0.0);
  for (var sy: u32 = 0u; sy < P.ss; sy++) {
    for (var sx: u32 = 0u; sx < P.ss; sx++) {
      acc += march(gid.x, gid.y, sx, sy);
    }
  }

  acc = acc / f32(P.ss * P.ss);
  out[gid.y * P.w + gid.x] = vec4<f32>(acc, 1.0);
}
"#;

const SHADER_LDR: &str = r#"
struct Params {
  w: u32, h: u32, ss: u32, _pad0: u32,
  dr: u32, da: u32, _padx0: u32, _padx1: u32,

  bh_size: f32, bh_mass: f32, disk_in: f32, disk_out: f32,
  disk_heat: f32, density_max: f32, ad_v: f32, ad_h: f32,
  ad_height: f32, ad_lit: f32, ad_noise: f32, exposure: f32,

  max_steps: u32, _pad1a: u32, _pad1b: u32, _pad1c: u32,

  cam_pos: vec4<f32>,
  cam_look: vec4<f32>,

  fov: f32, ring_zone: f32, roll: f32, _pad2a: f32,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> field: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<u32>;

fn make_unit(v: vec3<f32>) -> vec3<f32> {
  let l = length(v);
  if l < 1e-12 { return vec3<f32>(0.0, 0.0, 1.0); }
  return v / l;
}

fn filmic(x: f32) -> f32 {
  let x2 = max(x - 0.004, 0.0);
  return (x2 * (6.2*x2 + 0.5)) / (x2 * (6.2*x2 + 1.7) + 0.06);
}

fn rand2(x: f32, y: f32) -> vec2<f32> {
  let a = fract(sin(x*127.1 + y*311.7) * 43758.5453);
  let b = fract(sin(x*269.5 + y*183.3) * 43758.5453);
  return vec2<f32>(a, b);
}

fn hash3(p: vec3<f32>) -> f32 {
  var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
  q += dot(q, q.yzx + 19.19);
  return fract((q.x + q.y) * q.z);
}

fn hash2(x: f32, y: f32) -> f32 {
  return fract(sin(x*127.1 + y*311.7)*43758.5453);
}

fn noise2(x: f32, y: f32) -> f32 {
  let xi = floor(x); let yi = floor(y);
  let xf = x-xi; let yf = y-yi;
  let u = xf*xf*(3.0-2.0*xf);
  let v = yf*yf*(3.0-2.0*yf);
  return mix(mix(hash2(xi,yi), hash2(xi+1.0,yi), u),
             mix(hash2(xi,yi+1.0), hash2(xi+1.0,yi+1.0), u), v);
}

fn fbm(x: f32, y: f32) -> f32 {
  var v = 0.0; var a = 0.5;
  var xx = x; var yy = y;
  for (var i = 0; i < 3; i++) {
    v += a * noise2(xx, yy);
    xx *= 2.1; yy *= 2.1; a *= 0.5;
  }
  return v;
}

fn sample_field(flat: f32, px: f32, pz: f32) -> vec2<f32> {
  if flat < P.disk_in || flat > P.disk_out {
    return vec2<f32>(0.0);
  }

  let drf = f32(P.dr);
  let daf = f32(P.da);

  let fr = ((flat - P.disk_in) / (P.disk_out - P.disk_in)) * drf;
  let r0 = min(u32(floor(fr)), P.dr - 1u);
  let r1 = min(r0 + 1u, P.dr - 1u);
  let tr = fract(fr);

  let ang = atan2(pz, px) + 3.14159265;
  let fa = (ang / (2.0 * 3.14159265)) * daf;

  let a0 = u32(floor(fa)) % P.da;
  let a1 = (a0 + 1u) % P.da;
  let ta = fract(fa);

  let i00 = r0 * P.da + a0;
  let i10 = r1 * P.da + a0;
  let i01 = r0 * P.da + a1;
  let i11 = r1 * P.da + a1;

  let offset = P.dr * P.da;

  let d00 = field[i00];
  let d10 = field[i10];
  let d01 = field[i01];
  let d11 = field[i11];

  let v00 = field[i00 + offset];
  let v10 = field[i10 + offset];
  let v01 = field[i01 + offset];
  let v11 = field[i11 + offset];

  let d0 = mix(d00, d10, tr);
  let d1 = mix(d01, d11, tr);
  let density = mix(d0, d1, ta);

  let v0 = mix(v00, v10, tr);
  let v1 = mix(v01, v11, tr);
  let vtan = mix(v0, v1, ta);

  return vec2<f32>(density, vtan);
}

fn planck_sample(wl: f32, temp: f32) -> f32 {
  let x = 0.014388/(wl*temp);
  if x > 500.0 { return 0.0; }
  if x < 1e-4 { return pow(wl,-4.0)*temp; }
  return pow(wl,-5.0)/(exp(x)-1.0);
}

fn planck_rgb(temp: f32) -> vec3<f32> {
  if temp < 100.0 { return vec3<f32>(0.0); }
  let r = planck_sample(700e-9, temp);
  let g = planck_sample(546e-9, temp);
  let b = planck_sample(435e-9, temp);
  let m = max(max(r,g), max(b,1e-30));
  return vec3<f32>(r/m, g/m, b/m);
}

fn star_layer(dir: vec3<f32>, scale: f32, threshold: f32, intensity: f32) -> vec3<f32> {
  let p = dir * scale;
  let cell = floor(p);
  let local = fract(p) - vec3<f32>(0.5, 0.5, 0.5);

  let h = hash3(cell);
  if h < threshold {
    return vec3<f32>(0.0);
  }

  let center = vec3<f32>(
    hash3(cell + vec3<f32>(1.3, 0.0, 0.0)) - 0.5,
    hash3(cell + vec3<f32>(0.0, 2.1, 0.0)) - 0.5,
    hash3(cell + vec3<f32>(0.0, 0.0, 3.7)) - 0.5
  ) * 0.7;

  let d = length(local - center);
  let glow = exp(-d * d * 90.0) * intensity;
  let tint = 0.8 + 0.4 * hash3(cell + vec3<f32>(9.1, 4.7, 2.3));

  return vec3<f32>(
    glow * tint,
    glow * (0.9 + 0.1 * tint),
    glow
  );
}

fn stars(dir: vec3<f32>) -> vec3<f32> {
  let d = make_unit(dir);
  var col = vec3<f32>(0.0);

  col += star_layer(d, 180.0, 0.975, 0.12);
  col += star_layer(d, 260.0, 0.982, 0.18);

  col += star_layer(d, 420.0, 0.993, 0.65);
  col += star_layer(d, 700.0, 0.9975, 1.45);

  col += star_layer(d, 1100.0, 0.9990, 2.80);

  let lat = asin(clamp(d.y, -1.0, 1.0));
  let lon = atan2(d.z, d.x);

  let dust_noise = 0.5 * noise2(lon * 2.0, lat * 6.0)
                 + 0.5 * noise2(lon * 6.0, lat * 14.0);

  let dust = 0.004 + 0.004 * dust_noise;
  col += vec3<f32>(dust * 0.55, dust * 0.60, dust * 0.78);

  let band_noise = 0.6 * noise2(lon * 3.0, lat * 8.0)
                 + 0.4 * noise2(lon * 7.0, lat * 17.0);

  let band_shape = exp(-lat * lat * 14.0);
  let band_ripple = 0.85
    + 0.24 * sin(lon * 2.6 - 1.1)
    + 0.11 * sin(lon * 5.8 + 0.4)
    + 0.05 * sin(lon * 11.0 - 0.7);

  let band = band_shape * (0.010 + 0.010 * band_noise) * band_ripple;
  col += vec3<f32>(band * 0.55, band * 0.60, band * 0.85);

  return col;
}

fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
  let flat = sqrt(pos.x*pos.x + pos.z*pos.z);

  let h = pos.y / (flat * P.ad_height);
  let vert = exp(-h*h * P.ad_v);
  if vert < 0.001 { return vec3<f32>(0.0); }

  let rnorm = (flat - P.disk_in) / (P.disk_out - P.disk_in);
  let horiz = max(pow(1.0-rnorm, P.ad_h*0.72) * pow(rnorm+0.05, 0.17), 0.0);
  if horiz < 0.001 { return vec3<f32>(0.0); }

  let band = 1.0 + 0.18 * sin(flat * 2.8 - 1.2) + 0.08 * sin(flat * 6.1);

  let ang = atan2(pos.z, pos.x);
  let n = clamp(fbm(flat*P.ad_noise, ang*P.ad_noise*0.5), 0.0, 1.0);

  let sv = sample_field(flat, pos.x, pos.z);
  let dcell = sv.x;
  let vtan = sv.y;

  let dm = max(P.density_max, 1.0);
  let micro = 0.992 + 0.016 * hash2(pos.x * 16.0, pos.z * 16.0);
  let density = vert * horiz * band * (0.35 + 0.65*n) * clamp(dcell/dm, 0.0, 1.0) * micro;

  let ratio = flat / P.disk_in;
  let heat = P.disk_heat
    * pow(max(1.0 - pow(ratio, -0.5), 0.0), 0.25)
    * pow(ratio, -0.75);
  if heat < 200.0 { return vec3<f32>(0.0); }

  let spin = make_unit(vec3<f32>(-pos.z, 0.0, pos.x));
  let to_cam = make_unit(-dir);
  let speed = min(abs(vtan), 0.72);
  let beta = -dot(spin, to_cam) * sign(vtan) * speed;
  let gamma = 1.0 / sqrt(max(1.0 - speed*speed, 0.001));
  let dop = 1.0 / max(gamma*(1.0-beta), 0.01);

  var red_boost = 1.0;
  if dop < 1.0 {
    red_boost = 1.0 + clamp((1.0 - dop) * 0.8, 0.0, 0.55);
  }

  let beam = min(pow(dop, 2.8), 2.6);
  let grav = pow(max(1.0 - P.bh_size/flat, 0.01), 0.35);
  let seen = heat * dop * grav;

  let disk_normal = vec3<f32>(0.0, 1.0, 0.0);
  let view_angle = abs(dot(disk_normal, dir));
  let limb = pow(1.0 - view_angle, 0.6) + 0.4;

  var bright =
      density
    * pow(clamp(seen / P.disk_heat, 0.0, 1.2), 1.1)
    * beam
    * P.ad_lit
    * red_boost
    * limb;

  let ring_soft = smoothstep(P.bh_size * 1.04, P.bh_size * 1.34, flat);
  bright *= ring_soft;

  return planck_rgb(seen) * bright;
}

fn make_camera_ray(px: f32, py: f32) -> vec3<f32> {
  let half = tan(radians(P.fov)*0.5);
  let aspect = f32(P.w)/f32(P.h);
  let sx = (px/f32(P.w)*2.0 - 1.0)*half*aspect;
  let sy = (1.0 - py/f32(P.h)*2.0)*half;

  let fwd = make_unit(P.cam_look.xyz - P.cam_pos.xyz);
  let world_up = vec3<f32>(0.0, 1.0, 0.0);

  var right = make_unit(cross(fwd, world_up));
  var up = cross(right, fwd);

  let r = radians(P.roll);
  let cr = cos(r);
  let sr = sin(r);

  let right2 = right * cr + up * sr;
  let up2 = -right * sr + up * cr;

  right = right2;
  up = up2;

  return make_unit(right*sx + up*sy + fwd);
}

fn march(px: u32, py: u32, sx: u32, sy: u32) -> vec3<f32> {
  let j = rand2(f32(px)*13.0 + f32(sx), f32(py)*17.0 + f32(sy));

  let fx = f32(px) + (f32(sx) + j.x) / f32(P.ss);
  let fy = f32(py) + (f32(sy) + j.y) / f32(P.ss);

  var pos = P.cam_pos.xyz;
  var dir = make_camera_ray(fx, fy);
  var col = vec3<f32>(0.0);
  var swirl = 0.0;

  for (var i: u32 = 0u; i < P.max_steps; i++) {
    let dist = length(pos);

    if dist < P.bh_size * 0.97 {
      return col;
    }

    if dist > 120.0 {
      return col + stars(dir);
    }

    let flat = sqrt(pos.x*pos.x + pos.z*pos.z);

    var step = 0.02 + dist * 0.01;
    step = min(step, max(0.00012, (dist - P.bh_size) * 0.02));

    if flat > P.disk_in - 1.0 && flat < P.disk_out + 1.0 {
      let disk_half = max(flat * P.ad_height * 2.8, 0.02);
      let plane_dist = abs(pos.y);

      let plane_factor = clamp(plane_dist / (disk_half * 3.0), 0.15, 1.0);
      step *= plane_factor;

      let inner_dist = abs(flat - P.disk_in);
      let ring_factor = clamp(inner_dist * 3.0, 0.18, 1.0);
      step *= ring_factor;
    }

    let bend_scale = 1.5 * P.bh_size / max(dist * dist, 0.0001);
    step *= clamp(1.0 / (1.0 + bend_scale * 40.0), 0.12, 1.0);
    step = clamp(step, 0.00008, 0.05);

    let tc = pos / dist;
    let sideways = dir - tc * dot(dir, tc);
    let bend = 1.5 * P.bh_size / (dist * dist);
    dir = make_unit(dir + sideways * (-bend * step));
    swirl += bend * step;

    if flat > P.disk_in && flat < P.disk_out {
      col = min(col + disk_color(pos, dir), vec3<f32>(10.0));
    }

    pos += dir * step;
  }

  return col;
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  if gid.x >= P.w || gid.y >= P.h { return; }

  var acc = vec3<f32>(0.0);
  for (var sy: u32 = 0u; sy < P.ss; sy++) {
    for (var sx: u32 = 0u; sx < P.ss; sx++) {
      acc += march(gid.x, gid.y, sx, sy);
    }
  }

  acc = acc / f32(P.ss * P.ss);
  acc = acc * P.exposure;
  acc = acc / (1.0 + acc * 1.3);

  let rr = u32(clamp(filmic(acc.x), 0.0, 1.0) * 255.0);
  let gg = u32(clamp(filmic(acc.y), 0.0, 1.0) * 255.0);
  let bb = u32(clamp(filmic(acc.z), 0.0, 1.0) * 255.0);

  out[gid.y * P.w + gid.x] = rr | (gg << 8u) | (bb << 16u) | (255u << 24u);
}
"#;
