import os
import sys
import time
from typing import Dict, List, Optional, Tuple
import cv2
import numpy as np
from PySide6.QtCore import Qt, QThread, Signal
from PySide6.QtGui import QAction, QColor, QGuiApplication, QIcon, QImage, QPixmap
from PySide6.QtWidgets import QApplication, QMenu, QSystemTrayIcon

from src.capture import DesktopCapturer
from src.config import AppConfig
from src.detector import Detection, FastNSFWDetector
from src.i18n import is_rtl, tr
from src.overlay import BlurOverlay, BlurPatch
from src.paths import get_asset_path
from src.settings_ui import SettingsDialog
from src.single_instance import SingleInstanceGuard
from src.tracker import RegionTracker
from src.win32_utils import set_app_user_model_id


def load_app_icon(filename: str, fallback_hex: str) -> QIcon:
    path = get_asset_path(filename)
    if os.path.isfile(path):
        icon = QIcon(path)
        if not icon.isNull():
            return icon
    pix = QPixmap(32, 32)
    pix.fill(QColor(fallback_hex))
    return QIcon(pix)


def blur_region(frame_bgr: np.ndarray, box: Tuple[int, int, int, int], cfg: AppConfig) -> QImage:
    x1, y1, x2, y2 = box
    crop = frame_bgr[y1:y2, x1:x2]
    h, w = crop.shape[:2]

    downscale = cfg.blur_downscale
    sw = max(1, int(w * downscale))
    sh = max(1, int(h * downscale))
    small = cv2.resize(crop, (sw, sh), interpolation=cv2.INTER_LINEAR)
    blurred = cv2.GaussianBlur(small, (cfg.blur_kernel, cfg.blur_kernel), 0)
    restored = cv2.resize(blurred, (w, h), interpolation=cv2.INTER_LINEAR)

    rgb = np.ascontiguousarray(cv2.cvtColor(restored, cv2.COLOR_BGR2RGB))
    qimg = QImage(rgb.data, w, h, 3 * w, QImage.Format.Format_RGB888)
    return qimg.copy()


def find_motion_roi(
    prev_small: Optional[np.ndarray],
    curr_small: np.ndarray,
    full_w: int,
    full_h: int,
) -> Optional[Tuple[int, int, int, int]]:
    if prev_small is None:
        return None

    diff = cv2.absdiff(prev_small, curr_small)
    _, thresh = cv2.threshold(diff, 14, 255, cv2.THRESH_BINARY)
    ys, xs = np.where(thresh > 0)
    if len(xs) < 12:
        return None

    sh, sw = curr_small.shape[:2]
    sx = full_w / sw
    sy = full_h / sh
    pad = 40

    x1 = max(0, int(xs.min() * sx) - pad)
    y1 = max(0, int(ys.min() * sy) - pad)
    x2 = min(full_w, int((xs.max() + 1) * sx) + pad)
    y2 = min(full_h, int((ys.max() + 1) * sy) + pad)

    if (x2 - x1) < 120 or (y2 - y1) < 120:
        return None
    return (x1, y1, x2, y2)


