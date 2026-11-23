import ast
from dataclasses import dataclass
import os
from typing import Dict, List, Optional, Set, Tuple
import cv2
import numpy as np
import onnxruntime as ort

from src.config import BUILTIN_MODEL_ID, NONE_MODEL_ID
from src.i18n import tr
from src.paths import get_bundle_dir, get_data_dir, get_exe_dir, get_models_dir

NUDENET_LABELS = [
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
]

SAFE_CLASS_KEYWORDS = ("face", "feet", "foot", "armpit", "safe", "sfw", "clothes", "clothed")


@dataclass
class Detection:
    x1: int
    y1: int
    x2: int
    y2: int
    score: float
    label: str


def list_available_models(lang: str = "en") -> List[Tuple[str, str]]:
    r"""Scan both %LOCALAPPDATA%\CleanScreenGuard\models and install-folder models/."""
    items: List[Tuple[str, str]] = [(tr(lang, "model_builtin"), BUILTIN_MODEL_ID)]
    seen_names: Set[str] = set()

    candidate_dirs = [get_models_dir(), os.path.join(get_exe_dir(), "models")]
    for mdir in candidate_dirs:
        if not os.path.isdir(mdir):
            continue
        for fname in sorted(os.listdir(mdir)):
            if not fname.lower().endswith(".onnx"):
                continue
            key = fname.lower()
            if key in seen_names:
                continue
            seen_names.add(key)
            abs_path = os.path.join(mdir, fname)
            rel_id = os.path.join("models", fname)
            size_mb = os.path.getsize(abs_path) / (1024 * 1024)
            items.append((f"{fname} ({size_mb:.1f} MB)", rel_id))
    return items


def resolve_model_path(model_id: str) -> Optional[str]:
    if not model_id or model_id == NONE_MODEL_ID:
        return None

    if model_id == BUILTIN_MODEL_ID:
        bundled = os.path.join(get_bundle_dir(), "nudenet", "320n.onnx")
        if os.path.isfile(bundled):
            return bundled
        local_320 = os.path.join(get_models_dir(), "320n.onnx")
        if os.path.isfile(local_320):
            return local_320
        try:
            import nudenet

            pkg_320 = os.path.join(os.path.dirname(nudenet.__file__), "320n.onnx")
            if os.path.isfile(pkg_320):
                return pkg_320
        except Exception:
            pass
        return None

    if os.path.isabs(model_id) and os.path.isfile(model_id):
        return model_id

    for base in (get_data_dir(), get_exe_dir()):
        candidate = os.path.join(base, model_id)
        if os.path.isfile(candidate):
            return candidate

    if os.path.isfile(model_id):
        return model_id

    return None


def _parse_metadata_labels(session: ort.InferenceSession) -> Optional[Dict[int, str]]:
    try:
        meta = session.get_modelmeta().custom_metadata_map
        raw_names = meta.get("names")
        if not raw_names:
            return None
        parsed = ast.literal_eval(raw_names)
        if isinstance(parsed, dict):
            return {int(k): str(v) for k, v in parsed.items()}
        if isinstance(parsed, list):
            return {i: str(v) for i, v in enumerate(parsed)}
    except Exception:
        return None
    return None


