from typing import Optional
import dxcam
import numpy as np


class DesktopCapturer:
    def __init__(self, monitor_index: int = 0) -> None:
        self.monitor_index = monitor_index
        self._camera = dxcam.create(output_idx=monitor_index, output_color="BGR")
        if self._camera is None:
            raise RuntimeError(f"Failed to initialize DXGI capture on monitor {monitor_index}.")

    def grab(self) -> Optional[np.ndarray]:
        """Return the latest BGR frame, or None if the desktop hasn't changed."""
        return self._camera.grab()

    def close(self) -> None:
        if self._camera is not None:
            del self._camera
            self._camera = None
