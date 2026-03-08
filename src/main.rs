//! COSMIC PASTA SIMULATOR
//!
//! featuring:
//! millions of screaming noodles
//! illegal gravity seasoning
//! photon harassment
//! accretion disk marinara under active investigation
//!
//! what this cursed thing does:
//!
//! - summon a stupid amount of tiny orbital noodles
//! - fling them toward an Angry Gravity Marble
//! - force them through questionable relativistic pasta rituals
//! - let the marble fold spacetime into a glowing spaghetti disaster
//! - bake the surviving noodle chaos into a density field
//! - ray-march photons through the void until they confess a picture
//! - dump the result to disk and pretend any of this was under control
//!
//! approved, stamped, or reluctantly tolerated by:
//!
//! - the Δt Scheduling Clerk
//! - the β-Limit Enforcement Office
//! - the Relativistic Time Dilation Accountant
//! - the Orbital Spin Registrar
//! - the Radiative Spectrum Curator
//! - the Cosmic Fat Inspector
//! - the Orbit Radius Cartographer
//! - the Pixel Budget Inspector
//!
//! WARNING:
//! some of this is real physics.
//! some of this is pasta sorcery with paperwork.
//! somewhere between those two lives the renderer.
//!
//! if it breaks: spacetime moved.
//! if it works: leave the haunted constants alone.
//! if you increase the resolution like an idiot: expect a visit from the Pixel Budget Inspector.

use bytemuck::{Pod, Zeroable};
use glam::DVec3;
use image::{ImageBuffer, Rgb};
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;
use std::env;
use std::f64::consts::PI;
use wgpu::util::DeviceExt;

// NOTE:
// SETTINGS TWEAK AS YOU WISH
// most important knobs first.
// if the render looks cursed, start debugging up here.

/// NOTE: Anti-aliasing / supersampling.
/// 2 = sane
/// 3 = very clean
/// 4 = GPU starts sweating
const SS: u32 = 4; // SS4 goes hard

/// NOTE: how many times each photon pokes spacetime.
/// higher = more accuracy near the angry marble.
const MAX_STEPS: u32 = 50_000; // NOTE: 50k is a good default.

/// NOTE:
// main render resolution.
/// bigger = prettier marble but slower.
const W: u32 = 7680; // NOTE: use 7680x4320 for good marble. gpu gets bullied though :3
const H: u32 = 4320; // NOTE: use 3840x2160 for faster render

/// NOTE:
/// HDR / EXR render resolution.
/// if you push this too high wgpu will start yelling about buffer sizes.
const HDR_W: u32 = 3840; // WARN: DO NOT GO ABOVE 3840 or you anger the Pixel Budget Inspector :3
const HDR_H: u32 = 2160; // WARN: DO NOT GO ABOVE 2160 or you anger the Pixel Budget Inspector :3

/// NOTE:
/// donut pixels (grid of suffering)
/// this controls how detailed the accretion disk density field is.
const DR: usize = 768; // NOTE: 256x1024 for fast render
const DA: usize = 3072; // NOTE: 640x2560 for good sauce
// NOTE: or try 768x3072 idfk cosmic winds decide this i suppose.

// NOTE:
// end of actually important settings.
// everything below here is mostly vibe tuning.

/// NOTE: black hole properties.
/// NOTE: 2.0 is "realistic" don't trust me
const BH_MASS: f64 = 2.0;
/// NOTE: radius of nml aka don't pet the marble
const BH_SIZE: f64 = 1.5;
// NOTE: put BH_MASS 2.5+ and BH_SIZE 1.0+ for more cinematic evil marble

/// NOTE: cursed pasta ring bounds
/// NOTE: 1.2 looks nice, and is believable
const DISK_IN: f64 = 1.2;
/// NOTE: 12.0 also looks believable
const DISK_OUT: f64 = 15.0;
/// NOTE: cosmic speed limits. don't let beta be over 0.98 pls
const DISK_BOOST: f64 = 1.5;

