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
use std::f64::consts::PI;
use wgpu::util::DeviceExt;

// WARNING:
// this summons millions of cosmic noodles,
// bends spacetime slightly illegally,
// and then paints an accretion disk made of astrophysical pasta vomit.
// if it looks wrong: either relativity or the pasta spirits.
const W: u32 = 3840;
const H: u32 = 2160;

// how many noodles we throw into the gravity cauldron
const ROCKS: usize = 800_000;

// how long we keep chanting at the void
const STEPS: usize = 4_000;

// timestep: small so the universe doesn't instantly file a complaint
const DT: f64 = 0.0018;

// black hole: angry altar / gravity tax collector
const BH_MASS: f64 = 1.0;
const BH_SIZE: f64 = 1.0; // radius of "no refunds, no witnesses"

// cursed pasta ring bounds
const DISK_IN: f64 = 1.0;
const DISK_OUT: f64 = 12.0;

// visuals: unholy sliders i poked until it stopped looking like wet lint
const DISK_HEAT: f64 = 18000.0; // glow spell intensity
const ADISK_DENSITY_V: f64 = 5.0; // vertical squish curse
const ADISK_DENSITY_H: f64 = 5.0; // horizontal fade chant
const ADISK_HEIGHT: f64 = 0.01; // cosmic crepe thickness
const ADISK_LIT: f64 = 0.6; // "pls show up on screen" rune
const ADISK_NOISE_SCALE: f64 = 1.5; // turbulence dial (for fake trauma texture)

// donut pixels (grid of suffering)
const DR: usize = 128;
const DA: usize = 256;

// camera: filming forbidden pasta rituals from a low angle
const CAM_POS: DVec3 = DVec3::new(0.0, 2.2, -14.0);
const CAM_LOOK: DVec3 = DVec3::new(0.0, 0.0, 0.0);
const FOV: f64 = 55.0;

const EXPOSURE: f64 = 1.0; // when the void is too emo

// ray march knobs (how many times we poke spacetime with a stick)
const MAX_STEPS: u32 = 50_000;
const RING_ZONE: f64 = 1.0;

// AA: 2 is sane. 3 is "my gpu is now a candle"
const SS: u32 = 3;

fn main() {
    println!("summoning {} space noodles...", ROCKS);
    let mut idiots = make_the_donut(ROCKS);

    println!("stirring gravity cauldron for {} steps...", STEPS);
    let_the_idiots_spin(&mut idiots, STEPS);

    println!(
        "distilling pasta fog into a grid from {} survivors...",
        idiots.len()
    );
    let field = build_field(&idiots);

    println!("gpu photon bullying {}x{} ss={}...", W, H, SS);
    let rgb = pollster::block_on(render_wgpu(&field));

    // shovel bytes into png so humans can point at it and go "wow"
    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(W, H);
    for y in 0..H as usize {
        for x in 0..W as usize {
            let i = (y * W as usize + x) * 3;
            img.put_pixel(x as u32, y as u32, Rgb([rgb[i], rgb[i + 1], rgb[i + 2]]));
        }
    }

    img.save("blackhole_sim.png").unwrap();
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

        let i = field_idx(ri, ai);

        density[i] += 1.0;

        // tangential direction around the disk
        let spin = DVec3::new(-g.pos.z / flat, 0.0, g.pos.x / flat);

        // average spin per cell
        vtan[i] += g.vel.dot(spin);
        counts[i] += 1;
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

fn blur_field(f: Vec<f64>) -> Vec<f64> {
    let mut out = f.clone();

    // cheap blur. good enough. don't @ me.
    // this is literally "stir the sauce gently"
    for _ in 0..2 {
        let src = out.clone();
        for ri in 0..DR {
            for ai in 0..DA {
                let l = (ai + DA - 1) % DA;
                let r = (ai + 1) % DA;
                out[field_idx(ri, ai)] =
                    (src[field_idx(ri, l)] + src[field_idx(ri, ai)] + src[field_idx(ri, r)]) / 3.0;
            }
        }
    }

    out
}

// ── GPU renderer ──────────────────────────────────────────────────────────
//
// welcome to the photon torture chamber.
// wgsl uniform layout is a drama queen.
// align(16) so wgpu stops screaming at me.

// WARNING:
// i do not fully know what i'm doing.
// if it breaks: spacetime did it.
// if it works: also spacetime. i just held the spoon.
#[repr(C, align(16))]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Params {
    w: u32,
    h: u32,
    ss: u32,
    _pad0: u32,

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
    _pad2: [f32; 2],
}

