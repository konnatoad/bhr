// WARN:
// must match the rust Params exactly
// one wrong order or padding demon and the gpu starts seeing shit
struct Params {
    w: u32,
    h: u32,
    ss: u32,
    _pad0: u32,
    dr: u32,
    da: u32,
    _padx0: u32,
    _padx1: u32,
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
    cam_pos: vec4<f32>,
    cam_look: vec4<f32>,
    fov: f32,
    ring_zone: f32,
    roll: f32,
    _pad2a: f32,
}

@group(0) @binding(0) var<uniform> P: Params;
@group(0) @binding(1) var<storage, read> field: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<vec4<f32>>;

// HACK:
// normalize safely.
// if the vector is basically dead, point it forward and keep lying.
fn make_unit(v: vec3<f32>) -> vec3<f32> {
    let l = length(v);
    if l < 1e-12 { return vec3<f32>(0.0, 0.0, 1.0); }
    return v / l;
}

// NOTE:
// filmic tone-map curve.
// compresses HDR brightness into something a mortal monitor can survive.
fn filmic(x: f32) -> f32 {
    let x2 = max(x - 0.004, 0.0);
    return (x2 * (6.2 * x2 + 0.5)) / (x2 * (6.2 * x2 + 1.7) + 0.06);
}

// XXX:
// cheap deterministic 2D jitter.
// not sacred randomness, just enough to stop the pixels from looking crunchy.
// absolutely the kind of thing everyone yoinks instead of writing proudly
fn rand2(x: f32, y: f32) -> vec2<f32> {
    let a = fract(sin(x * 127.1 + y * 311.7) * 43758.5453);
    let b = fract(sin(x * 269.5 + y * 183.3) * 43758.5453);
    return vec2<f32>(a, b);
}

// XXX:
// tiny hash for procedural star nonsense.
// statistically suspicious, visually acceptable.
// we can thank LLM for this shit
fn hash3(p: vec3<f32>) -> f32 {
    var q = fract(p * vec3<f32>(127.1, 311.7, 74.7));
    q                                                         += dot(q, q.yzx + 19.19);
    return fract((q.x + q.y) * q.z);
}

// XXX:
// same crime as above but in 2D.
// more yoinked shit with touch of llm numbers
fn hash2(x: f32, y: f32) -> f32 {
    return fract(sin(x * 127.1 + y * 311.7) * 43758.5453);
}

// XXX:
// bilinear value noise.
// used in every fucking shader on earth
// so yes this was spiritually yoinked from the communal graphics cauldron
fn noise2(x: f32, y: f32) -> f32 {
    let xi = floor(x); let yi = floor(y);
    let xf = x - xi; let yf = y - yi;
    let u = xf * xf * (3.0 - 2.0 * xf);
    let v = yf * yf * (3.0 - 2.0 * yf);
    return mix(mix(hash2(xi,yi), hash2(xi + 1.0,yi), u),
             mix(hash2(xi,yi + 1.0), hash2(xi + 1.0,yi + 1.0), u), v);
}

// NOTE:
// tiny fbm stack for turbulence and disk grime.
// same noise, several times, slightly louder each pass. true shader tradition.
fn fbm(x: f32, y: f32) -> f32 {
    var v = 0.0; var a = 0.5;
    var xx = x; var yy = y;
    for (var i = 0; i < 3; i++) {
        v                                                         += a * noise2(xx, yy);
        xx                                                         *= 2.1; yy                                                         *= 2.1; a                                                         *= 0.5;
    }
    return v;
}