class SingleOnnxYoloModel:
    """Loads any NudeNet or Ultralytics YOLO (.onnx) model (det, segm, or end-to-end)."""

    def __init__(self, model_path: str, max_cpu_threads: int = 2) -> None:
        self.model_path = model_path
        self.model_name = os.path.basename(model_path)

        sess_opts = ort.SessionOptions()
        sess_opts.intra_op_num_threads = max(1, max_cpu_threads)
        sess_opts.inter_op_num_threads = 1
        sess_opts.enable_mem_pattern = False
        sess_opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

        available = ort.get_available_providers()
        providers = (
            ["DmlExecutionProvider", "CPUExecutionProvider"]
            if "DmlExecutionProvider" in available
            else ["CPUExecutionProvider"]
        )

        self._session = ort.InferenceSession(model_path, sess_options=sess_opts, providers=providers)
        inp = self._session.get_inputs()[0]
        self._input_name = inp.name
        self.active_provider = self._session.get_providers()[0]

        shape = inp.shape
        detected_size = 640
        if len(shape) == 4 and isinstance(shape[2], int) and shape[2] > 0:
            detected_size = int(shape[2])
        elif "320" in self.model_name.lower():
            detected_size = 320
        self.input_size = detected_size

        meta_labels = _parse_metadata_labels(self._session)
        out0_shape = self._session.get_outputs()[0].shape

        is_nudenet = False
        if meta_labels is not None:
            self.labels_map = meta_labels
            if any("FEMALE_BREAST_EXPOSED" in v for v in meta_labels.values()):
                is_nudenet = True
        else:
            ch_dim = out0_shape[1] if (len(out0_shape) == 3 and isinstance(out0_shape[1], int)) else None
            if ch_dim == 22 or "320n" in self.model_name.lower() or "640m" in self.model_name.lower():
                self.labels_map = {i: lbl for i, lbl in enumerate(NUDENET_LABELS)}
                is_nudenet = True
            else:
                self.labels_map = {}

        self.is_nudenet = is_nudenet
        self._canvas = np.zeros((self.input_size, self.input_size, 3), dtype=np.uint8)
        self._blob = np.zeros((1, 3, self.input_size, self.input_size), dtype=np.float32)

        kind = "NudeNet" if self.is_nudenet else "Custom/Anime YOLO"
        print(f"[Model] Loaded {self.model_name} ({self.input_size}px, {kind}) on {self.active_provider}")

    def _should_keep_label(self, label: str, nudenet_target_labels: Set[str]) -> bool:
        if self.is_nudenet:
            return label in nudenet_target_labels
        lower = label.lower()
        return not any(safe_kw in lower for safe_kw in SAFE_CLASS_KEYWORDS)

    def run_crop(
        self,
        crop_bgr: np.ndarray,
        offset_x: int,
        offset_y: int,
        nudenet_target_labels: Set[str],
        conf_threshold: float,
    ) -> List[Tuple[List[int], float, str]]:
        ch, cw = crop_bgr.shape[:2]
        if cw < 16 or ch < 16:
            return []

        size = self.input_size
        scale = size / max(cw, ch)
        nw = max(1, int(round(cw * scale)))
        nh = max(1, int(round(ch * scale)))

        resized = cv2.resize(crop_bgr, (nw, nh), interpolation=cv2.INTER_LINEAR)
        self._canvas.fill(0)
        self._canvas[:nh, :nw] = resized

        rgb = cv2.cvtColor(self._canvas, cv2.COLOR_BGR2RGB)
        np.multiply(rgb.transpose(2, 0, 1), 1.0 / 255.0, out=self._blob[0], casting="unsafe")

        outputs = self._session.run(None, {self._input_name: self._blob})
        out0 = outputs[0]
        inv_scale = 1.0 / scale
        hits: List[Tuple[List[int], float, str]] = []

        if out0.ndim == 3 and out0.shape[2] == 6 and out0.shape[1] > 6:
            rows = out0[0]
            mask = rows[:, 4] >= conf_threshold
            for row in rows[mask]:
                x1_r, y1_r, x2_r, y2_r, sc, cid_f = row
                cid = int(round(cid_f))
                label = self.labels_map.get(cid, f"NSFW_{cid}")
                if not self._should_keep_label(label, nudenet_target_labels):
                    continue
                x1 = int(x1_r * inv_scale) + offset_x
                y1 = int(y1_r * inv_scale) + offset_y
                w_box = int((x2_r - x1_r) * inv_scale)
                h_box = int((y2_r - y1_r) * inv_scale)
                if w_box > 4 and h_box > 4:
                    hits.append(([x1, y1, w_box, h_box], float(sc), label))
            return hits

        preds = np.squeeze(out0, axis=0)
        if preds.ndim != 2:
            return []
        if preds.shape[0] < preds.shape[1]:
            preds = preds.T

        num_mask_coeffs = 0
        if len(outputs) >= 2 and outputs[1].ndim == 4:
            num_mask_coeffs = int(outputs[1].shape[1])

        cls_end = preds.shape[1] - num_mask_coeffs
        if cls_end <= 4:
            return []

        class_scores = preds[:, 4:cls_end]
        class_ids = np.argmax(class_scores, axis=1)
        scores = class_scores[np.arange(len(preds)), class_ids]

        mask = scores >= conf_threshold
        if not np.any(mask):
            return []

        boxes = preds[mask, :4]
        scores = scores[mask]
        class_ids = class_ids[mask]

        for (cx, cy, bw, bh), sc, cid in zip(boxes, scores, class_ids):
            cid_i = int(cid)
            label = self.labels_map.get(cid_i, f"NSFW_{cid_i}")
            if not self._should_keep_label(label, nudenet_target_labels):
                continue

            x1 = int((cx - bw * 0.5) * inv_scale) + offset_x
            y1 = int((cy - bh * 0.5) * inv_scale) + offset_y
            w_box = int(bw * inv_scale)
            h_box = int(bh * inv_scale)
            if w_box > 4 and h_box > 4:
                hits.append(([x1, y1, w_box, h_box], float(sc), label))

        return hits


