from dataclasses import asdict, dataclass
import json
import os
from typing import Optional, Set

from src.paths import get_settings_path

APP_VERSION = "1.0.0"
# Automatically set to your real repo URL when built via GitHub Actions,
# or edit this line manually before running build_exe.bat locally:
OFFICIAL_GITHUB_URL = "https://github.com/AmgedAlfakih/CleanScreen-Guard"

BUILTIN_MODEL_ID = "__builtin_nudenet_320n__"
NONE_MODEL_ID = ""


@dataclass
class AppConfig:
    language: str = "en"

    primary_model: str = BUILTIN_MODEL_ID
    secondary_model: str = NONE_MODEL_ID

    target_fps: int = 20
    scan_mode: str = "balanced"
    max_cpu_threads: int = 2

    confidence_threshold: float = 0.22
    box_hold_seconds: float = 0.35
    box_padding_px: int = 28

    blur_strength: int = 85
    blur_kernel: int = 15
    debug_overlay: bool = False

    blur_genitalia_exposed: bool = True
    blur_breasts_exposed: bool = True
    blur_buttocks_exposed: bool = True
    blur_anus_exposed: bool = True
    blur_bikini_covered: bool = True
    blur_belly_exposed: bool = True

    @property
    def blur_downscale(self) -> float:
        clamped = max(10, min(100, self.blur_strength))
        return 0.275 - (clamped / 100.0) * 0.25

    @property
    def active_labels(self) -> Set[str]:
        labels: Set[str] = set()
        if self.blur_genitalia_exposed:
            labels.update({"FEMALE_GENITALIA_EXPOSED", "MALE_GENITALIA_EXPOSED"})
        if self.blur_breasts_exposed:
            labels.add("FEMALE_BREAST_EXPOSED")
        if self.blur_buttocks_exposed:
            labels.add("BUTTOCKS_EXPOSED")
        if self.blur_anus_exposed:
            labels.add("ANUS_EXPOSED")
        if self.blur_bikini_covered:
            labels.update({
                "FEMALE_GENITALIA_COVERED",
                "FEMALE_BREAST_COVERED",
                "BUTTOCKS_COVERED",
                "ANUS_COVERED",
            })
        if self.blur_belly_exposed:
            labels.add("BELLY_EXPOSED")
        return labels

    def save(self, path: Optional[str] = None) -> None:
        target = path or get_settings_path()
        try:
            os.makedirs(os.path.dirname(os.path.abspath(target)), exist_ok=True)
            with open(target, "w", encoding="utf-8") as f:
                json.dump(asdict(self), f, indent=2, ensure_ascii=False)
        except Exception as exc:
            print(f"[Warning] Could not save settings to {target}: {exc}")

    @classmethod
    def load(cls, path: Optional[str] = None) -> "AppConfig":
        target = path or get_settings_path()
        cfg = cls()
        if not os.path.isfile(target):
            return cfg
        try:
            with open(target, "r", encoding="utf-8") as f:
                data = json.load(f)
            valid_keys = asdict(cfg).keys()
            for key, value in data.items():
                if key in valid_keys:
                    setattr(cfg, key, value)
        except Exception as exc:
            print(f"[Warning] Could not load settings from {target}, using defaults: {exc}")
        return cfg