async fn render_wgpu(field: &DiskField) -> Vec<u8> {
    // if this trips, uniform layout mismatch. aka wgsl gremlin time.
    assert_eq!(std::mem::size_of::<Params>(), 128);

    // summon gpu
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
            label: Some("bhr"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::Off,
        })
        .await
        .expect("no device (the void said no)");

    // pack density + vtan into one buffer because i hate myself less than two buffers
    let mut packed = Vec::<f32>::with_capacity(DR * DA * 2);
    packed.extend_from_slice(&field.density);
    packed.extend_from_slice(&field.vtan);

    let field_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("field"),
        contents: bytemuck::cast_slice(&packed),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let params = Params {
        w: W,
        h: H,
        ss: SS,
        _pad0: 0,

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
        _pad2: [0.0; 2],
    };

    let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let out_len = (W as usize) * (H as usize);

    // output buffer: where the gpu dumps its pasta painting
    let out_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("out"),
        size: (out_len * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // readback buffer: the "bring it back to cpu so i can look at it" bucket
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: (out_len * 4) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // compile shader (wgsl is a language made by goblins)
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bhr"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });

    // bind group layout: where i pretend i understand binding rules
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bgl"),
        entries: &[
            // uniforms: small sacred scroll
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(std::num::NonZeroU64::new(128).unwrap()),
                },
                count: None,
            },
            // field: pasta oracle grid
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
            // out: where the photons cry into
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

    // bind group: actual binding of the sacred objects
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
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

    // pipeline layout: "ok gpu, here’s your rulebook"
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pl"),
        bind_group_layouts: &[&bgl],
        immediate_size: 0,
    });

    // compute pipeline: where we officially start committing photon crimes
    let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("pipe"),
        layout: Some(&pl),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    // command encoder: i write down instructions for the gpu like it's a demon contract
    let mut enc =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });

    {
        // compute pass: the spell circle
        let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("pass"),
            timestamp_writes: None,
        });

        cp.set_pipeline(&pipe);
        cp.set_bind_group(0, &bg, &[]);

        // idk why 16x16 is "the way", but everyone does it, so i do it too
        cp.dispatch_workgroups((W + 15) / 16, (H + 15) / 16, 1);
    }

    // copy the gpu painting into the readback bucket
    enc.copy_buffer_to_buffer(&out_buf, 0, &readback, 0, (out_len * 4) as u64);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();

    // map buffer async because wgpu likes drama
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).unwrap();
    });

    // wait. if you hear sobbing, that's the gpu.
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(std::time::Duration::MAX),
    });

    rx.recv().unwrap().expect("map failed (alignment curse?)");

    // pull bytes out. do not ask questions.
    let data = slice.get_mapped_range().to_vec();
    readback.unmap();

    // unpack rgba -> rgb (alpha is just here for emotional support)
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

const SHADER: &str = r#"
struct Params {
  w: u32, h: u32, ss: u32, _pad0: u32,

  bh_size: f32, bh_mass: f32, disk_in: f32, disk_out: f32,
  disk_heat: f32, density_max: f32, ad_v: f32, ad_h: f32,
  ad_height: f32, ad_lit: f32, ad_noise: f32, exposure: f32,

  max_steps: u32, _pad1a: u32, _pad1b: u32, _pad1c: u32,

  cam_pos: vec4<f32>,
  cam_look: vec4<f32>,

  fov: f32, ring_zone: f32, _pad2a: f32, _pad2b: f32,
}

@group(0) @binding(0) var<uniform>            P     : Params;
@group(0) @binding(1) var<storage, read>      field : array<f32>;
@group(0) @binding(2) var<storage,read_write> out   : array<u32>;

fn make_unit(v: vec3<f32>) -> vec3<f32> {
  // normalize before it becomes cursed spaghetti
  let l = length(v);
  if l < 1e-12 { return vec3<f32>(0.0, 0.0, 1.0); }
  return v / l;
}

fn filmic(x: f32) -> f32 {
  // tone map so the pasta doesn't become nuclear
  let x2 = max(x - 0.004, 0.0);
  return (x2 * (6.2*x2 + 0.5)) / (x2 * (6.2*x2 + 1.7) + 0.06);
}

fn hash3(p: vec3<f32>) -> f32 {
  // random numbers in space. yes. it’s witchcraft.
  var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
  q += dot(q, q.yzx + 19.19);
  return fract((q.x + q.y) * q.z);
}

fn hash2(x: f32, y: f32) -> f32 {
  // i cast: SIN WIZARDRY
  return fract(sin(x*127.1 + y*311.7)*43758.5453);
}

fn noise2(x: f32, y: f32) -> f32 {
  // noise so the disk looks like it has trauma
  let xi = floor(x); let yi = floor(y);
  let xf = x-xi; let yf = y-yi;
  let u = xf*xf*(3.0-2.0*xf);
  let v = yf*yf*(3.0-2.0*yf);
  return mix(mix(hash2(xi,yi), hash2(xi+1.0,yi), u),
             mix(hash2(xi,yi+1.0), hash2(xi+1.0,yi+1.0), u), v);
}

