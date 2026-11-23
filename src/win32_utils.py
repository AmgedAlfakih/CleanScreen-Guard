import ctypes
from ctypes import wintypes
import os
import sys

try:
    import winreg
except ImportError:
    winreg = None

user32 = ctypes.WinDLL("user32", use_last_error=True)

WDA_NONE = 0x00000000
WDA_EXCLUDEFROMCAPTURE = 0x00000011

GWL_EXSTYLE = -20
WS_EX_TRANSPARENT = 0x00000020
WS_EX_TOOLWINDOW = 0x00000080
WS_EX_LAYERED = 0x00080000
WS_EX_NOACTIVATE = 0x08000000

RUN_REG_PATH = r"Software\Microsoft\Windows\CurrentVersion\Run"
RUN_VALUE_NAME = "CleanScreenGuard"

user32.SetWindowDisplayAffinity.argtypes = [wintypes.HWND, wintypes.DWORD]
user32.SetWindowDisplayAffinity.restype = wintypes.BOOL


def set_app_user_model_id(app_id: str = "CleanScreenGuard.App.1") -> None:
    """Give Windows 11 a dedicated AppUserModelID for proper taskbar icon & grouping."""
    try:
        shell32 = ctypes.windll.shell32
        shell32.SetCurrentProcessExplicitAppUserModelID(app_id)
    except Exception:
        pass


def exclude_window_from_capture(hwnd: int) -> None:
    """Exclude the overlay from DWM screen captures to prevent feedback loops."""
    success = user32.SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)
    if not success:
        err = ctypes.get_last_error()
        raise OSError(
            f"SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE) failed (Win32 error {err}). "
            "Requires Windows 10 2004+ / Windows 11."
        )


def make_window_click_through(hwnd: int) -> None:
    """Ensure mouse clicks pass through and the overlay never steals focus."""
    ex_style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
    ex_style |= WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
    user32.SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style)


def _get_startup_command() -> str:
    if getattr(sys, "frozen", False):
        return f'"{os.path.abspath(sys.executable)}"'
    pythonw = os.path.join(os.path.dirname(sys.executable), "pythonw.exe")
    exe = pythonw if os.path.isfile(pythonw) else sys.executable
    app_script = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "app.py"))
    return f'"{exe}" "{app_script}"'


def is_startup_enabled() -> bool:
    if winreg is None:
        return False
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_REG_PATH, 0, winreg.KEY_READ) as key:
            val, _ = winreg.QueryValueEx(key, RUN_VALUE_NAME)
            return bool(val)
    except FileNotFoundError:
        return False
    except Exception:
        return False


def set_startup_enabled(enabled: bool) -> bool:
    """Add or remove CleanScreen Guard from Windows HKCU Run registry key."""
    if winreg is None:
        return False
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_REG_PATH, 0, winreg.KEY_SET_VALUE) as key:
            if enabled:
                winreg.SetValueEx(key, RUN_VALUE_NAME, 0, winreg.REG_SZ, _get_startup_command())
            else:
                try:
                    winreg.DeleteValue(key, RUN_VALUE_NAME)
                except FileNotFoundError:
                    pass
        return True
    except Exception as exc:
        print(f"[Warning] Failed to update Windows Startup setting: {exc}")
        return False
