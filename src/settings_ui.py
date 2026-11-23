import html
import os
from typing import Tuple
from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QIcon
from PySide6.QtWidgets import (
    QCheckBox,
    QComboBox,
    QDialog,
    QGridLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSlider,
    QVBoxLayout,
)

from src.config import APP_VERSION, AppConfig, NONE_MODEL_ID, OFFICIAL_GITHUB_URL
from src.detector import list_available_models
from src.i18n import SUPPORTED_LANGUAGES, is_rtl, tr
from src.paths import get_models_dir
from src.win32_utils import is_startup_enabled, set_startup_enabled


class SettingsDialog(QDialog):
    """Live Settings window with English/Arabic RTL, Windows Auto-Start, Model Selector, and Official GitHub Footer."""

    settings_changed = Signal()
    models_changed = Signal(str, str)
    language_changed = Signal(str)

    def __init__(self, cfg: AppConfig, app_icon: QIcon, parent=None) -> None:
        super().__init__(parent)
        self._cfg = cfg
        self._updating_ui = False
        self._last_model_status: Tuple[str, str] = ("", "loading")

        self.setWindowIcon(app_icon)
        self.setMinimumWidth(520)
        self.setWindowFlags(self.windowFlags() | Qt.WindowType.WindowStaysOnTopHint)
        self._apply_dark_theme()
        self._build_ui()
        self._load_from_config()
        self._retranslate_ui()

    def _apply_dark_theme(self) -> None:
        self.setStyleSheet("""
            QDialog { background-color: #18181B; color: #F4F4F5; font-family: "Segoe UI"; font-size: 13px; }
            QGroupBox {
                border: 1px solid #3F3F46; border-radius: 6px; margin-top: 10px;
                padding-top: 12px; font-weight: bold; color: #E4E4E7;
            }
            QGroupBox::title { subcontrol-origin: margin; left: 10px; right: 10px; padding: 0 4px; }
            QLabel { color: #D4D4D8; }
            QLabel#statusLbl { color: #60A5FA; font-size: 12px; font-weight: normal; }
            QLabel#footerLbl { color: #A1A1AA; font-size: 11.5px; padding-top: 4px; }
            QCheckBox { color: #F4F4F5; spacing: 6px; }
            QComboBox {
                background-color: #27272A; color: #F4F4F5; border: 1px solid #52525B;
                border-radius: 4px; padding: 4px 8px;
            }
            QPushButton {
                background-color: #2563EB; color: white; border: none;
                border-radius: 5px; padding: 6px 12px; font-weight: bold;
            }
            QPushButton:hover { background-color: #1D4ED8; }
            QPushButton#secondaryBtn { background-color: #3F3F46; }
            QPushButton#secondaryBtn:hover { background-color: #52525B; }
        """)

    def _make_slider_row(self, min_v: int, max_v: int, on_change):
        slider = QSlider(Qt.Orientation.Horizontal)
        slider.setRange(min_v, max_v)
        val_label = QLabel()
        val_label.setMinimumWidth(72)
        val_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        slider.valueChanged.connect(on_change)
        return slider, val_label

    def _build_ui(self) -> None:
        root = QVBoxLayout(self)
        root.setSpacing(10)

        self.lang_group = QGroupBox()
        gen_grid = QGridLayout(self.lang_group)

        self.lbl_language = QLabel()
        self.lang_combo = QComboBox()
        for display_name, lang_code in SUPPORTED_LANGUAGES:
            self.lang_combo.addItem(display_name, lang_code)
        self.lang_combo.currentIndexChanged.connect(self._on_language_changed)

        self.chk_autostart = QCheckBox()
        self.chk_autostart.toggled.connect(self._on_autostart_toggled)

        gen_grid.addWidget(self.lbl_language, 0, 0)
        gen_grid.addWidget(self.lang_combo, 0, 1)
        gen_grid.addWidget(self.chk_autostart, 1, 0, 1, 2)
        root.addWidget(self.lang_group)

        self.model_group = QGroupBox()
        model_grid = QGridLayout(self.model_group)

        self.lbl_primary_model = QLabel()
        self.primary_combo = QComboBox()
        self.primary_combo.currentIndexChanged.connect(self._on_model_changed)
        model_grid.addWidget(self.lbl_primary_model, 0, 0)
        model_grid.addWidget(self.primary_combo, 0, 1, 1, 2)

        self.lbl_secondary_model = QLabel()
        self.secondary_combo = QComboBox()
        self.secondary_combo.currentIndexChanged.connect(self._on_model_changed)
        model_grid.addWidget(self.lbl_secondary_model, 1, 0)
        model_grid.addWidget(self.secondary_combo, 1, 1, 1, 2)

        self.btn_open_models = QPushButton()
        self.btn_open_models.setObjectName("secondaryBtn")
        self.btn_open_models.clicked.connect(self._open_models_folder)

        self.btn_refresh = QPushButton()
        self.btn_refresh.setObjectName("secondaryBtn")
        self.btn_refresh.clicked.connect(self._refresh_model_lists)

        self.status_label = QLabel()
        self.status_label.setObjectName("statusLbl")

        model_grid.addWidget(self.status_label, 2, 0, 1, 1)
        model_grid.addWidget(self.btn_open_models, 2, 1)
        model_grid.addWidget(self.btn_refresh, 2, 2)

        root.addWidget(self.model_group)

        self.det_group = QGroupBox()
        det_grid = QGridLayout(self.det_group)

        self.lbl_sensitivity = QLabel()
        self.conf_slider, self.conf_val = self._make_slider_row(10, 75, self._on_ui_changed)
        det_grid.addWidget(self.lbl_sensitivity, 0, 0)
        det_grid.addWidget(self.conf_slider, 0, 1)
        det_grid.addWidget(self.conf_val, 0, 2)

        self.lbl_hold_time = QLabel()
        self.hold_slider, self.hold_val = self._make_slider_row(5, 150, self._on_ui_changed)
        det_grid.addWidget(self.lbl_hold_time, 1, 0)
        det_grid.addWidget(self.hold_slider, 1, 1)
        det_grid.addWidget(self.hold_val, 1, 2)

        self.lbl_padding = QLabel()
        self.pad_slider, self.pad_val = self._make_slider_row(0, 80, self._on_ui_changed)
        det_grid.addWidget(self.lbl_padding, 2, 0)
        det_grid.addWidget(self.pad_slider, 2, 1)
        det_grid.addWidget(self.pad_val, 2, 2)

        root.addWidget(self.det_group)

        self.perf_group = QGroupBox()
        perf_grid = QGridLayout(self.perf_group)

        self.lbl_blur_strength = QLabel()
        self.blur_slider, self.blur_val = self._make_slider_row(10, 100, self._on_ui_changed)
        perf_grid.addWidget(self.lbl_blur_strength, 0, 0)
        perf_grid.addWidget(self.blur_slider, 0, 1)
        perf_grid.addWidget(self.blur_val, 0, 2)

        self.lbl_fps = QLabel()
        self.fps_slider, self.fps_val = self._make_slider_row(5, 30, self._on_ui_changed)
        perf_grid.addWidget(self.lbl_fps, 1, 0)
        perf_grid.addWidget(self.fps_slider, 1, 1)
        perf_grid.addWidget(self.fps_val, 1, 2)

        self.lbl_scan_mode = QLabel()
        self.mode_combo = QComboBox()
        self.mode_combo.addItem("", "fast")
        self.mode_combo.addItem("", "balanced")
        self.mode_combo.addItem("", "max_detail")
        self.mode_combo.currentIndexChanged.connect(self._on_ui_changed)
        perf_grid.addWidget(self.lbl_scan_mode, 2, 0)
        perf_grid.addWidget(self.mode_combo, 2, 1, 1, 2)

        self.chk_debug = QCheckBox()
        self.chk_debug.toggled.connect(self._on_ui_changed)
        perf_grid.addWidget(self.chk_debug, 3, 0, 1, 3)

        root.addWidget(self.perf_group)

        self.cat_group = QGroupBox()
        cat_grid = QGridLayout(self.cat_group)

        self.chk_genitalia = QCheckBox()
        self.chk_breasts = QCheckBox()
        self.chk_buttocks = QCheckBox()
        self.chk_anus = QCheckBox()
        self.chk_bikini = QCheckBox()
        self.chk_belly = QCheckBox()

        for cb in (
            self.chk_genitalia,
            self.chk_breasts,
            self.chk_buttocks,
            self.chk_anus,
            self.chk_bikini,
            self.chk_belly,
        ):
            cb.toggled.connect(self._on_ui_changed)

        cat_grid.addWidget(self.chk_genitalia, 0, 0)
        cat_grid.addWidget(self.chk_breasts, 0, 1)
        cat_grid.addWidget(self.chk_buttocks, 1, 0)
        cat_grid.addWidget(self.chk_anus, 1, 1)
        cat_grid.addWidget(self.chk_bikini, 2, 0)
        cat_grid.addWidget(self.chk_belly, 2, 1)

        root.addWidget(self.cat_group)

        btn_row = QHBoxLayout()
        self.reset_btn = QPushButton()
        self.reset_btn.setObjectName("secondaryBtn")
        self.reset_btn.clicked.connect(self._reset_defaults)

        self.close_btn = QPushButton()
        self.close_btn.clicked.connect(self.hide)

        btn_row.addWidget(self.reset_btn)
        btn_row.addStretch()
        btn_row.addWidget(self.close_btn)
        root.addLayout(btn_row)

        # --- Official Free & Open-Source GitHub Footer ---
        self.footer_label = QLabel()
        self.footer_label.setObjectName("footerLbl")
        self.footer_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.footer_label.setTextFormat(Qt.TextFormat.RichText)
        self.footer_label.setOpenExternalLinks(True)
        self.footer_label.setTextInteractionFlags(Qt.TextInteractionFlag.TextBrowserInteraction)
        root.addWidget(self.footer_label)

    def _retranslate_ui(self) -> None:
        lang = self._cfg.language
        direction = Qt.LayoutDirection.RightToLeft if is_rtl(lang) else Qt.LayoutDirection.LeftToRight
        self.setLayoutDirection(direction)

        self.setWindowTitle(tr(lang, "window_title"))
        self.lang_group.setTitle(tr(lang, "grp_general"))
        self.lbl_language.setText(tr(lang, "lbl_language"))
        self.chk_autostart.setText(tr(lang, "chk_autostart"))

        self.model_group.setTitle(tr(lang, "grp_models"))
        self.lbl_primary_model.setText(tr(lang, "lbl_primary_model"))
        self.lbl_secondary_model.setText(tr(lang, "lbl_secondary_model"))
        self.btn_open_models.setText(tr(lang, "btn_open_models"))
        self.btn_refresh.setText(tr(lang, "btn_refresh_models"))

        self.det_group.setTitle(tr(lang, "grp_detection"))
        self.lbl_sensitivity.setText(tr(lang, "lbl_sensitivity"))
        self.lbl_hold_time.setText(tr(lang, "lbl_hold_time"))
        self.lbl_padding.setText(tr(lang, "lbl_padding"))

        self.perf_group.setTitle(tr(lang, "grp_performance"))
        self.lbl_blur_strength.setText(tr(lang, "lbl_blur_strength"))
        self.lbl_fps.setText(tr(lang, "lbl_fps"))
        self.lbl_scan_mode.setText(tr(lang, "lbl_scan_mode"))
        self.mode_combo.setItemText(0, tr(lang, "mode_fast"))
        self.mode_combo.setItemText(1, tr(lang, "mode_balanced"))
        self.mode_combo.setItemText(2, tr(lang, "mode_max_detail"))
        self.chk_debug.setText(tr(lang, "chk_debug"))

        self.cat_group.setTitle(tr(lang, "grp_categories"))
        self.chk_genitalia.setText(tr(lang, "chk_genitalia"))
        self.chk_breasts.setText(tr(lang, "chk_breasts"))
        self.chk_buttocks.setText(tr(lang, "chk_buttocks"))
        self.chk_anus.setText(tr(lang, "chk_anus"))
        self.chk_bikini.setText(tr(lang, "chk_bikini"))
        self.chk_belly.setText(tr(lang, "chk_belly"))

        self.reset_btn.setText(tr(lang, "btn_reset"))
        self.close_btn.setText(tr(lang, "btn_done"))

        oss_txt = html.escape(tr(lang, "footer_oss"))
        gh_txt = html.escape(tr(lang, "footer_github_link"))
        url = html.escape(OFFICIAL_GITHUB_URL, quote=True)
        self.footer_label.setText(
            f'CleanScreen Guard v{APP_VERSION} — {oss_txt} • '
            f'<a href="{url}" style="color:#60A5FA; text-decoration:none;">{gh_txt}</a>'
        )

        self._refresh_model_lists()
        self._refresh_labels()
        self._render_model_status()

    def set_model_status(self, model_names: str, provider_key: str) -> None:
        self._last_model_status = (model_names, provider_key)
        self._render_model_status()

    def _render_model_status(self) -> None:
        lang = self._cfg.language
        names, prov = self._last_model_status
        if prov == "loading":
            self.status_label.setText(tr(lang, "status_loading"))
        elif prov == "switching":
            self.status_label.setText(tr(lang, "status_switching"))
        elif prov == "none" or not names:
            self.status_label.setText(tr(lang, "status_no_model"))
        else:
            prov_txt = tr(lang, "provider_gpu") if prov == "gpu" else tr(lang, "provider_cpu")
            self.status_label.setText(f"{names}  |  {prov_txt}")

    def _open_models_folder(self) -> None:
        models_path = get_models_dir()
        os.makedirs(models_path, exist_ok=True)
        if hasattr(os, "startfile"):
            os.startfile(models_path)

    def _refresh_model_lists(self) -> None:
        self._updating_ui = True
        lang = self._cfg.language
        available = list_available_models(lang=lang)

        self.primary_combo.clear()
        for label, mid in available:
            self.primary_combo.addItem(label, mid)

        self.secondary_combo.clear()
        self.secondary_combo.addItem(tr(lang, "model_none"), NONE_MODEL_ID)
        for label, mid in available:
            self.secondary_combo.addItem(label, mid)

        p_idx = self.primary_combo.findData(self._cfg.primary_model)
        self.primary_combo.setCurrentIndex(p_idx if p_idx >= 0 else 0)

        s_idx = self.secondary_combo.findData(self._cfg.secondary_model)
        self.secondary_combo.setCurrentIndex(s_idx if s_idx >= 0 else 0)
        self._updating_ui = False

    def _on_language_changed(self) -> None:
        if self._updating_ui:
            return
        new_lang = str(self.lang_combo.currentData())
        self._cfg.language = new_lang
        self._cfg.save()
        self._retranslate_ui()
        self.language_changed.emit(new_lang)

    def _on_autostart_toggled(self, checked: bool) -> None:
        if self._updating_ui:
            return
        set_startup_enabled(checked)

    def _on_model_changed(self) -> None:
        if self._updating_ui:
            return
        self._cfg.primary_model = str(self.primary_combo.currentData())
        self._cfg.secondary_model = str(self.secondary_combo.currentData())
        self._cfg.save()
        self.set_model_status("", "switching")
        self.models_changed.emit(self._cfg.primary_model, self._cfg.secondary_model)

    def _load_from_config(self) -> None:
        self._updating_ui = True
        lang_idx = self.lang_combo.findData(self._cfg.language)
        self.lang_combo.setCurrentIndex(lang_idx if lang_idx >= 0 else 0)
        self.chk_autostart.setChecked(is_startup_enabled())

        self.conf_slider.setValue(int(round(self._cfg.confidence_threshold * 100)))
        self.hold_slider.setValue(int(round(self._cfg.box_hold_seconds * 100)))
        self.pad_slider.setValue(self._cfg.box_padding_px)
        self.blur_slider.setValue(self._cfg.blur_strength)
        self.fps_slider.setValue(self._cfg.target_fps)

        idx = self.mode_combo.findData(self._cfg.scan_mode)
        if idx >= 0:
            self.mode_combo.setCurrentIndex(idx)

        self.chk_debug.setChecked(self._cfg.debug_overlay)
        self.chk_genitalia.setChecked(self._cfg.blur_genitalia_exposed)
        self.chk_breasts.setChecked(self._cfg.blur_breasts_exposed)
        self.chk_buttocks.setChecked(self._cfg.blur_buttocks_exposed)
        self.chk_anus.setChecked(self._cfg.blur_anus_exposed)
        self.chk_bikini.setChecked(self._cfg.blur_bikini_covered)
        self.chk_belly.setChecked(self._cfg.blur_belly_exposed)

        self._refresh_labels()
        self._updating_ui = False

    def _refresh_labels(self) -> None:
        lang = self._cfg.language
        self.conf_val.setText(f"{self.conf_slider.value() / 100.0:.2f}")
        self.hold_val.setText(f"{self.hold_slider.value() / 100.0:.2f} {tr(lang, 'unit_sec')}")
        self.pad_val.setText(f"{self.pad_slider.value()} {tr(lang, 'unit_px')}")
        self.blur_val.setText(f"{self.blur_slider.value()}%")
        self.fps_val.setText(f"{self.fps_slider.value()} {tr(lang, 'unit_fps')}")

    def _on_ui_changed(self) -> None:
        if self._updating_ui:
            return
        self._refresh_labels()

        self._cfg.confidence_threshold = self.conf_slider.value() / 100.0
        self._cfg.box_hold_seconds = self.hold_slider.value() / 100.0
        self._cfg.box_padding_px = self.pad_slider.value()
        self._cfg.blur_strength = self.blur_slider.value()
        self._cfg.target_fps = self.fps_slider.value()
        self._cfg.scan_mode = str(self.mode_combo.currentData())
        self._cfg.debug_overlay = self.chk_debug.isChecked()

        self._cfg.blur_genitalia_exposed = self.chk_genitalia.isChecked()
        self._cfg.blur_breasts_exposed = self.chk_breasts.isChecked()
        self._cfg.blur_buttocks_exposed = self.chk_buttocks.isChecked()
        self._cfg.blur_anus_exposed = self.chk_anus.isChecked()
        self._cfg.blur_bikini_covered = self.chk_bikini.isChecked()
        self._cfg.blur_belly_exposed = self.chk_belly.isChecked()

        self._cfg.save()
        self.settings_changed.emit()

    def _reset_defaults(self) -> None:
        current_lang = self._cfg.language
        defaults = AppConfig(language=current_lang)
        for field_name in (
            "primary_model",
            "secondary_model",
            "target_fps",
            "scan_mode",
            "confidence_threshold",
            "box_hold_seconds",
            "box_padding_px",
            "blur_strength",
            "debug_overlay",
            "blur_genitalia_exposed",
            "blur_breasts_exposed",
            "blur_buttocks_exposed",
            "blur_anus_exposed",
            "blur_bikini_covered",
            "blur_belly_exposed",
        ):
            setattr(self._cfg, field_name, getattr(defaults, field_name))
        self._load_from_config()
        self._retranslate_ui()
        self._cfg.save()
        self.models_changed.emit(self._cfg.primary_model, self._cfg.secondary_model)
        self.settings_changed.emit()