fn fbm(x: f32, y: f32) -> f32 {
  // layered noise because one noise wasn't annoying enough
  var v = 0.0; var a = 0.5;
  var xx = x; var yy = y;
  for (var i = 0; i < 3; i++) {
    v  += a * noise2(xx, yy);
    xx *= 2.1; yy *= 2.1; a *= 0.5;
  }
  return v;
}

fn sample_field(flat: f32, px: f32, pz: f32) -> vec2<f32> {
  // consult the pasta oracle grid (density + spin)
  if flat < P.disk_in || flat > P.disk_out { return vec2<f32>(0.0); }
  var ri = u32(((flat-P.disk_in)/(P.disk_out-P.disk_in))*128.0);
  ri = min(ri, 127u);
  let ang = atan2(pz, px) + 3.14159265;
  let ai  = u32((ang/(2.0*3.14159265))*256.0) % 256u;
  let i   = ri*256u + ai;
  return vec2<f32>(field[i], field[i + 128u*256u]);
}

fn planck_sample(wl: f32, temp: f32) -> f32 {
  // planck-ish: hot pasta glows more. science-ish.
  let x = 0.014388/(wl*temp);
  if x > 500.0 { return 0.0; }
  if x < 1e-4  { return pow(wl,-4.0)*temp; }
  return pow(wl,-5.0)/(exp(x)-1.0);
}

fn planck_rgb(temp: f32) -> vec3<f32> {
  // temperature -> rgb (sauce color chooser)
  if temp < 100.0 { return vec3<f32>(0.0); }
  let r = planck_sample(700e-9, temp);
  let g = planck_sample(546e-9, temp);
  let b = planck_sample(435e-9, temp);
  let m = max(max(r,g), max(b,1e-30));
  return vec3<f32>(r/m, g/m, b/m);
}

fn stars(dir: vec3<f32>) -> vec3<f32> {
  // fake stars so the void isn't socially awkward
  let d = make_unit(dir);
  var col = vec3<f32>(0.0);

  let s1 = floor(d * 250.0);
  let h1 = hash3(s1);
  if h1 > 0.994 {
    let br = pow((h1-0.994)/0.006, 2.0) * 0.9;
    let ct = hash3(s1+0.5);
    col += vec3<f32>(br*(0.8+ct*0.4), br*(0.85+ct*0.15), br);
  }

  let s2 = floor(d * 700.0);
  let h2 = hash3(s2+77.7);
  if h2 > 0.9985 {
    let br = pow((h2-0.9985)/0.0015, 2.0) * 2.5;
    let ct = hash3(s2+0.3);
    let dx = abs(fract(d.x*700.0) - 0.5);
    let spike = 1.0 + max(0.0, 0.3 - dx*15.0);
    col += vec3<f32>(br*spike, br*(0.9+ct*0.1)*spike, br*(0.7+ct*0.3)*spike);
  }

  let lat  = asin(clamp(d.y, -1.0, 1.0));
  let lon  = atan2(d.z, d.x);
  let band = exp(-lat*lat*20.0) * (0.004 + 0.002*noise2(lon*3.0, lat*8.0));
  col += vec3<f32>(band*0.6, band*0.65, band*0.9);

  return col;
}

fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
  // accretion disk = cosmic pasta vomit spinning at illegal speed
  let flat = sqrt(pos.x*pos.x + pos.z*pos.z);

  let h    = pos.y / (flat * P.ad_height);
  let vert = exp(-h*h * P.ad_v);
  if vert < 0.001 { return vec3<f32>(0.0); }

  let rnorm = (flat - P.disk_in) / (P.disk_out - P.disk_in);
  let horiz = max(pow(1.0-rnorm, P.ad_h) * pow(rnorm+0.04, 0.1), 0.0);
  if horiz < 0.001 { return vec3<f32>(0.0); }

  let band = 1.0 + 0.18 * sin(flat * 2.8 - 1.2) + 0.08 * sin(flat * 6.1);

  let ang  = atan2(pos.z, pos.x);
  let n    = clamp(fbm(flat*P.ad_noise, ang*P.ad_noise*0.5), 0.0, 1.0);

  let sv    = sample_field(flat, pos.x, pos.z);
  let dcell = sv.x;
  let vtan  = sv.y;

  let dm      = max(P.density_max, 1.0);
  let density = vert * horiz * band * (0.35 + 0.65*n) * clamp(dcell/dm, 0.0, 1.0);

  let ratio = flat / P.disk_in;
  let heat  = P.disk_heat
    * pow(max(1.0 - pow(ratio, -0.5), 0.0), 0.25)
    * pow(ratio, -0.75);
  if heat < 200.0 { return vec3<f32>(0.0); }

  let spin   = make_unit(vec3<f32>(-pos.z, 0.0, pos.x));
  let to_cam = make_unit(-dir);
  let speed  = min(abs(vtan), 0.72);
  let beta   = -dot(spin, to_cam) * sign(vtan) * speed;
  let gamma  = 1.0 / sqrt(max(1.0 - speed*speed, 0.001));
  let dop    = 1.0 / max(gamma*(1.0-beta), 0.01);

  // idfk why doppler makes it look so good but it does. i’m not asking.
  var red_boost = 1.0;
  if dop < 1.0 {
    red_boost = 1.0 + clamp((1.0 - dop) * 0.8, 0.0, 0.55);
  }

  let beam = min(pow(dop, 2.5), 2.2);

  // gravitational redshift-ish: photons leaving the hole get tired and sad
  let grav = sqrt(max(1.0 - P.bh_size/flat, 0.01));
  let seen = heat * dop * grav;

  let bright = density
    * pow(clamp(seen/P.disk_heat, 0.0, 1.8), 2.0)
    * beam
    * P.ad_lit
    * red_boost;

  return planck_rgb(seen) * bright;
}