/// NOTE:
/// disk appearance sliders
/// these were tuned until the disk stopped looking like wet lint.
/// NOTE: glow spell intensity
const DISK_HEAT: f64 = 18000.0;
/// NOTE: vertical squish curse
const ADISK_DENSITY_V: f64 = 3.2;
/// NOTE: horizontal fade chant
const ADISK_DENSITY_H: f64 = 2.4;
/// NOTE: cosmic crepe thickness (must stay thin)
const ADISK_HEIGHT: f64 = 0.014;
/// NOTE: "pls show up on screen" rune
const ADISK_LIT: f64 = 2.5;
/// NOTE: turbulence dial (fake trauma texture)
const ADISK_NOISE_SCALE: f64 = 2.2;

/// NOTE:
/// camera placement
/// move this if you want to film the pasta ritual from another angle.
/// NOTE: 0.0, 2.2, -14.0 is my defaults
const CAM_POS: DVec3 = DVec3::new(0.0, 0.55, -20.0);
/// NOTE: 0.0, 0.0, 0.0 = looking directly at marble
const CAM_LOOK: DVec3 = DVec3::new(0.15, 0.03, 0.0);
// NOTE: degrees how drunk the cameraman is
const CAM_ROLL: f64 = -10.0;
// NOTE: how far u watching the emo marble from
const FOV: f64 = 65.0;

/// NOTE:
/// preview brightness for PNG output
const EXPOSURE: f64 = 0.46; // when the void is too emo

/// XXX:
/// ray march helper knob.
/// currently doesn't do much unless stepping logic changes.
const RING_ZONE: f64 = 1.0; // XXX: arbitrary numbers that barely does anything

/// PERF:
/// number of particles in the disk simulation.
/// higher = smoother density field but slower CPU phase.
const ROCKS: usize = 3_000_000; // NOTE: honestly 8mil is fine... 3mil if want faster

/// PERF:
/// how long the particle simulation runs.
const STEPS: usize = 4_000; // NOTE: 4k is pretty default. honestly no need to touch

/// NOTE:
/// simulation timestep.
/// small so the universe doesn't instantly file a complaint.
/// this should roughly match comp_dt(), but is pinned here as a tuned constant.
///
/// XXX:
/// this is manually pinned instead of using comp_dt() directly.
/// future me can decide whether that was wisdom or pasta poisoning :3
const DT: f64 = 0.0020;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Png,
    Tiff,
    Exr,
    All,
}

/// WARN:
/// output mode decides whether we go through the huge HDR path or the lighter LDR path.
/// if png accidentally starts using hdr again, render time and memory usage both get cursed.
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

/// HACK:
/// estimates a usable Δt from one orbital period near the angry marble
/// not sacred physics. just a cursed spacing for the sim steps
fn comp_dt() -> f64 {
    let r = BH_SIZE;
    let m = BH_MASS;

    // NOTE:
    // orbital period from Kepler-ish relation:
    // T = 2π * sqrt(r³ / M)
    let op = 2.0 * PI * ((r * r * r) / m).sqrt();
    op / STEPS as f64
}

/// INFO:
/// calculates angular velocity Ω for a roughly circular orbit in the disk
/// aka how fast the cosmic DVD logo spins around the void
fn comp_omega() -> f64 {
    let r = (DISK_IN + DISK_OUT) * 0.5;

    // Ω = sqrt(M / r³)
    ((BH_MASS / (r * r * r)).sqrt() * DISK_BOOST)
}

/// XXX:
/// helper rune to spew out φ
/// basically just π here. emotional support constant
fn comp_phi() -> f64 {
    std::f64::consts::PI
}

/// XXX:
/// calculates a fake average wavelength λ for RGB-ish suffering
/// not deeply useful right now, just here for vibes
fn comp_lambda() -> f64 {
    (700.0 + 532.0 + 435.0) / 3.0
}

/// INFO:
/// calculates β = v/c for a typical disk orbit
/// in this cursed unit system c = 1, so β is basically just the speed
/// capped below 1 so relativity doesn't call the cops
fn comp_beta() -> f64 {
    let r = (DISK_IN + DISK_OUT) * 0.5;

    // circular orbit speed ≈ sqrt(M / r)
    ((BH_MASS / r).sqrt() * DISK_BOOST).min(0.999)
}