// NOTE:
// sample the baked disk lookup field.
// translates physical disk position into density + tangential velocity
// because apparently we enjoy turning pasta into spreadsheets
fn sample_field(flat: f32, px: f32, pz: f32) -> vec2<f32> {
    if flat < P.disk_in || flat > P.disk_out {
        return vec2<f32>(0.0);
    }

    let drf = f32(P.dr);
    let daf = f32(P.da);

    // NOTE:
    // convert radius into table space.
    // real disk outside, crunchy bins inside
    let fr = ((flat - P.disk_in) / (P.disk_out - P.disk_in)) * drf;
    let r0 = min(u32(floor(fr)), P.dr - 1u);
    let r1 = min(r0 + 1u, P.dr - 1u);
    let tr = fract(fr);

    // NOTE:
    // atan2 gives [-π, π], which is cute until you need buffer indices
    // so we drag it into [0, 2π] like a misbehaving raccoon
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
// NOTE:
// planck radiation law.
// turns temperature into spectral intensity and numeric hostility
fn planck_sample(wl: f32, temp: f32) -> f32 {
    let x = 0.014388 / (wl * temp);
    if x > 500.0 { return 0.0; }
    if x < 1e-4 { return pow(wl,-4.0) * temp; }
    return pow(wl,-5.0) / (exp(x) - 1.0);
}

// HACK:
// sample blackbody color at three wavelengths and pretend that's enough RGB
// scientifically inspired, artistically underqualified
fn planck_rgb(temp: f32) -> vec3<f32> {
    if temp < 100.0 { return vec3<f32>(0.0); }
    let r = planck_sample(700e-9, temp);
    let g = planck_sample(532e-9, temp);
    let b = planck_sample(460e-9, temp);
    let m = max(max(r,g), max(b,1e-30));
    return vec3<f32>(r / m, g / m, b / m);
}

// NOTE:
// one procedural star cell.
// fake a tiny star blob inside hashed direction-space garbage
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

// NOTE:
// background stars + dust band.
// because empty space looks fake unless you add more fake space
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

    let dust_noise = 0.5 * noise2(lon * 2.0, lat * 6.0) + 0.5 * noise2(lon * 6.0, lat * 14.0);

    let dust = 0.004 + 0.004 * dust_noise;
    col     += vec3<f32>(dust * 0.55, dust * 0.60, dust * 0.78);

    let band_noise = 0.6 * noise2(lon * 3.0, lat * 8.0) + 0.4 * noise2(lon * 7.0, lat * 17.0);

    let band_shape = exp(-lat * lat * 14.0);
    let band_ripple = 0.85 + 0.24 * sin(lon * 2.6 - 1.1) + 0.11 * sin(lon * 5.8 + 0.4) + 0.05 * sin(lon * 11.0 - 0.7);

    let band = band_shape * (0.010 + 0.010 * band_noise) * band_ripple;
    col += vec3<f32>(band * 0.55, band * 0.60, band * 0.85);

    return col;
}

