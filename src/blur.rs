/// Pure-Rust ultra-fast tiny-patch separable blur (zero OpenCV dependency).
/// Downscales the detected BGRA region, runs a 2-pass separable box blur,
/// and upscales smoothly into a premultiplied BGRA patch for UpdateLayeredWindow.

pub struct BlurPatch {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub width: usize,
    pub height: usize,
    pub bgra: Vec<u8>,
    pub label: String,
}

pub fn blur_region_bgra(
    frame_bgra: &[u8],
    frame_w: usize,
    frame_h: usize,
    box_rect: (i32, i32, i32, i32),
    blur_strength: u32,
    label: String,
) -> Option<BlurPatch> {
    let x1 = box_rect.0.clamp(0, frame_w.saturating_sub(1) as i32) as usize;
    let y1 = box_rect.1.clamp(0, frame_h.saturating_sub(1) as i32) as usize;
    let x2 = box_rect.2.clamp(0, frame_w as i32) as usize;
    let y2 = box_rect.3.clamp(0, frame_h as i32) as usize;
    if x2 <= x1 + 2 || y2 <= y1 + 2 {
        return None;
    }

    let bw = x2 - x1;
    let bh = y2 - y1;

    // Higher blur_strength -> smaller downscale factor -> heavier, faster blur
    let clamped = blur_strength.clamp(10, 100) as f32;
    let downscale = (0.275 - (clamped / 100.0) * 0.245).clamp(0.025, 0.25);
    let sw = ((bw as f32 * downscale).round() as usize).clamp(3, 64);
    let sh = ((bh as f32 * downscale).round() as usize).clamp(3, 64);

    // 1. Fast bilinear downsample directly from full BGRA frame
    let mut small = vec![0u8; sw * sh * 4];
    let sx = bw as f32 / sw as f32;
    let sy = bh as f32 / sh as f32;
    for dy in 0..sh {
        let src_y = (y1 + ((dy as f32 + 0.5) * sy) as usize).min(frame_h - 1);
        let row_off = src_y * frame_w * 4;
        for dx in 0..sw {
            let src_x = (x1 + ((dx as f32 + 0.5) * sx) as usize).min(frame_w - 1);
            let s_idx = row_off + src_x * 4;
            let d_idx = (dy * sw + dx) * 4;
            small[d_idx] = frame_bgra[s_idx];
            small[d_idx + 1] = frame_bgra[s_idx + 1];
            small[d_idx + 2] = frame_bgra[s_idx + 2];
            small[d_idx + 3] = 255;
        }
    }

    // 2. Two-pass separable box blur (approximates Gaussian smoothly on tiny buffer)
    let mut tmp = vec![0u8; sw * sh * 4];
    let radius = 2usize;
    for _ in 0..2 {
        // Horizontal pass
        for y in 0..sh {
            for x in 0..sw {
                let mut sum = [0u32; 3];
                let mut cnt = 0u32;
                let lx = x.saturating_sub(radius);
                let rx = (x + radius).min(sw - 1);
                for kx in lx..=rx {
                    let i = (y * sw + kx) * 4;
                    sum[0] += small[i] as u32;
                    sum[1] += small[i + 1] as u32;
                    sum[2] += small[i + 2] as u32;
                    cnt += 1;
                }
                let o = (y * sw + x) * 4;
                tmp[o] = (sum[0] / cnt) as u8;
                tmp[o + 1] = (sum[1] / cnt) as u8;
                tmp[o + 2] = (sum[2] / cnt) as u8;
                tmp[o + 3] = 255;
            }
        }
        // Vertical pass
        for y in 0..sh {
            let ty = y.saturating_sub(radius);
            let by = (y + radius).min(sh - 1);
            for x in 0..sw {
                let mut sum = [0u32; 3];
                let mut cnt = 0u32;
                for ky in ty..=by {
                    let i = (ky * sw + x) * 4;
                    sum[0] += tmp[i] as u32;
                    sum[1] += tmp[i + 1] as u32;
                    sum[2] += tmp[i + 2] as u32;
                    cnt += 1;
                }
                let o = (y * sw + x) * 4;
                small[o] = (sum[0] / cnt) as u8;
                small[o + 1] = (sum[1] / cnt) as u8;
                small[o + 2] = (sum[2] / cnt) as u8;
                small[o + 3] = 255;
            }
        }
    }

    Some(BlurPatch {
        x1: x1 as i32,
        y1: y1 as i32,
        x2: x2 as i32,
        y2: y2 as i32,
        width: sw,
        height: sh,
        bgra: small,
        label,
    })
}
