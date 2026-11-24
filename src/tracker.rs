use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct TrackedBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub label: String,
    pub last_seen: f64,
    pub vx: f32,
    pub vy: f32,
    shift_history: VecDeque<(u64, f32, f32)>,
}

impl TrackedBox {
    fn displacement_since(&self, ref_seq: u64) -> (f32, f32) {
        if ref_seq == 0 || self.shift_history.is_empty() {
            return (0.0, 0.0);
        }
        let mut sx = 0.0f32;
        let mut sy = 0.0f32;
        for &(seq, dx, dy) in &self.shift_history {
            if seq > ref_seq {
                sx += dx;
                sy += dy;
            }
        }
        (sx, sy)
    }

    fn prune_history(&mut self, ref_seq: u64) {
        while let Some(&(seq, _, _)) = self.shift_history.front() {
            if seq <= ref_seq {
                self.shift_history.pop_front();
            } else {
                break;
            }
        }
    }
}

fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
    let ix1 = a.0.max(b.0);
    let iy1 = a.1.max(b.1);
    let ix2 = a.2.min(b.2);
    let iy2 = a.3.min(b.3);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    if inter <= 0.0 {
        return 0.0;
    }
    let area_a = ((a.2 - a.0) * (a.3 - a.1)).max(1.0);
    let area_b = ((b.2 - b.0) * (b.3 - b.1)).max(1.0);
    inter / (area_a + area_b - inter)
}

/// Pure-Rust Hybrid AI + Fast Block-Matching Motion Tracker (0% GPU between AI keyframes).
pub struct RegionTracker {
    tracks: Vec<TrackedBox>,
    smooth_alpha: f32,
}

impl RegionTracker {
    pub fn new() -> Self {
        Self {
            tracks: Vec::new(),
            smooth_alpha: 0.72,
        }
    }

