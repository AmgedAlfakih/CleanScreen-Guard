from dataclasses import dataclass
from typing import List, Tuple


@dataclass
class TrackedBox:
    x1: float
    y1: float
    x2: float
    y2: float
    label_text: str
    last_seen: float


def _iou(a: Tuple[float, float, float, float], b: Tuple[float, float, float, float]) -> float:
    ix1 = max(a[0], b[0])
    iy1 = max(a[1], b[1])
    ix2 = min(a[2], b[2])
    iy2 = min(a[3], b[3])
    iw = max(0.0, ix2 - ix1)
    ih = max(0.0, iy2 - iy1)
    inter = iw * ih
    if inter <= 0.0:
        return 0.0
    area_a = max(1.0, (a[2] - a[0]) * (a[3] - a[1]))
    area_b = max(1.0, (b[2] - b[0]) * (b[3] - b[1]))
    return inter / (area_a + area_b - inter)


class RegionTracker:
    """Per-region anti-flicker tracker with independent hold timers."""

    def __init__(self, smooth_alpha: float = 0.65) -> None:
        self._tracks: List[TrackedBox] = []
        self._alpha = smooth_alpha

    def clear(self) -> None:
        self._tracks.clear()

    def update(
        self,
        detections: List[Tuple[Tuple[int, int, int, int], str]],
        now: float,
        hold_seconds: float,
    ) -> List[Tuple[Tuple[int, int, int, int], str]]:
        matched_track_indices = set()

        for (x1, y1, x2, y2), label_text in detections:
            best_idx = -1
            best_iou = 0.15
            cand = (float(x1), float(y1), float(x2), float(y2))

            for idx, tr in enumerate(self._tracks):
                if idx in matched_track_indices:
                    continue
                score = _iou((tr.x1, tr.y1, tr.x2, tr.y2), cand)
                if score > best_iou:
                    best_iou = score
                    best_idx = idx

            if best_idx >= 0:
                tr = self._tracks[best_idx]
                a = self._alpha
                tr.x1 = a * x1 + (1.0 - a) * tr.x1
                tr.y1 = a * y1 + (1.0 - a) * tr.y1
                tr.x2 = a * x2 + (1.0 - a) * tr.x2
                tr.y2 = a * y2 + (1.0 - a) * tr.y2
                tr.label_text = label_text
                tr.last_seen = now
                matched_track_indices.add(best_idx)
            else:
                self._tracks.append(
                    TrackedBox(
                        x1=float(x1),
                        y1=float(y1),
                        x2=float(x2),
                        y2=float(y2),
                        label_text=label_text,
                        last_seen=now,
                    )
                )
                matched_track_indices.add(len(self._tracks) - 1)

        self._tracks = [tr for tr in self._tracks if (now - tr.last_seen) <= hold_seconds]

        return [
            ((int(round(tr.x1)), int(round(tr.y1)), int(round(tr.x2)), int(round(tr.y2))), tr.label_text)
            for tr in self._tracks
        ]