fn make_camera_ray(px: f32, py: f32) -> vec3<f32> {
  // camera ray: point at the void and hope it doesn't bite
  let half   = tan(radians(P.fov)*0.5);
  let aspect = f32(P.w)/f32(P.h);
  let sx = (px/f32(P.w)*2.0 - 1.0)*half*aspect;
  let sy = (1.0 - py/f32(P.h)*2.0)*half;
  let fwd   = make_unit(P.cam_look.xyz - P.cam_pos.xyz);
  let right = make_unit(cross(fwd, vec3<f32>(0.0,1.0,0.0)));
  let up    = cross(right, fwd);
  return make_unit(right*sx + up*sy + fwd);
}

fn march(px: u32, py: u32, sx: u32, sy: u32) -> vec3<f32> {
  // photon bullying loop: poke spacetime until it draws pasta
  let fx = f32(px) + (f32(sx)+0.5)/f32(P.ss);
  let fy = f32(py) + (f32(sy)+0.5)/f32(P.ss);

  var pos   = P.cam_pos.xyz;
  var dir   = make_camera_ray(fx, fy);
  var col   = vec3<f32>(0.0);
  var swirl = 0.0;

  for (var i: u32 = 0u; i < P.max_steps; i++) {
    let dist = length(pos);

    // black hole is actually black. no glow. no forgiveness.
    if dist < P.bh_size * 0.97 {
      return col;
    }

    if dist > 120.0 {
      return col + stars(dir);
    }

    // base step: small near pain, bigger far away
    var step = 0.05 * min(dist/8.0, 2.5);

    // ring area: finer steps so we don't skip the lensed ribbon of doom
    if      dist < P.ring_zone * 1.04 { step = 0.0003; }
    else if dist < P.ring_zone * 1.12 { step = 0.0008; }
    else if dist < P.ring_zone * 1.30 { step = 0.002;  }
    else if dist < P.ring_zone * 1.80 { step = 0.006;  }
    else if dist < P.ring_zone * 3.5  { step = 0.015;  }
    else if dist < P.ring_zone * 8.0  { step = 0.03;   }

    // "disk bends under" helper:
    // idfk how to do this "properly" so i just take tiny steps near the disk plane.
    let flat = sqrt(pos.x*pos.x + pos.z*pos.z);
    if flat > P.disk_in && flat < P.disk_out {
      let disk_half = max(flat * P.ad_height * 2.8, 0.02);
      if abs(pos.y) < disk_half {
        step = min(step, 0.0018);
      }
    }

    // GR-ish bending. 1.5 is load-bearing. if you touch it, it breaks. trust me bro.
    let tc       = pos / dist;
    let sideways = dir - tc*dot(dir,tc);
    let bend     = 1.5 * P.bh_size / (dist*dist);
    dir   = make_unit(dir + sideways*(-bend*step));
    swirl += bend * step;

    // if we're inside the pasta ring radius, add sauce
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

  // supersampling: more photon bullying per pixel
  var acc = vec3<f32>(0.0);
  for (var sy: u32 = 0u; sy < P.ss; sy++) {
    for (var sx: u32 = 0u; sx < P.ss; sx++) {
      acc += march(gid.x, gid.y, sx, sy);
    }
  }
  acc = acc * (P.exposure / f32(P.ss*P.ss));

  let rr = u32(clamp(filmic(acc.x), 0.0, 1.0) * 255.0);
  let gg = u32(clamp(filmic(acc.y), 0.0, 1.0) * 255.0);
  let bb = u32(clamp(filmic(acc.z), 0.0, 1.0) * 255.0);

  out[gid.y*P.w + gid.x] = rr | (gg<<8u) | (bb<<16u) | (255u<<24u);
}
"#;
