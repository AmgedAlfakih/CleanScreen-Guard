from typing import List, Tuple
from PySide6.QtCore import Qt, QRectF
from PySide6.QtGui import QColor, QFont, QGuiApplication, QImage, QPainter, QPen
from PySide6.QtWidgets import QWidget

from src.win32_utils import exclude_window_from_capture, make_window_click_through

BlurPatch = Tuple[Tuple[int, int, int, int], QImage, str]


class BlurOverlay(QWidget):
    def __init__(self, monitor_index: int = 0) -> None:
        super().__init__()
        self._patches: List[BlurPatch] = []
        self._excluded = False
        self._debug_mode = False

        self.setWindowFlags(
            Qt.WindowType.FramelessWindowHint
            | Qt.WindowType.WindowStaysOnTopHint
            | Qt.WindowType.Tool
            | Qt.WindowType.WindowTransparentForInput
        )
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WidgetAttribute.WA_ShowWithoutActivating, True)

        screens = QGuiApplication.screens()
        screen = screens[monitor_index] if monitor_index < len(screens) else screens[0]
        self.setGeometry(screen.geometry())

    def showEvent(self, event) -> None:
        super().showEvent(event)
        if not self._excluded:
            hwnd = int(self.winId())
            make_window_click_through(hwnd)
            exclude_window_from_capture(hwnd)
            self._excluded = True

    def set_debug_mode(self, enabled: bool) -> None:
        self._debug_mode = enabled
        self.update()

    def update_patches(self, patches: List[BlurPatch]) -> None:
        self._patches = patches
        self.update()

    def clear(self) -> None:
        if self._patches:
            self._patches = []
            self.update()

    def paintEvent(self, event) -> None:
        if not self._patches:
            return

        painter = QPainter(self)
        dpr = self.devicePixelRatioF()
        for (x1, y1, x2, y2), qimg, debug_text in self._patches:
            target = QRectF(x1 / dpr, y1 / dpr, (x2 - x1) / dpr, (y2 - y1) / dpr)
            painter.drawImage(target, qimg)

            if self._debug_mode and debug_text:
                painter.setPen(QPen(QColor("#EF4444"), 2))
                painter.drawRect(target)
                painter.setFont(QFont("Segoe UI", 9, QFont.Weight.Bold))
                painter.drawText(target.topLeft().x() + 4, target.topLeft().y() + 14, debug_text)
        painter.end()