class CensorWorker(QThread):
    patches_ready = Signal(int, list)
    model_status_changed = Signal(str, str)

    def __init__(self, monitor_indices: List[int], config: AppConfig) -> None:
        super().__init__()
        self._monitor_indices = monitor_indices
        self._cfg = config
        self._running = True
        self._paused = False
        self._force_rescan = False
        self._pending_models: Optional[Tuple[str, str]] = None
        self._trackers: Dict[int, RegionTracker] = {i: RegionTracker() for i in monitor_indices}
        self._last_scan_time: Dict[int, float] = {i: 0.0 for i in monitor_indices}

    def set_paused(self, paused: bool) -> None:
        self._paused = paused
        if paused:
            for idx in self._monitor_indices:
                self._trackers[idx].clear()
                self.patches_ready.emit(idx, [])
        else:
            self._force_rescan = True

    def trigger_rescan(self) -> None:
        self._force_rescan = True

    def request_model_reload(self, primary_id: str, secondary_id: str) -> None:
        self._pending_models = (primary_id, secondary_id)
        self._force_rescan = True

    def stop(self) -> None:
        self._running = False
        self.wait()

    def run(self) -> None:
        detector = FastNSFWDetector(
            primary_model_id=self._cfg.primary_model,
            secondary_model_id=self._cfg.secondary_model,
            max_cpu_threads=self._cfg.max_cpu_threads,
        )
        names, prov = detector.get_status_info()
        self.model_status_changed.emit(names, prov)

        capturers: Dict[int, DesktopCapturer] = {}
        last_frames: Dict[int, Optional[np.ndarray]] = {}
        prev_small_gray: Dict[int, Optional[np.ndarray]] = {}

        for idx in self._monitor_indices:
            try:
                capturers[idx] = DesktopCapturer(monitor_index=idx)
                last_frames[idx] = None
                prev_small_gray[idx] = None
            except Exception as exc:
                print(f"[Warning] Skipping monitor {idx}: {exc}")

        try:
            while self._running:
                t0 = time.perf_counter()
                frame_interval = 1.0 / max(1, self._cfg.target_fps)

                if self._pending_models is not None:
                    p_id, s_id = self._pending_models
                    self._pending_models = None
                    names, prov = detector.reload_models(p_id, s_id)
                    self.model_status_changed.emit(names, prov)

                if self._paused:
                    time.sleep(0.05)
                    continue

                active_labels = self._cfg.active_labels
                scan_mode = self._cfg.scan_mode
                conf_threshold = self._cfg.confidence_threshold
                hold_seconds = self._cfg.box_hold_seconds
                pad = self._cfg.box_padding_px

                now = time.perf_counter()
                force = self._force_rescan
                self._force_rescan = False

                for idx, capturer in capturers.items():
                    frame = capturer.grab()
                    if frame is None and not force:
                        continue

                    if frame is not None:
                        last_frames[idx] = frame
                    current_frame = last_frames[idx]
                    if current_frame is None:
                        continue

                    h, w = current_frame.shape[:2]

                    small_gray = cv2.cvtColor(
                        cv2.resize(current_frame, (160, 90), interpolation=cv2.INTER_AREA),
                        cv2.COLOR_BGR2GRAY,
                    )
                    motion_roi = find_motion_roi(prev_small_gray[idx], small_gray, w, h)
                    prev_small_gray[idx] = small_gray

                    if (
                        motion_roi is None
                        and not force
                        and frame is not None
                        and not self._trackers[idx]._tracks
                        and (now - self._last_scan_time[idx]) < 0.5
                    ):
                        continue

                    self._last_scan_time[idx] = now
                    detections: List[Detection] = detector.detect(
                        current_frame,
                        active_labels,
                        conf_threshold=conf_threshold,
                        scan_mode=scan_mode,
                        motion_roi=motion_roi,
                    )

                    padded_detections = [
                        (
                            (
                                max(0, d.x1 - pad),
                                max(0, d.y1 - pad),
                                min(w, d.x2 + pad),
                                min(h, d.y2 + pad),
                            ),
                            f"{d.label} ({d.score:.2f})",
                        )
                        for d in detections
                    ]

                    active_boxes = self._trackers[idx].update(
                        padded_detections, now=now, hold_seconds=hold_seconds
                    )

                    patches: List[BlurPatch] = []
                    for (bx1, by1, bx2, by2), label_txt in active_boxes:
                        bx1 = max(0, min(w - 1, bx1))
                        by1 = max(0, min(h - 1, by1))
                        bx2 = max(0, min(w, bx2))
                        by2 = max(0, min(h, by2))
                        if bx2 > bx1 and by2 > by1:
                            box = (bx1, by1, bx2, by2)
                            patches.append((box, blur_region(current_frame, box, self._cfg), label_txt))

                    self.patches_ready.emit(idx, patches)

                elapsed = time.perf_counter() - t0
                if elapsed < frame_interval:
                    time.sleep(frame_interval - elapsed)
        finally:
            for capturer in capturers.values():
                capturer.close()


