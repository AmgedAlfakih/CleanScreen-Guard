import os
import sys

APP_FOLDER_NAME = "CleanScreenGuard"


def is_frozen() -> bool:
    """Return True when running inside a packaged PyInstaller .exe."""
    return bool(getattr(sys, "frozen", False))


def get_exe_dir() -> str:
    """Directory where the .exe (or project root in dev mode) resides."""
    if is_frozen():
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def get_bundle_dir() -> str:
    """Read-only internal directory where PyInstaller unpacks bundled data files."""
    if is_frozen() and hasattr(sys, "_MEIPASS"):
        return str(getattr(sys, "_MEIPASS"))
    return get_exe_dir()


def get_data_dir() -> str:
    r"""
    Writable user data directory for settings.json and custom models/.
    - When installed as a Windows .exe: uses %LOCALAPPDATA%\CleanScreenGuard so
      installing to Program Files never hits 'Access Denied' errors.
    - If a file named 'portable.txt' exists next to the .exe, stays portable next to the .exe.
    - When running from Python source: uses the project root folder.
    """
    exe_dir = get_exe_dir()
    if not is_frozen():
        return exe_dir

    if os.path.isfile(os.path.join(exe_dir, "portable.txt")):
        return exe_dir

    local_app_data = os.environ.get("LOCALAPPDATA") or os.path.expanduser("~")
    data_dir = os.path.join(local_app_data, APP_FOLDER_NAME)
    os.makedirs(data_dir, exist_ok=True)
    return data_dir


def get_models_dir() -> str:
    path = os.path.join(get_data_dir(), "models")
    os.makedirs(path, exist_ok=True)
    return path


def get_settings_path() -> str:
    return os.path.join(get_data_dir(), "settings.json")


def get_asset_path(filename: str) -> str:
    for base in (get_bundle_dir(), get_exe_dir()):
        candidate = os.path.join(base, "assets", filename)
        if os.path.isfile(candidate):
            return candidate
    return os.path.join(get_exe_dir(), "assets", filename)