/// INFO:
/// calculates the Lorentz gamma factor from β
/// this is the relativity tax that shows up in the WGSL voodoo later
fn comp_gamma() -> f64 {
    let beta = comp_beta();

    // γ = 1 / sqrt(1 - β²)
    1.0 / (1.0 - beta * beta).sqrt()
}

/// NOTE:
/// filmic tone-mapping curve.
/// compresses HDR brightness into something a normal monitor can survive.
fn filmic(x: f32) -> f32 {
    // NOTE:
    // tiny offset removes near-black noise before the curve
    let x2 = (x - 0.004).max(0.0);

    // HACK:
    // these cursed constants approximate a filmic response curve.
    // nobody remembers them, we just trust the cinema gods.
    (x2 * (6.2 * x2 + 0.5)) / (x2 * (6.2 * x2 + 1.7) + 0.06)
}

/// XXX:
/// currently unused.
/// cpu hdr -> png path from the old ritual.
/// safe to delete unless the renderer goes back to cpu previews.
///
/// PERF:
/// walks every pixel. fine for small previews.
/// if you feed it a giant hdr buffer it will absolutely complain.
fn hdr_to_u8_image(rgb: &[f32], w: u32, h: u32, exposure: f32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 3) as usize;

            let mut r = rgb[i] * exposure;
            let mut g = rgb[i + 1] * exposure;
            let mut b = rgb[i + 2] * exposure;

            // NOTE:
            // squeeze the hdr a bit so filmic doesn't explode
            r = r / (1.0 + r * 1.3);
            g = g / (1.0 + g * 1.3);
            b = b / (1.0 + b * 1.3);

            // NOTE:
            // filmic tone-map then crush into 8-bit sadness
            let rr = (filmic(r).clamp(0.0, 1.0) * 255.0) as u8;
            let gg = (filmic(g).clamp(0.0, 1.0) * 255.0) as u8;
            let bb = (filmic(b).clamp(0.0, 1.0) * 255.0) as u8;

            img.put_pixel(x, y, Rgb([rr, gg, bb]));
        }
    }

    img
}

/// NOTE:
/// tiff is intentionally not raw exr-style hdr.
/// this does a softer hdr squeeze so highlights survive
/// without going full void goblin like exr does.
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

            // NOTE:
            // gentle hdr squeeze before filmic
            r = r / (1.0 + r * 0.45);
            g = g / (1.0 + g * 0.45);
            b = b / (1.0 + b * 0.45);

            // NOTE:
            // tone-map then pack into 16-bit suffering
            let rr = (filmic(r).clamp(0.0, 1.0) * 65535.0) as u16;
            let gg = (filmic(g).clamp(0.0, 1.0) * 65535.0) as u16;
            let bb = (filmic(b).clamp(0.0, 1.0) * 65535.0) as u16;

            img.put_pixel(x, y, Rgb([rr, gg, bb]));
        }
    }

    img
}

/// PERF:
/// dumb but reliable box downscale.
/// walks a bunch of pixels and averages them.
/// large hdr buffers will absolutely feel this.
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

