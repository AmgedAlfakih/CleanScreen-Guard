use anyhow::{anyhow, Result};
use half::f16;
use ort::environment::Environment;
use ort::ep::directml::{DirectML, PerformancePreference};
use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::{IoBinding, Session};
use ort::value::{DynValue, Tensor};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::config::{
    get_data_dir, get_exe_dir, get_models_dir, BUILTIN_MODEL_640M_ID, BUILTIN_MODEL_ID,
    NONE_MODEL_ID,
};

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
    let mut out = vec![
        (
            "Built-in: NudeNet 640m (High Accuracy — Default)".to_string(),
            BUILTIN_MODEL_640M_ID.to_string(),
        ),
        (
            "Built-in: NudeNet 320n (Fast — Lowest GPU)".to_string(),
            BUILTIN_MODEL_ID.to_string(),
        ),
    ];
    let mut seen = HashSet::from([
        "320n.onnx".to_string(),
        "320n_fp16.onnx".to_string(),
        "640m.onnx".to_string(),
        "640m_fp16.onnx".to_string(),
    ]);
    for dir in [get_models_dir(), get_exe_dir().join("models")] {
        if let Ok(entries) = fs::read_dir(&dir) {
            let mut names: Vec<_> = entries.flatten().collect();
            names.sort_by_key(|e| e.file_name());
            for entry in names {
                let path = entry.path();
                if path
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("onnx"))
                    .unwrap_or(false)
                {
                    if let Some(fname) = path.file_name().and_then(|s| s.to_str()) {
                        let key = fname.to_lowercase();
                        if seen.insert(key) {
                            let mb = fs::metadata(&path)
                                .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                                .unwrap_or(0.0);
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
    let lower = name.to_lowercase();
    if lower.contains("_fp16") || lower.contains("_int8") || lower.contains("_qdq") {
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
    if model_id == BUILTIN_MODEL_640M_ID || model_id == BUILTIN_MODEL_ID {
        let stem = if model_id == BUILTIN_MODEL_640M_ID { "640m" } else { "320n" };
        let mut candidates = Vec::new();
        if prefer_fp16 {
            candidates.push(exe_dir.join("models").join(format!("{stem}_fp16.onnx")));
            candidates.push(exe_dir.join("runtime").join(format!("{stem}_fp16.onnx")));
            candidates.push(get_models_dir().join(format!("{stem}_fp16.onnx")));
        }
        candidates.push(exe_dir.join("models").join(format!("{stem}.onnx")));
        candidates.push(exe_dir.join("runtime").join(format!("{stem}.onnx")));
        candidates.push(get_models_dir().join(format!("{stem}.onnx")));
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
    io_binding: IoBinding,
    input_name: String,
    input_value: DynValue,
    name: String,
    input_size: usize,
    precision_tag: &'static str,
    expects_fp16_input: bool,
}

impl LoadedModel {
    fn run_on_frame(
        &mut self,
        frame_bgra: &[u8],
        frame_w: usize,
        frame_h: usize,
        target_labels: &HashSet<&'static str>,
        conf_threshold: f32,
        out_hits: &mut Vec<Detection>,
    ) {
        let size = self.input_size;
        let scale = size as f32 / (frame_w.max(frame_h) as f32);
        let nw = ((frame_w as f32 * scale).round() as usize).clamp(1, size);
        let nh = ((frame_h as f32 * scale).round() as usize).clamp(1, size);
        let plane = size * size;
        let inv_255 = 1.0f32 / 255.0f32;

        // Fill pre-allocated owned tensor directly (zero heap allocations)
        if self.expects_fp16_input {
            let Ok((_, dst)) = self.input_value.try_extract_tensor_mut::<f16>() else {
                return;
            };
            dst.fill(f16::ZERO);
            for dy in 0..nh {
                let sy = (((dy as f32 + 0.5) / scale) as usize).min(frame_h - 1);
                let row_off = sy * frame_w * 4;
                let dst_row = dy * size;
                for dx in 0..nw {
                    let sx = (((dx as f32 + 0.5) / scale) as usize).min(frame_w - 1);
                    let p = row_off + sx * 4;
                    let idx = dst_row + dx;
                    dst[idx] = f16::from_f32(frame_bgra[p + 2] as f32 * inv_255);
                    dst[plane + idx] = f16::from_f32(frame_bgra[p + 1] as f32 * inv_255);
                    dst[2 * plane + idx] = f16::from_f32(frame_bgra[p] as f32 * inv_255);
                }
            }
        } else {
            let Ok((_, dst)) = self.input_value.try_extract_tensor_mut::<f32>() else {
                return;
            };
            dst.fill(0.0);
            for dy in 0..nh {
                let sy = (((dy as f32 + 0.5) / scale) as usize).min(frame_h - 1);
                let row_off = sy * frame_w * 4;
                let dst_row = dy * size;
                for dx in 0..nw {
                    let sx = (((dx as f32 + 0.5) / scale) as usize).min(frame_w - 1);
                    let p = row_off + sx * 4;
                    let idx = dst_row + dx;
                    dst[idx] = frame_bgra[p + 2] as f32 * inv_255;
                    dst[plane + idx] = frame_bgra[p + 1] as f32 * inv_255;
                    dst[2 * plane + idx] = frame_bgra[p] as f32 * inv_255;
                }
            }
        }

        // DirectML IoBinding: reuses bound output buffer and owned input tensor
        if self.io_binding.bind_input(&self.input_name, &self.input_value).is_err() {
            return;
        }
        let Ok(outputs) = self.session.run_binding(&self.io_binding) else {
            return;
        };

        let inv_scale = 1.0f32 / scale;

        // Supports both standard FP32 output and True FP16 output (keep_io_types=False)
        if let Ok((shape, data)) = outputs[0].try_extract_tensor::<f32>() {
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            decode_yolo_output(
                &dims,
                |idx| data[idx],
                inv_scale,
                frame_w,
                frame_h,
                target_labels,
                conf_threshold,
                out_hits,
            );
        } else if let Ok((shape, data)) = outputs[0].try_extract_tensor::<f16>() {
            let dims: Vec<usize> = shape.iter().map(|&d| d as usize).collect();
            decode_yolo_output(
                &dims,
                |idx| data[idx].to_f32(),
                inv_scale,
                frame_w,
                frame_h,
                target_labels,
                conf_threshold,
                out_hits,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_yolo_output<F: Fn(usize) -> f32>(
    dims: &[usize],
    at: F,
    inv_scale: f32,
    frame_w: usize,
    frame_h: usize,
    target_labels: &HashSet<&'static str>,
    conf_threshold: f32,
    out_hits: &mut Vec<Detection>,
) {
    if dims.len() != 3 {
        return;
    }
    let (rows, cols, transposed) = if dims[1] < dims[2] {
        (dims[2], dims[1], true)
    } else {
        (dims[1], dims[2], false)
    };
    if cols <= 4 {
        return;
    }

    let num_classes = (cols - 4).min(NUDENET_LABELS.len());
    for r in 0..rows {
        let mut best_c = 0usize;
        let mut best_s = 0.0f32;
        for c in 0..num_classes {
            let s = if transposed {
                at((4 + c) * rows + r)
            } else {
                at(r * cols + 4 + c)
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
            (at(r), at(rows + r), at(2 * rows + r), at(3 * rows + r))
        } else {
            (at(r * cols), at(r * cols + 1), at(r * cols + 2), at(r * cols + 3))
        };
        let x1 = ((cx - bw * 0.5) * inv_scale).round() as i32;
        let y1 = ((cy - bh * 0.5) * inv_scale).round() as i32;
        let x2 = ((cx + bw * 0.5) * inv_scale).round() as i32;
        let y2 = ((cy + bh * 0.5) * inv_scale).round() as i32;
        if x2 > x1 + 4 && y2 > y1 + 4 {
            out_hits.push(Detection {
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

pub struct FastNSFWDetector {
    env: Environment,
    models: Vec<LoadedModel>,
    scout_model: Option<LoadedModel>,
    compute_device: String,
    use_fp16: bool,
    cascade_mode: bool,
    pub last_infer_ms: f32,
}

impl FastNSFWDetector {
    pub fn new(
        primary: &str,
        secondary: &str,
        compute_device: &str,
        use_fp16: bool,
        cascade_mode: bool,
    ) -> Result<Self> {
        let dll_path = find_onnxruntime_dll();
        let env = ort::init_from(&dll_path)?
            .with_name("CleanScreenGuard")
            .build()?;

        let mut det = Self {
            env,
            models: Vec::new(),
            scout_model: None,
            compute_device: compute_device.to_string(),
            use_fp16,
            cascade_mode,
            last_infer_ms: 0.0,
        };
        det.reload_models(primary, secondary, compute_device, use_fp16, cascade_mode)?;
        Ok(det)
    }

    pub fn status_text(&self) -> String {
        if self.models.is_empty() {
            return "No model loaded (place 640m.onnx or 320n.onnx in models/)".to_string();
        }
        let names = self
            .models
            .iter()
            .map(|m| format!("{} ({}px {})", m.name, m.input_size, m.precision_tag))
            .collect::<Vec<_>>()
            .join(" + ");
        let cascade_tag = if let Some(ref scout) = self.scout_model {
            format!("Cascade {} -> ", scout.name)
        } else {
            String::new()
        };
        let dev = match self.compute_device.as_str() {
            "power_saving" => "iGPU / Power-Saving (DirectML)",
            "high_performance" => "Dedicated GPU (DirectML)",
            "cpu" => "CPU",
            _ => "Auto GPU (DirectML)",
        };
        format!("{cascade_tag}{names}  |  {dev}")
    }

    pub fn reload_models(
        &mut self,
        primary: &str,
        secondary: &str,
        compute_device: &str,
        use_fp16: bool,
        cascade_mode: bool,
    ) -> Result<()> {
        self.compute_device = compute_device.to_string();
        self.use_fp16 = use_fp16;
        self.cascade_mode = cascade_mode;
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
            for fb_id in [BUILTIN_MODEL_640M_ID, BUILTIN_MODEL_ID] {
                if let Some(fb) = resolve_model_path(fb_id, false) {
                    if let Ok(m) = self.build_model(&fb, compute_device) {
                        loaded.push(m);
                        break;
                    }
                }
            }
        }

        // Load lightweight 320n scout automatically when primary is a 640px model
        let mut scout = None;
        let primary_is_heavy = loaded.len() == 1 && loaded[0].input_size >= 640;
        if cascade_mode && primary_is_heavy {
            if let Some(scout_path) = resolve_model_path(BUILTIN_MODEL_ID, prefer_fp16) {
                if !seen.contains(&scout_path) {
                    match self.build_model(&scout_path, compute_device) {
                        Ok(m) => scout = Some(m),
                        Err(_) if prefer_fp16 => {
                            if let Some(fp32) = resolve_model_path(BUILTIN_MODEL_ID, false) {
                                scout = self.build_model(&fp32, compute_device).ok();
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
        }

        self.models = loaded;
        self.scout_model = scout;
        Ok(())
    }

    fn build_model(&self, path: &Path, compute_device: &str) -> Result<LoadedModel> {
        let mut builder = Session::builder(&self.env)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("{e}"))?
            .with_parallel_execution(false)
            .map_err(|e| anyhow!("{e}"))?
            .with_intra_threads(2)
            .map_err(|e| anyhow!("{e}"))?;

        if compute_device != "cpu" {
            builder = builder
                .with_memory_pattern(false)
                .map_err(|e| anyhow!("{e}"))?;
            let pref = match compute_device {
                "power_saving" => PerformancePreference::MinimumPower,
                "high_performance" => PerformancePreference::HighPerformance,
                _ => PerformancePreference::Default,
            };
            let dml = DirectML::default().with_performance_preference(pref).build();
            builder = builder
                .with_execution_providers([dml])
                .map_err(|e| anyhow!("{e}"))?;
        }

        let mut session = builder.commit_from_file(path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("model.onnx")
            .to_string();

        let lower_name = name.to_lowercase();
        let input_size = if lower_name.contains("320") { 320 } else { 640 };

        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .unwrap_or_else(|| "images".to_string());
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .unwrap_or_else(|| "output0".to_string());

        let expects_fp16_input = session
            .inputs()
            .first()
            .map(|i| format!("{:?}", i.dtype()).to_lowercase().contains("float16"))
            .unwrap_or(false);
        let expects_fp16_output = session
            .outputs()
            .first()
            .map(|o| format!("{:?}", o.dtype()).to_lowercase().contains("float16"))
            .unwrap_or(false);

        let precision_tag = if lower_name.contains("int8") || lower_name.contains("qdq") {
            "INT8"
        } else if expects_fp16_input && expects_fp16_output {
            "FP16-IO"
        } else if expects_fp16_input || lower_name.contains("fp16") {
            "FP16"
        } else {
            "FP32"
        };

        let total = 3 * input_size * input_size;
        let input_value: DynValue = if expects_fp16_input {
            Tensor::from_array((
                [1usize, 3, input_size, input_size],
                vec![f16::ZERO; total].into_boxed_slice(),
            ))?
            .into_dyn()
        } else {
            Tensor::from_array((
                [1usize, 3, input_size, input_size],
                vec![0.0f32; total].into_boxed_slice(),
            ))?
            .into_dyn()
        };

        let mut io_binding = session.create_binding()?;
        let cpu_out_mem = MemoryInfo::new(
            AllocationDevice::CPU,
            0,
            AllocatorType::Device,
            MemoryType::CPUOutput,
        )?;
        io_binding.bind_output_to_device(&output_name, &cpu_out_mem)?;

        if io_binding.bind_input(&input_name, &input_value).is_ok() {
            let _ = session.run_binding(&io_binding);
        }

        Ok(LoadedModel {
            session,
            io_binding,
            input_name,
            input_value,
            name,
            input_size,
            precision_tag,
            expects_fp16_input,
        })
    }

    pub fn detect(
        &mut self,
        frame_bgra: &[u8],
        frame_w: usize,
        frame_h: usize,
        target_labels: &HashSet<&'static str>,
        conf_threshold: f32,
        allow_scout: bool,
        scout_threshold: f32,
    ) -> Vec<Detection> {
        if self.models.is_empty() || target_labels.is_empty() {
            return Vec::new();
        }
        let t0 = Instant::now();

        // Stage 1: Fast 320n Scout on clean frames (skips 640m when screen is clean)
        if allow_scout && self.cascade_mode {
            if let Some(ref mut scout) = self.scout_model {
                let mut scout_hits = Vec::new();
                let thresh = scout_threshold.min(conf_threshold).clamp(0.08, 0.22);
                scout.run_on_frame(
                    frame_bgra,
                    frame_w,
                    frame_h,
                    target_labels,
                    thresh,
                    &mut scout_hits,
                );
                if scout_hits.is_empty() {
                    self.last_infer_ms = t0.elapsed().as_secs_f32() * 1000.0;
                    return Vec::new();
                }
            }
        }

        // Stage 2: Full-accuracy 640m Verifier
        let mut raw_hits: Vec<Detection> = Vec::new();
        for model in &mut self.models {
            model.run_on_frame(
                frame_bgra,
                frame_w,
                frame_h,
                target_labels,
                conf_threshold,
                &mut raw_hits,
            );
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
