use anyhow::{anyhow, Result};
use half::f16;
use ort::environment::Environment;
use ort::ep::directml::{DirectML, PerformancePreference};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::TensorRef;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::config::{get_data_dir, get_exe_dir, get_models_dir, BUILTIN_MODEL_ID, NONE_MODEL_ID};

pub const NUDENET_LABELS: [&str; 18] = [
    "FEMALE_GENITALIA_COVERED",
    "FACE_FEMALE",
    "BUTTOCKS_EXPOSED",
    "FEMALE_BREAST_EXPOSED",
    "FEMALE_GENITALIA_EXPOSED",
    "MALE_BREAST_EXPOSED",
    "ANUS_EXPOSED",
    "FEET_EXPOSED",
    "BELLY_COVERED",
    "FEET_COVERED",
    "ARMPITS_COVERED",
    "ARMPITS_EXPOSED",
    "FACE_MALE",
    "BELLY_EXPOSED",
    "MALE_GENITALIA_EXPOSED",
    "ANUS_COVERED",
    "FEMALE_BREAST_COVERED",
    "BUTTOCKS_COVERED",
];

#[derive(Debug, Clone)]
pub struct Detection {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
    pub score: f32,
    pub label: String,
}

pub fn list_available_models() -> Vec<(String, String)> {
    let mut out = vec![(
        "Built-in: NudeNet 320n (Fast — Realistic & 3D)".to_string(),
        BUILTIN_MODEL_ID.to_string(),
    )];
    let mut seen = HashSet::new();
    for dir in [get_models_dir(), get_exe_dir().join("models")] {
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut names: Vec<_> = entries.flatten().collect();
            names.sort_by_key(|e| e.file_name());
            for entry in names {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()).map(|s| s.eq_ignore_ascii_case("onnx")).unwrap_or(false) {
                    if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                        let key = fname.to_lowercase();
                        if seen.insert(key) {
                            let mb = fs::metadata(&path).map(|m| m.len() as f64 / (1024.0 * 1024.0)).unwrap_or(0.0);
                            out.push((format!("{fname} ({mb:.1} MB)"), format!("models\\{fname}")));
                        }
                    }
                }
            }
        }
    }
    out
}

fn find_fp16_sibling(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    if name.to_lowercase().contains("_fp16") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let ext = path.extension()?.to_str()?;
    let sibling = path.with_file_name(format!("{stem}_fp16.{ext}"));
    sibling.is_file().then_some(sibling)
}

pub fn resolve_model_path(model_id: &str, prefer_fp16: bool) -> Option<PathBuf> {
    if model_id.is_empty() || model_id == NONE_MODEL_ID {
        return None;
    }
    let exe_dir = get_exe_dir();
    if model_id == BUILTIN_MODEL_ID {
        let mut candidates = Vec::new();
        if prefer_fp16 {
            candidates.push(exe_dir.join("models").join("320n_fp16.onnx"));
            candidates.push(exe_dir.join("runtime").join("320n_fp16.onnx"));
            candidates.push(get_models_dir().join("320n_fp16.onnx"));
        }
        candidates.push(exe_dir.join("models").join("320n.onnx"));
        candidates.push(exe_dir.join("runtime").join("320n.onnx"));
        candidates.push(get_models_dir().join("320n.onnx"));
        return candidates.into_iter().find(|p| p.is_file());
    }

    let p = PathBuf::from(model_id);
    let resolved = if p.is_absolute() && p.is_file() {
        Some(p)
    } else {
        [get_data_dir().join(model_id), exe_dir.join(model_id), p]
            .into_iter()
            .find(|c| c.is_file())
    }?;

    if prefer_fp16 {
        if let Some(sib) = find_fp16_sibling(&resolved) {
            return Some(sib);
        }
    }
    Some(resolved)
}