/// NOTE:
/// bloom is only applied to the ldr preview path right now.
/// exr stays raw, tiff stays softly squeezed, png gets the pretty lying filter.
fn apply_bloom_u8(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>) {
    use image::imageops;

    let mut bright = img.clone();

    // extract bright pixels
    for p in bright.pixels_mut() {
        let v = (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0;
        if v < 185.0 {
            *p = image::Rgb([0, 0, 0]);
        }
    }

    // blur them
    let glow = imageops::blur(&bright, 7.0);

    // add glow back
    for (base, g) in img.pixels_mut().zip(glow.pixels()) {
        base[0] = base[0].saturating_add((g[0] as f32 * 0.30) as u8);
        base[1] = base[1].saturating_add((g[1] as f32 * 0.30) as u8);
        base[2] = base[2].saturating_add((g[2] as f32 * 0.30) as u8);
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

/// NOTE:
/// main ritual:
/// print cursed diagnostics -> simulate noodles -> bake field -> render chosen output path.
fn main() {
    println!("Δt = {:.4}", comp_dt());
    println!("Ω = {:.4}", comp_omega());
    println!("β = {:.4}", comp_beta());
    println!("γ = {:.4}", comp_gamma());
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

    // WARN:
    // png should stay on the ldr path.
    // if this starts calling hdr again, memory use goes nuclear for no reason.
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

    // WARN:
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

/// NOTE:
/// summon the accretion disk noodles.
/// creates orbital grains in a cursed donut around the angry marble.
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
        // circular orbit condition: v^2 / r = M / r^2
        // solve for v -> v = sqrt(M / r)
        let v_circ = (BH_MASS / r).sqrt() * DISK_BOOST;

        // wobble: inject chaos, as a treat
        let wobble = (rng.random::<f64>() - 0.5) * v_circ * 0.04;

        // tangent velocity so it spins like it’s possessed
        let vel = DVec3::new(
            a.sin() * (v_circ + wobble),
            (rng.random::<f64>() - 0.5) * 0.008,
            -a.cos() * (v_circ + wobble),
        );

        out.push(Grain {
            pos,
            vel,
            alive: true, // optimistic. foolish.
        });
    }

    out
}

/// NOTE:
/// spin the noodle swarm around the angry marble.
/// this is the main particle simulation step.
fn let_the_idiots_spin(grains: &mut Vec<Grain>, steps: usize) {
    let report_every = steps / 10;

    // PERF:
    // this is the main cpu bully.
    // millions of little shits times thousands of steps.
    // no wonder tax evading is a thing.
    for step in 0..steps {
        if report_every > 0 && step % report_every == 0 {
            println!(
                "  chant {}/{} alive={}",
                step,
                steps,
                grains.iter().filter(|g| g.alive).count()
            );
        }

        // PERF:
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

            // FIXME:
            // gravity hex (not real GR, just summoning the vibe of it)
            // newtonian gravity with a spicy inner boost.
            // a_vec = -(M / r^3) * r_vec
            let inv_r3 = 1.0 / (r2 * r);

            // HACK:
            // pseudo-GR correction
            // this term is literally "i whispered general relativity at newton"
            let grav = g.pos * (-BH_MASS * inv_r3 * (1.0 + 3.0 * BH_MASS / r));

            // HACK:
            // tiny damping so noodles don't become confetti
            // duct tape. not sacred astrophysics.
            let radial = g.pos / r;
            let goo = radial * (-g.vel.dot(radial) * 0.0003);

            // let gravity ruin everyone's day
            let a = grav + goo;

            // FIXME:
            // euler integrator is cheap spellcasting.
            // leapfrog would behave better if the pasta ritual ever evolves.
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

/// NOTE:
/// bake the surviving noodle swarm into the disk lookup field
/// the shader later samples this thing to figure out where the sauce lives.
fn build_field(grains: &[Grain]) -> DiskField {
    // density = how much cosmic pasta is packed here
    let mut density = vec![0.0_f64; DR * DA];

    // vtan = how fast the pasta is zooming around
    let mut vtan = vec![0.0_f64; DR * DA];

    // counts = how many noodles screamed into each cell
    let mut counts = vec![0.0_f64; DR * DA];

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

        // NOTE:
        // local tangent direction so we can measure how hard the noodle is spinning
        let spin = DVec3::new(-g.pos.z / flat, 0.0, g.pos.x / flat);
        let vel = g.vel.dot(spin);

        // NOTE:
        // smear each grain into neighboring cells so the disk doesn't look like crunchy grid lasagna
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
                counts[idx] += weight;
            }
        }
    }

    // NOTE:
    // average the tangent speed per cell so one overcrowded pasta tile doesn't lie to the shader
    for i in 0..DR * DA {
        if counts[i] > 0.0 {
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

/// NOTE:
/// blur the disk field a few times so the sauce stops looking like crunchy grid nonsense.
fn blur_field(mut f: Vec<f64>) -> Vec<f64> {
    // PERF:
    // cloning the whole field every pass is lazy but reliable.
    // not the sexiest thing i've done, but it keeps the blur honest.
    for _ in 0..4 {
        let src = f.clone();

        for ri in 0..DR {
            for ai in 0..DA {
                let l = (ai + DA - 1) % DA;
                let r = (ai + 1) % DA;

                let ru = ri.saturating_sub(1);
                let rd = (ri + 1).min(DR - 1);

                // NOTE:
                // simple cross blur with wraparound in angle and clamped radius
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

/// welcome to the photon torture chamber.
/// wgsl uniform layout is a drama queen.
/// align(16) so wgpu stops screaming at me.
/// i do not fully know what i'm doing.
/// if it breaks: spacetime did it.
/// if it works: also spacetime. i just held the spoon.
///
/// WARN:
/// uniform layout must match wgsl Params exactly.
/// if this struct changes and shader struct does not, spacetime explodes immediately.
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

/// WARN:
/// this helper builds the exact param blob shared by cpu + shader.
/// any mismatch here vs wgsl is instant gremlin behavior.
/// literally...
/// one wrong field, order, padding demon
/// and the gpu starts hallucinating.
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

/// NOTE:
/// hdr gpu ritual.
/// shoots photons on the gpu, then drags the float sauce back to cpu memory.
async fn render_wgpu_hdr(field: &DiskField, render_w: u32, render_h: u32) -> Vec<f32> {
    // TEST:
    // sanity check uniform size while cpu/wgsl layout still haunted
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

    // OPTIM:
    // hdr and ldr both repack the same field junk.
    // could share this setup if i stop committing renderer crimes.
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

    // WARN:
    // hdr output is vec4<f32> per pixel.
    // this buffer gets fat insanely fast.
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

        // NOTE:
        // launch one thread group per 16x16 chunk of pixel suffering
        println!("dispatching hdr compute shader...");
        cp.dispatch_workgroups(render_w.div_ceil(16), render_h.div_ceil(16), 1);
    }

    enc.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, out_bytes);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();

    // WARN:
    // if map_async / poll / unmap gets touched wrong,
    // readback hangs or returns cursed garbage.
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

    // NOTE:
    // shader writes vec4<f32>, but the cpu only cares about rgb.
    // alpha gets thrown into the void.
    let mut rgb = vec![0.0f32; out_len * 3];
    for i in 0..out_len {
        let o = i * 4;
        rgb[i * 3] = f32s[o];
        rgb[i * 3 + 1] = f32s[o + 1];
        rgb[i * 3 + 2] = f32s[o + 2];
    }

    rgb
}

/// NOTE:
/// ldr gpu ritual.
/// same photon bullying as hdr, except this one comes back already packed for png suffering.
async fn render_wgpu_ldr(field: &DiskField, render_w: u32, render_h: u32) -> Vec<u8> {
    // TEST:
    // sanity check uniform size while cpu/wgsl layout still haunted
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

    // OPTIM:
    // hdr and ldr both repack the same field junk.
    // could share this setup if i stop committing renderer crimes.
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

        // NOTE:
        // launch one thread group per 16x16 tile of pixel suffering
        println!("dispatching ldr compute shader...");
        cp.dispatch_workgroups(render_w.div_ceil(16), render_h.div_ceil(16), 1);
    }

    enc.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, out_bytes);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();

    // WARN:
    // if map_async / poll / unmap gets touched wrong,
    // readback hangs or returns cursed garbage.
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

    // NOTE:
    // shader packed rgba8 into u32 lanes.
    // cpu only keeps rgb and throws alpha into the void.
    let mut rgb = vec![0u8; out_len * 3];
    for i in 0..out_len {
        rgb[i * 3] = data[i * 4];
        rgb[i * 3 + 1] = data[i * 4 + 1];
        rgb[i * 3 + 2] = data[i * 4 + 2];
    }

    rgb
}

/// shader jail:
/// i do not fully understand all this.
/// i just know where it bites.
/// photons get bullied until they confess a picture.
/// if a physicist sees this: no you didn't.
///
/// FIXME:
/// hdr and ldr shaders are mostly the same cursed organism duplicated.
/// one day i should stop committing this particular cauldron crime.
const SHADER_HDR: &str = include_str!("shaders/blackhole_hdr.wgsl");
const SHADER_LDR: &str = include_str!("shaders/blackhole_ldr.wgsl");
