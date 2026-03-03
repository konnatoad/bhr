use image::{ImageBuffer, Rgb};
use rayon::prelude::*;

const W: u32 = 1280;
const H: u32 = 720;

fn hash_u32(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    ((x & 0x00FF_FFFF) as f32) / 16_777_216.0
}

fn ray_dir(px: u32, py: u32) -> (f32, f32, f32) {
    let aspect = W as f32 / H as f32;
    let nx = (px as f32 + 0.5) / W as f32 * 2.0 - 1.0;
    let ny = 1.0 - (py as f32 + 0.5) / H as f32 * 2.0;
    let fov_scale = 1.0 / 1.732;
    let x = nx * aspect * fov_scale;
    let y = ny * fov_scale;
    let z = -1.0;
    let len = (x * x + y * y + z * z).sqrt();
    (x / len, y / len, z / len)
}

fn sample_starfield(dx: f32, dy: f32, dz: f32) -> (u8, u8, u8) {
    todo!();
}

fn main() {
    let mut buf = vec![0u8; (W as usize) * (H as usize) * 3];

    buf.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
        let x = (i as u32) % W;
        let y = (i as u32) / W;
        let (dx, dy, dz) = ray_dir(x, y);
        let (r, g, b) = sample_starfield(dx, dy, dz);
        px[0] = r;
        px[1] = g;
        px[2] = b;
    });
    let img: ImageBuffer<Rgb<u8>, _> =
        ImageBuffer::from_raw(W, H, buf).expect("Buffer size should match image dimensions");

    img.save("out.png").expect("failed to save out.png");
}