class TrayController:
    def __init__(
        self,
        app: QApplication,
        cfg: AppConfig,
        worker: CensorWorker,
        overlays: Dict[int, BlurOverlay],
        settings_dialog: SettingsDialog,
        active_icon: QIcon,
        paused_icon: QIcon,
    ) -> None:
        self._cfg = cfg
        self._worker = worker
        self._overlays = overlays
        self._settings_dialog = settings_dialog
        self._active_icon = active_icon
        self._paused_icon = paused_icon
        self._paused = False

        self.tray = QSystemTrayIcon(self._active_icon, app)
        self.tray.activated.connect(self._on_activated)

        self.menu = QMenu()
        self.settings_action = QAction(self.menu)
        self.settings_action.triggered.connect(self.open_settings)

        self.toggle_action = QAction(self.menu)
        self.toggle_action.triggered.connect(self._on_toggle)

        self.quit_action = QAction(self.menu)
        self.quit_action.triggered.connect(app.quit)

        self.menu.addAction(self.settings_action)
        self.menu.addAction(self.toggle_action)
        self.menu.addSeparator()
        self.menu.addAction(self.quit_action)

        self.tray.setContextMenu(self.menu)
        self.retranslate()
        self.tray.show()

    def open_settings(self) -> None:
        self._settings_dialog.show()
        self._settings_dialog.raise_()
        self._settings_dialog.activateWindow()

    def _on_activated(self, reason: QSystemTrayIcon.ActivationReason) -> None:
        if reason in (
            QSystemTrayIcon.ActivationReason.Trigger,
            QSystemTrayIcon.ActivationReason.DoubleClick,
        ):
            self.open_settings()

    def _on_toggle(self) -> None:
        self._paused = not self._paused
        self._worker.set_paused(self._paused)
        if self._paused:
            for ov in self._overlays.values():
                ov.clear()
        self.tray.setIcon(self._paused_icon if self._paused else self._active_icon)
        self.retranslate()

    def retranslate(self) -> None:
        lang = self._cfg.language
        direction = Qt.LayoutDirection.RightToLeft if is_rtl(lang) else Qt.LayoutDirection.LeftToRight
        self.menu.setLayoutDirection(direction)

        self.settings_action.setText(tr(lang, "tray_settings"))
        self.toggle_action.setText(tr(lang, "tray_resume" if self._paused else "tray_pause"))
        self.quit_action.setText(tr(lang, "tray_quit"))
        self.tray.setToolTip(tr(lang, "tray_tooltip_paused" if self._paused else "tray_tooltip_active"))


def main() -> None:
    set_app_user_model_id("CleanScreenGuard.App.1")

    app = QApplication(sys.argv)
    app.setApplicationName("CleanScreen Guard")
    app.setQuitOnLastWindowClosed(False)

    instance_guard = SingleInstanceGuard()
    if not instance_guard.try_acquire():
        sys.exit(0)

    active_icon = load_app_icon("app_icon.ico", "#DC2626")
    paused_icon = load_app_icon("app_icon_paused.ico", "#71717A")
    app.setWindowIcon(active_icon)

    cfg = AppConfig.load()
    num_screens = max(1, len(QGuiApplication.screens()))
    monitor_indices = list(range(num_screens))

    overlays: Dict[int, BlurOverlay] = {}
    for idx in monitor_indices:
        ov = BlurOverlay(monitor_index=idx)
        ov.set_debug_mode(cfg.debug_overlay)
        ov.show()
        overlays[idx] = ov

    worker = CensorWorker(monitor_indices, cfg)
    worker.patches_ready.connect(lambda idx, patches: overlays[idx].update_patches(patches))

    settings_dialog = SettingsDialog(cfg, app_icon=active_icon)
    worker.model_status_changed.connect(settings_dialog.set_model_status)
    settings_dialog.models_changed.connect(worker.request_model_reload)

    tray_ctrl = TrayController(app, cfg, worker, overlays, settings_dialog, active_icon, paused_icon)
    settings_dialog.language_changed.connect(lambda _lang: tray_ctrl.retranslate())
    instance_guard.activate_requested.connect(tray_ctrl.open_settings)

    def on_settings_changed() -> None:
        for ov in overlays.values():
            ov.set_debug_mode(cfg.debug_overlay)
        worker.trigger_rescan()

    settings_dialog.settings_changed.connect(on_settings_changed)

    worker.start()

    def on_quit() -> None:
        worker.stop()
        instance_guard.close()

    app.aboutToQuit.connect(on_quit)

    print(
        f"CleanScreen Guard v1.0.0 running across {num_screens} monitor(s). "
        "Click the CleanScreen Guard tray icon anytime to open Settings."
    )
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