// NOTE:
// accretion disk shading.
// density, temperature, doppler, redshift, and several bad life choices meet here
fn disk_color(pos: vec3<f32>, dir: vec3<f32>) -> vec3<f32> {
    let flat = sqrt(pos.x * pos.x + pos.z * pos.z);

    if flat < P.bh_size * 1.03 {
        return vec3<f32>(0.0);
    }

    let h = pos.y / (flat * P.ad_height);
    // NOTE:
    // vertical falloff from the disk midplane.
    // if you're too far above the pasta sheet, no sauce for you
    let vert = exp(-h * h * P.ad_v);
    if vert < 0.001 { return vec3<f32>(0.0); }

    let rnorm = (flat - P.disk_in) / (P.disk_out - P.disk_in);

    // NOTE:
    // radial shaping so the disk doesn't glow like a cursed bathroom ring light
    let horiz = max(pow(1.0 - rnorm, P.ad_h * 0.72) * pow(rnorm + 0.05, 0.17), 0.0);
    if horiz < 0.001 { return vec3<f32>(0.0); }

    let band = 1.0 + 0.18 * sin(flat * 2.8 - 1.2) + 0.08 * sin(flat * 6.1);

    let ang = atan2(pos.z, pos.x);

    // HACK:
    // procedural breakup so the disk has grime instead of looking computer-generated
    // which it obviously is, but we don't need to advertise it
    let n = clamp(fbm(flat * P.ad_noise, ang * P.ad_noise * 0.5), 0.0, 1.0);

    let sv = sample_field(flat, pos.x, pos.z);
    let dcell = sv.x;
    let vtan = sv.y;

    let dm = max(P.density_max, 1.0);
    let micro = 0.992 + 0.016 * hash2(pos.x * 16.0, pos.z * 16.0);
    let density = vert * horiz * band * (0.35 + 0.65 * n) * clamp(dcell / dm, 0.0, 1.0) * micro;

    let ratio = flat / P.disk_in;
    let heat = P.disk_heat * pow(max(1.0 - pow(ratio, -0.5), 0.0), 0.25) * pow(ratio, -0.75);
    if heat < 200.0 { return vec3<f32>(0.0); }

    let spin = make_unit(vec3<f32>(-pos.z, 0.0, pos.x));
    let to_cam = make_unit(dir);
    let speed = min(abs(vtan), 0.82);
    let beta = -dot(spin, to_cam) * sign(vtan) * speed;
    let gamma = 1.0 / sqrt(max(1.0 - speed * speed, 0.001));
    let dop = 1.0 / max(gamma * (1.0 - beta), 0.01);

    // use a softer doppler for color so the far side stays warm instead of dying
    let color_dop = dop;

    // much softer brightness asymmetry
    let beam = pow(dop, 1.15);

    // let the red side keep some life
    var red_boost = 1.0;
    if dop < 1.0 {
        red_boost = 1.0 + clamp((1.0 - dop) * 2.4, 0.0, 1.6);
    }

    let grav = pow(max(1.0 - P.bh_size / flat, 0.01), 0.10);
    let seen = heat * color_dop * grav;

    let disk_normal = vec3<f32>(0.0, 1.0, 0.0);
    let view_angle = abs(dot(disk_normal, dir));
    let limb = pow(1.0 - view_angle, 0.6) + 0.4;

    var bright = density * pow(clamp(seen / P.disk_heat, 0.0, 1.2), 1.1) * beam * P.ad_lit * red_boost * limb;

    let ring_soft = smoothstep(P.bh_size * 1.010, P.bh_size * 1.028, flat);
    bright                                                         *= ring_soft;
    return planck_rgb(seen) * bright;
}

// NOTE:
// build camera ray from camera pos + target.
// regular camera math, now with optional spiritual tilt
fn make_camera_ray(px: f32, py: f32) -> vec3<f32> {
    let half = tan(radians(P.fov) * 0.5);
    let aspect = f32(P.w) / f32(P.h);
    let sx = (px / f32(P.w) * 2.0 - 1.0) * half * aspect;
    let sy = (1.0 - py / f32(P.h) * 2.0) * half;
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
    return make_unit(right * sx + up * sy + fwd);
}