class FastNSFWDetector:
    """Manages primary (+ optional secondary) ONNX models with hot-reload & smart ROI scanning."""

    def __init__(
        self,
        primary_model_id: str = BUILTIN_MODEL_ID,
        secondary_model_id: str = NONE_MODEL_ID,
        max_cpu_threads: int = 2,
    ) -> None:
        self._max_cpu_threads = max_cpu_threads
        self._models: List[SingleOnnxYoloModel] = []
        self._rr_index = 0
        self.reload_models(primary_model_id, secondary_model_id)

    def get_status_info(self) -> Tuple[str, str]:
        if not self._models:
            return ("", "none")
        names = " + ".join(f"{m.model_name} ({m.input_size}px)" for m in self._models)
        prov_key = "gpu" if "Dml" in self._models[0].active_provider else "cpu"
        return (names, prov_key)

    def reload_models(self, primary_model_id: str, secondary_model_id: str) -> Tuple[str, str]:
        new_models: List[SingleOnnxYoloModel] = []
        loaded_paths: Set[str] = set()

        for mid in (primary_model_id, secondary_model_id):
            path = resolve_model_path(mid)
            if path and path not in loaded_paths:
                try:
                    new_models.append(SingleOnnxYoloModel(path, max_cpu_threads=self._max_cpu_threads))
                    loaded_paths.add(path)
                except Exception as exc:
                    print(f"[Error] Failed to load model '{mid}': {exc}")

        if not new_models:
            fallback = resolve_model_path(BUILTIN_MODEL_ID)
            if fallback:
                new_models.append(SingleOnnxYoloModel(fallback, max_cpu_threads=self._max_cpu_threads))

        self._models = new_models
        return self.get_status_info()

    def _get_quadrants(self, w: int, h: int, overlap: float = 0.12) -> List[Tuple[int, int, int, int]]:
        ov_x = int(w * overlap)
        ov_y = int(h * overlap)
        mx, my = w // 2, h // 2
        return [
            (0, 0, mx + ov_x, my + ov_y),
            (mx - ov_x, 0, w, my + ov_y),
            (0, my - ov_y, mx + ov_x, h),
            (mx - ov_x, my - ov_y, w, h),
        ]

    def detect(
        self,
        frame_bgr: np.ndarray,
        target_labels: Set[str],
        conf_threshold: float = 0.22,
        scan_mode: str = "balanced",
        motion_roi: Optional[Tuple[int, int, int, int]] = None,
    ) -> List[Detection]:
        if not self._models:
            return []

        h, w = frame_bgr.shape[:2]
        crops: List[Tuple[int, int, np.ndarray]] = [(0, 0, frame_bgr)]

        if scan_mode == "fast":
            if motion_roi is not None:
                rx1, ry1, rx2, ry2 = motion_roi
                if (rx2 - rx1) < w * 0.85 or (ry2 - ry1) < h * 0.85:
                    crops.append((rx1, ry1, frame_bgr[ry1:ry2, rx1:rx2]))
        elif scan_mode == "balanced":
            if motion_roi is not None and (
                (motion_roi[2] - motion_roi[0]) < w * 0.85
                or (motion_roi[3] - motion_roi[1]) < h * 0.85
            ):
                rx1, ry1, rx2, ry2 = motion_roi
                crops.append((rx1, ry1, frame_bgr[ry1:ry2, rx1:rx2]))
            elif w >= 900 and h >= 600:
                quads = self._get_quadrants(w, h)
                qx1, qy1, qx2, qy2 = quads[self._rr_index % 4]
                self._rr_index += 1
                crops.append((qx1, qy1, frame_bgr[qy1:qy2, qx1:qx2]))
        else:
            if w >= 900 and h >= 600:
                for qx1, qy1, qx2, qy2 in self._get_quadrants(w, h):
                    crops.append((qx1, qy1, frame_bgr[qy1:qy2, qx1:qx2]))

        raw_boxes: List[List[int]] = []
        raw_scores: List[float] = []
        raw_labels: List[str] = []

        for model in self._models:
            if model.is_nudenet and not target_labels:
                continue
            for ox, oy, crop_img in crops:
                for box, sc, lbl in model.run_crop(crop_img, ox, oy, target_labels, conf_threshold):
                    x1 = max(0, box[0])
                    y1 = max(0, box[1])
                    x2 = min(w, box[0] + box[2])
                    y2 = min(h, box[1] + box[3])
                    if x2 > x1 and y2 > y1:
                        raw_boxes.append([x1, y1, x2 - x1, y2 - y1])
                        raw_scores.append(sc)
                        raw_labels.append(lbl)

        if not raw_boxes:
            return []

        indices = cv2.dnn.NMSBoxes(raw_boxes, raw_scores, conf_threshold, 0.35)
        detections: List[Detection] = []
        if len(indices) > 0:
            for idx in indices.flatten():
                x, y, bw, bh = raw_boxes[idx]
                detections.append(
                    Detection(
                        x1=x,
                        y1=y,
                        x2=x + bw,
                        y2=y + bh,
                        score=raw_scores[idx],
                        label=raw_labels[idx],
                    )
                )

        return detections