    pub fn has_tracks(&self) -> bool {
        !self.tracks.is_empty()
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn reset_velocities(&mut self) {
        for tr in &mut self.tracks {
            tr.vx = 0.0;
            tr.vy = 0.0;
            tr.shift_history.clear();
        }
    }

    pub fn keep_alive(&mut self, now: f64) {
        for tr in &mut self.tracks {
            tr.last_seen = now;
            tr.vx = 0.0;
            tr.vy = 0.0;
        }
    }

    /// Expire tracks whose last AI confirmation is older than hold_seconds.
    pub fn prune_expired(&mut self, now: f64, hold_seconds: f32) -> bool {
        let before = self.tracks.len();
        self.tracks
            .retain(|tr| (now - tr.last_seen) <= hold_seconds as f64);
        self.tracks.len() != before
    }

    pub fn get_boxes(&self) -> Vec<((i32, i32, i32, i32), String)> {
        self.tracks
            .iter()
            .map(|tr| {
                (
                    (
                        tr.x1.round() as i32,
                        tr.y1.round() as i32,
                        tr.x2.round() as i32,
                        tr.y2.round() as i32,
                    ),
                    tr.label.clone(),
                )
            })
            .collect()
    }

    /// Pure-Rust coarse-to-fine SAD block matching on 320px grayscale frames (<0.3 ms CPU).
    pub fn track_motion(
        &mut self,
        prev_gray: &[u8],
        curr_gray: &[u8],
        gw: usize,
        gh: usize,
        full_w: usize,
        full_h: usize,
        now: f64,
        hold_seconds: f32,
        frame_seq: u64,
    ) -> Vec<((i32, i32, i32, i32), String)> {
        if self.tracks.is_empty() {
            return Vec::new();
        }
        let max_age = hold_seconds as f64 + 0.25;
        self.tracks.retain(|tr| (now - tr.last_seen) <= max_age);
        if self.tracks.is_empty() {
            return Vec::new();
        }

        let sx = gw as f32 / full_w as f32;
        let sy = gh as f32 / full_h as f32;
        let inv_sx = full_w as f32 / gw as f32;
        let inv_sy = full_h as f32 / gh as f32;

        for tr in &mut self.tracks {
            let bx1 = ((tr.x1 * sx) as i32).clamp(2, gw as i32 - 6);
            let by1 = ((tr.y1 * sy) as i32).clamp(2, gh as i32 - 6);
            let bx2 = ((tr.x2 * sx) as i32).clamp(bx1 + 4, gw as i32 - 2);
            let by2 = ((tr.y2 * sy) as i32).clamp(by1 + 4, gh as i32 - 2);

            let mut best_dx = 0i32;
            let mut best_dy = 0i32;
            let mut best_sad = u64::MAX;

            for dy in (-12..=12).step_by(2) {
                for dx in (-12..=12).step_by(2) {
                    if bx1 + dx < 0 || by1 + dy < 0 || bx2 + dx >= gw as i32 || by2 + dy >= gh as i32 {
                        continue;
                    }
                    let sad = patch_sad(prev_gray, curr_gray, gw, bx1, by1, bx2, by2, dx, dy, 2);
                    if sad < best_sad {
                        best_sad = sad;
                        best_dx = dx;
                        best_dy = dy;
                    }
                }
            }

            let c_dx = best_dx;
            let c_dy = best_dy;
            for dy in (c_dy - 2)..=(c_dy + 2) {
                for dx in (c_dx - 2)..=(c_dx + 2) {
                    if bx1 + dx < 0 || by1 + dy < 0 || bx2 + dx >= gw as i32 || by2 + dy >= gh as i32 {
                        continue;
                    }
                    let sad = patch_sad(prev_gray, curr_gray, gw, bx1, by1, bx2, by2, dx, dy, 1);
                    if sad < best_sad {
                        best_sad = sad;
                        best_dx = dx;
                        best_dy = dy;
                    }
                }
            }

            let dx_full = best_dx as f32 * inv_sx;
            let dy_full = best_dy as f32 * inv_sy;
            tr.vx = 0.5 * dx_full + 0.5 * tr.vx;
            tr.vy = 0.5 * dy_full + 0.5 * tr.vy;

            let bw = tr.x2 - tr.x1;
            let bh = tr.y2 - tr.y1;
            let nx1 = (tr.x1 + dx_full).clamp(0.0, (full_w as f32 - bw).max(0.0));
            let ny1 = (tr.y1 + dy_full).clamp(0.0, (full_h as f32 - bh).max(0.0));
            let actual_dx = nx1 - tr.x1;
            let actual_dy = ny1 - tr.y1;
            tr.x1 = nx1;
            tr.y1 = ny1;
            tr.x2 = nx1 + bw;
            tr.y2 = ny1 + bh;

            if frame_seq > 0 && (actual_dx.abs() > 0.01 || actual_dy.abs() > 0.01) {
                if tr.shift_history.len() >= 24 {
                    tr.shift_history.pop_front();
                }
                tr.shift_history.push_back((frame_seq, actual_dx, actual_dy));
            }
        }

        self.get_boxes()
    }

    pub fn update(
        &mut self,
        detections: &[((i32, i32, i32, i32), String)],
        now: f64,
        hold_seconds: f32,
        result_seq: u64,
        full_w: usize,
        full_h: usize,
    ) -> Vec<((i32, i32, i32, i32), String)> {
        let mut matched = vec![false; self.tracks.len()];

        for ((x1, y1, x2, y2), label) in detections {
            let cand = (*x1 as f32, *y1 as f32, *x2 as f32, *y2 as f32);
            let mut best_idx: Option<usize> = None;
            let mut best_iou = 0.12f32;
            let mut best_comp = (0.0f32, 0.0f32);

            for (idx, tr) in self.tracks.iter().enumerate() {
                if matched[idx] {
                    continue;
                }
                let (cdx, cdy) = if result_seq > 0 {
                    tr.displacement_since(result_seq)
                } else {
                    (0.0, 0.0)
                };
                let past_box = (tr.x1 - cdx, tr.y1 - cdy, tr.x2 - cdx, tr.y2 - cdy);
                let score = iou(past_box, cand);
                if score > best_iou {
                    best_iou = score;
                    best_idx = Some(idx);
                    best_comp = (cdx, cdy);
                }
            }

            if let Some(idx) = best_idx {
                let tr = &mut self.tracks[idx];
                let (cdx, cdy) = best_comp;
                let bw = cand.2 - cand.0;
                let bh = cand.3 - cand.1;
                let proj_x1 = (cand.0 + cdx).clamp(0.0, (full_w as f32 - bw).max(0.0));
                let proj_y1 = (cand.1 + cdy).clamp(0.0, (full_h as f32 - bh).max(0.0));
                let proj_x2 = proj_x1 + bw;
                let proj_y2 = proj_y1 + bh;

                let a = self.smooth_alpha;
                tr.x1 = a * proj_x1 + (1.0 - a) * tr.x1;
                tr.y1 = a * proj_y1 + (1.0 - a) * tr.y1;
                tr.x2 = a * proj_x2 + (1.0 - a) * tr.x2;
                tr.y2 = a * proj_y2 + (1.0 - a) * tr.y2;
                tr.label = label.clone();
                tr.last_seen = now;
                if result_seq > 0 {
                    tr.prune_history(result_seq);
                }
                matched[idx] = true;
            } else {
                self.tracks.push(TrackedBox {
                    x1: cand.0,
                    y1: cand.1,
                    x2: cand.2,
                    y2: cand.3,
                    label: label.clone(),
                    last_seen: now,
                    vx: 0.0,
                    vy: 0.0,
                    shift_history: VecDeque::with_capacity(24),
                });
                matched.push(true);
            }
        }

        self.tracks
            .retain(|tr| (now - tr.last_seen) <= hold_seconds as f64);
        self.get_boxes()
    }
}

#[inline]
fn patch_sad(
    prev: &[u8],
    curr: &[u8],
    gw: usize,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    dx: i32,
    dy: i32,
    step: usize,
) -> u64 {
    let mut sum = 0u64;
    let mut y = y1;
    while y < y2 {
        let py = (y as usize) * gw;
        let cy = ((y + dy) as usize) * gw;
        let mut x = x1;
        while x < x2 {
            let p = prev[py + x as usize] as i32;
            let c = curr[cy + (x + dx) as usize] as i32;
            sum += (p - c).unsigned_abs() as u64;
            x += step as i32;
        }
        y += step as i32;
    }
    sum
}