// NOTE:
// main photon marcher.
// bend ray, harvest pasta glow, maybe escape, maybe get deleted
fn march(px: u32, py: u32, sx: u32, sy: u32) -> vec3<f32> {
    let j = rand2(f32(px) * 13.0 + f32(sx), f32(py) * 17.0 + f32(sy));

    let fx = f32(px) + (f32(sx) + j.x) / f32(P.ss);
    let fy = f32(py) + (f32(sy) + j.y) / f32(P.ss);

    var pos = P.cam_pos.xyz;
    var dir = make_camera_ray(fx, fy);
    var col = vec3<f32>(0.0);
    var swirl = 0.0;

    var min_dist = 1e9;
    var prev_dist = 1e9;
    var rim_done = false;
    var prev_y = pos.y;
    var plane_hits: u32 = 0u;

    for (var i: u32 = 0u; i < P.max_steps; i++) {
        let dist = length(pos);
        min_dist = min(min_dist, dist);

        // FIXME:
        // old photon-rim residue.
        // this block currently has a job title but no real responsibilities
        if !rim_done && dist > prev_dist {
            let rim_center = P.bh_size * 1.03;
            let rim_width = P.bh_size * 0.004;

            let rim_dist = abs(prev_dist - rim_center);
            let rim = smoothstep(rim_width, 0.0, rim_dist);

            let rim_view = pow(1.0 - abs(dir.y), 3.5);

            rim_done = true;
        }

        prev_dist = min(prev_dist, dist);
        if dist < P.bh_size {
            return col;
        }

        if dist > 120.0 {

            let swirl_ring = exp(-pow((swirl - 3.35) / 0.09, 2.0));

            let ring_view = pow(1.0 - abs(dir.y), 2.6);

            let ring_gate = 1.0 - smoothstep(P.bh_size * 1.00, P.bh_size * 1.018, min_dist);

            let photon_ring = vec3<f32>(1.0, 0.99, 0.96) * swirl_ring * ring_view * ring_gate * 0.42;

            col = max(col, photon_ring);

            return col + stars(dir);
        }

        let flat = sqrt(pos.x * pos.x + pos.z * pos.z);

        var step = 0.02 + dist * 0.01;
        step = min(step, max(0.00012, (dist - P.bh_size) * 0.02));

        if flat > P.disk_in - 1.0 && flat < P.disk_out + 1.0 {
            let disk_half = max(flat * P.ad_height * 2.8, 0.02);
            let plane_dist = abs(pos.y);

            let plane_factor = clamp(plane_dist / (disk_half * 3.0), 0.15, 1.0);
            step                                                         *= plane_factor;

            let inner_dist = abs(flat - P.disk_in);
            let ring_factor = clamp(inner_dist * 3.0, 0.18, 1.0);
            step                                                         *= ring_factor;
        }

        let bend_scale = 1.5 * P.bh_size / max(dist * dist, 0.0001);
        step                                                         *= clamp(1.0 / (1.0 + bend_scale * 40.0), 0.12, 1.0);
        step = clamp(step, 0.00003, 0.03);

        let grav_dir = -pos / dist;
        let bend = P.bh_mass / (dist * dist);
        dir = make_unit(dir + grav_dir * bend * step);
        swirl                                                         += bend * step;

        let next_pos = pos + dir * step;
        let next_flat = length(next_pos.xz);

        if next_flat > P.disk_in && next_flat < P.disk_out {
            let crossed_plane = (prev_y <= 0.0 && next_pos.y > 0.0) || (prev_y >= 0.0 && next_pos.y < 0.0);

            if crossed_plane {
                let t = clamp(prev_y / (prev_y - next_pos.y), 0.0, 1.0);
                let hit = pos + (next_pos - pos) * t;
                let hit_flat = length(hit.xz);

                if hit_flat > P.disk_in && hit_flat < P.disk_out {
                    plane_hits                                                         += 1u;

                    let caustic_center = P.disk_in * 1.03;
                    let caustic_width = P.disk_in * 0.10;
                    let caustic = 1.0 + 2.2 * exp(-pow((hit_flat - caustic_center) / caustic_width, 2.0));

                    let photon_ring_zone = 1.0 + 1.8 * exp(-pow((min_dist - P.bh_size * 1.035) / (P.bh_size * 0.06), 2.0));

                    let echo = select(1.0, 1.35, plane_hits >= 2u);
                    let thin = 1.0 / max(abs(next_pos.y - prev_y), 0.020);

                    let inner_block = smoothstep(P.bh_size * 1.03, P.bh_size * 1.07, hit_flat);

                    col = min(
    col + disk_color(hit, dir) * step * 3.2 * caustic * photon_ring_zone * echo * thin * inner_block,
    vec3<f32>(6.0)
);
                }
            }

            let disk_half = max(next_flat * P.ad_height, 0.003);
            if abs(next_pos.y) < disk_half * 0.55 {
                col = min(col + disk_color(next_pos, dir) * step * 1.6, vec3<f32>(6.0));
            }
        }

        pos = next_pos;
        prev_y = next_pos.y;
    }
    return col;
}

// PERF:
// compute entry point.
// one invocation per pixel, followed by several more crimes for supersampling
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if gid.x >= P.w || gid.y >= P.h { return; }

    var acc = vec3<f32>(0.0);
    for (var sy: u32 = 0u; sy < P.ss; sy++) {
        for (var sx: u32 = 0u; sx < P.ss; sx++) {
            acc                                                         += march(gid.x, gid.y, sx, sy);
        }
    }

    acc = acc / f32(P.ss * P.ss);
    out[gid.y * P.w + gid.x] = vec4<f32>(acc, 1.0);
}