pub fn find_onnxruntime_dll() -> PathBuf {
    let exe_dir = get_exe_dir();
    for candidate in [
        exe_dir.join("onnxruntime.dll"),
        exe_dir.join("runtime").join("onnxruntime.dll"),
        PathBuf::from("runtime").join("onnxruntime.dll"),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    exe_dir.join("onnxruntime.dll")
}

struct LoadedModel {
    session: Session,
    name: String,
    input_size: usize,
    is_fp16: bool,
    expects_fp16_input: bool,
    f32_buf: Vec<f32>,
    f16_buf: Vec<f16>,
}

pub struct FastNSFWDetector {
    env: Environment,
    models: Vec<LoadedModel>,
    compute_device: String,
    use_fp16: bool,
    pub last_infer_ms: f32,
}

impl FastNSFWDetector {
    pub fn new(primary: &str, secondary: &str, compute_device: &str, use_fp16: bool) -> Result<Self> {
        let dll_path = find_onnxruntime_dll();
        let env = ort::init_from(&dll_path)?
            .with_name("CleanScreenGuard")
            .build()?;

        let mut det = Self {
            env,
            models: Vec::new(),
            compute_device: compute_device.to_string(),
            use_fp16,
            last_infer_ms: 0.0,
        };
        det.reload_models(primary, secondary, compute_device, use_fp16)?;
        Ok(det)
    }

    pub fn status_text(&self) -> String {
        if self.models.is_empty() {
            return "No model loaded (place 320n_fp16.onnx in models/)".to_string();
        }
        let names = self
            .models
            .iter()
            .map(|m| format!("{} ({}px {})", m.name, m.input_size, if m.is_fp16 { "FP16" } else { "FP32" }))
            .collect::<Vec<_>>()
            .join(" + ");
        let dev = match self.compute_device.as_str() {
            "power_saving" => "iGPU / Power-Saving (DirectML)",
            "high_performance" => "Dedicated GPU (DirectML)",
            "cpu" => "CPU",
            _ => "Auto GPU (DirectML)",
        };
        format!("{names}  |  {dev}")
    }

    pub fn short_tag(&self) -> String {
        if self.models.is_empty() {
            return "NO MODEL".to_string();
        }
        let prec = if self.models.iter().any(|m| m.is_fp16) { "FP16" } else { "FP32" };
        let dev = match self.compute_device.as_str() {
            "power_saving" => "iGPU",
            "high_performance" => "dGPU",
            "cpu" => "CPU",
            _ => "GPU",
        };
        format!("{prec} | {dev}")
    }

    pub fn reload_models(
        &mut self,
        primary: &str,
        secondary: &str,
        compute_device: &str,
        use_fp16: bool,
    ) -> Result<()> {
        self.compute_device = compute_device.to_string();
        self.use_fp16 = use_fp16;
        let prefer_fp16 = use_fp16 && compute_device != "cpu";

        let mut loaded = Vec::new();
        let mut seen = HashSet::new();

        for mid in [primary, secondary] {
            if let Some(path) = resolve_model_path(mid, prefer_fp16) {
                if !seen.insert(path.clone()) {
                    continue;
                }
                match self.build_model(&path, compute_device) {
                    Ok(m) => loaded.push(m),
                    Err(e) => {
                        eprintln!("[Detector] Failed to load {}: {e}", path.display());
                        if prefer_fp16 {
                            if let Some(fp32) = resolve_model_path(mid, false) {
                                if seen.insert(fp32.clone()) {
                                    if let Ok(m) = self.build_model(&fp32, compute_device) {
                                        loaded.push(m);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if loaded.is_empty() {
            if let Some(fb) = resolve_model_path(BUILTIN_MODEL_ID, false) {
                if let Ok(m) = self.build_model(&fb, compute_device) {
                    loaded.push(m);
                }
            }
        }

        self.models = loaded;
        Ok(())
    }

    fn build_model(&self, path: &Path, compute_device: &str) -> Result<LoadedModel> {
        let mut builder = Session::builder(&self.env)?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(2)?;

        if compute_device != "cpu" {
            let pref = match compute_device {
                "power_saving" => PerformancePreference::MinimumPower,
                "high_performance" => PerformancePreference::HighPerformance,
                _ => PerformancePreference::Default,
            };
            let dml = DirectML::default().with_performance_preference(pref).build();
            builder = builder.with_execution_providers([dml])?;
        }

        let mut session = builder.commit_from_file(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.onnx")
            .to_string();

        let input_size = if name.to_lowercase().contains("320") { 320 } else { 640 };
        let expects_fp16_input = session
            .inputs()
            .first()
            .map(|i| format!("{:?}", i.dtype()).to_lowercase().contains("float16"))
            .unwrap_or(false);
        let is_fp16 = expects_fp16_input || name.to_lowercase().contains("fp16");

        let total = 3 * input_size * input_size;
        let f32_buf = vec![0.0f32; total];
        let f16_buf = vec![f16::ZERO; if expects_fp16_input { total } else { 0 }];

        // Silent background DirectML shader warmup so first real scan never hitches
        if expects_fp16_input {
            if let Ok(t) = TensorRef::from_array_view(([1usize, 3, input_size, input_size], &*f16_buf)) {
                let _ = session.run(ort::inputs![t]);
            }
        } else if let Ok(t) = TensorRef::from_array_view(([1usize, 3, input_size, input_size], &*f32_buf)) {
            let _ = session.run(ort::inputs![t]);
        }

        Ok(LoadedModel {
            session,
            name,
            input_size,
            is_fp16,
            expects_fp16_input,
            f32_buf,
            f16_buf,
        })
    }

    pub fn detect(
        &mut self,
        frame_bgra: &[u8],
        frame_w: usize,
        frame_h: usize,
        target_labels: &HashSet<&'static str>,
        conf_threshold: f32,
    ) -> Vec<Detection> {
        if self.models.is_empty() || target_labels.is_empty() {
            return Vec::new();
        }
        let t0 = Instant::now();
        let mut raw_hits: Vec<Detection> = Vec::new();

        for model in &mut self.models {
            let size = model.input_size;
            let scale = size as f32 / (frame_w.max(frame_h) as f32);
            let nw = ((frame_w as f32 * scale).round() as usize).clamp(1, size);
            let nh = ((frame_h as f32 * scale).round() as usize).clamp(1, size);

            // Fill reusable CHW RGB letterbox tensor directly from native BGRA frame
            model.f32_buf.fill(0.0);
            let plane = size * size;
            let inv_255 = 1.0f32 / 255.0f32;
            for dy in 0..nh {
                let sy = ((dy as f32 + 0.5) / scale) as usize;
                let sy = sy.min(frame_h - 1);
                let row_off = sy * frame_w * 4;
                let dst_row = dy * size;
                for dx in 0..nw {
                    let sx = ((dx as f32 + 0.5) / scale) as usize;
                    let sx = sx.min(frame_w - 1);
                    let p = row_off + sx * 4;
                    let idx = dst_row + dx;
                    // BGRA -> RGB CHW normalized [0, 1]
                    model.f32_buf[idx] = frame_bgra[p + 2] as f32 * inv_255;
                    model.f32_buf[plane + idx] = frame_bgra[p + 1] as f32 * inv_255;
                    model.f32_buf[2 * plane + idx] = frame_bgra[p] as f32 * inv_255;
                }
            }

            let outputs = if model.expects_fp16_input {
                for (dst, &src) in model.f16_buf.iter_mut().zip(model.f32_buf.iter()) {
                    *dst = f16::from_f32(src);
                }
                match TensorRef::from_array_view(([1usize, 3, size, size], &*model.f16_buf)) {
                    Ok(t) => model.session.run(ort::inputs![t]).ok(),
                    Err(_) => None,
                }
            } else {
                match TensorRef::from_array_view(([1usize, 3, size, size], &*model.f32_buf)) {
                    Ok(t) => model.session.run(ort::inputs![t]).ok(),
                    Err(_) => None,
                }
            };

            let Some(outputs) = outputs else { continue };
            let Ok((shape, data)) = outputs[0].try_extract_tensor::<f32>() else { continue };
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            if dims.len() != 3 {
                continue;
            }

            let inv_scale = 1.0f32 / scale;
            let (rows, cols, transposed) = if dims[1] < dims[2] {
                (dims[2], dims[1], true) // YOLO [1, 22, 2100]
            } else {
                (dims[1], dims[2], false)
            };
            if cols <= 4 {
                continue;
            }

            let num_classes = (cols - 4).min(NUDENET_LABELS.len());
            for r in 0..rows {
                let mut best_c = 0usize;
                let mut best_s = 0.0f32;
                for c in 0..num_classes {
                    let s = if transposed {
                        data[(4 + c) * rows + r]
                    } else {
                        data[r * cols + 4 + c]
                    };
                    if s > best_s {
                        best_s = s;
                        best_c = c;
                    }
                }
                if best_s < conf_threshold {
                    continue;
                }
                let lbl = NUDENET_LABELS[best_c];
                if !target_labels.contains(lbl) {
                    continue;
                }
                let (cx, cy, bw, bh) = if transposed {
                    (data[r], data[rows + r], data[2 * rows + r], data[3 * rows + r])
                } else {
                    (data[r * cols], data[r * cols + 1], data[r * cols + 2], data[r * cols + 3])
                };
                let x1 = ((cx - bw * 0.5) * inv_scale).round() as i32;
                let y1 = ((cy - bh * 0.5) * inv_scale).round() as i32;
                let x2 = ((cx + bw * 0.5) * inv_scale).round() as i32;
                let y2 = ((cy + bh * 0.5) * inv_scale).round() as i32;
                if x2 > x1 + 4 && y2 > y1 + 4 {
                    raw_hits.push(Detection {
                        x1: x1.clamp(0, frame_w as i32),
                        y1: y1.clamp(0, frame_h as i32),
                        x2: x2.clamp(0, frame_w as i32),
                        y2: y2.clamp(0, frame_h as i32),
                        score: best_s,
                        label: lbl.to_string(),
                    });
                }
            }
        }

        self.last_infer_ms = t0.elapsed().as_secs_f32() * 1000.0;
        nms(raw_hits, 0.35)
    }
}

fn nms(mut boxes: Vec<Detection>, iou_thresh: f32) -> Vec<Detection> {
    boxes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut keep: Vec<Detection> = Vec::new();
    for cand in boxes {
        let suppressed = keep.iter().any(|k| {
            let ix1 = cand.x1.max(k.x1);
            let iy1 = cand.y1.max(k.y1);
            let ix2 = cand.x2.min(k.x2);
            let iy2 = cand.y2.min(k.y2);
            let iw = (ix2 - ix1).max(0) as f32;
            let ih = (iy2 - iy1).max(0) as f32;
            let inter = iw * ih;
            if inter <= 0.0 {
                return false;
            }
            let aa = ((cand.x2 - cand.x1) * (cand.y2 - cand.y1)).max(1) as f32;
            let ab = ((k.x2 - k.x1) * (k.y2 - k.y1)).max(1) as f32;
            inter / (aa + ab - inter) > iou_thresh
        });
        if !suppressed {
            keep.push(cand);
        }
    }
    keep
}
