# CleanScreen Guard (حارس الشاشة النظيفة)

**CleanScreen Guard** is a free, 100% local, GPU-accelerated Windows 11 desktop app written in **native Rust** that detects and blurs NSFW and suggestive content in real time across **games, videos, and browsers**.

Everything runs completely offline on your machine—zero cloud uploads, zero telemetry, and no Python, Qt, or OpenCV runtime bloat[cite: 2].

---

## Features

- **Native Rust Engine (`v2.0`):** Low RAM usage (~40–60 MB) with native Windows DXGI Desktop Duplication capture and per-pixel-alpha click-through overlays (`WDA_EXCLUDEFROMCAPTURE` prevents blur feedback loops)[cite: 2, 8, 10].
- **Built-in NudeNet `640m` (Default) + `320n`:** High-accuracy `640m` (FP16) runs by default on GPU, with `320n` included for ultra-low GPU or CPU-only fallback[cite: 22]. Drop any compatible YOLO/NudeNet `.onnx` model into the `models/` folder to add custom models[cite: 5, 22].
- **DirectML GPU Acceleration:** Supports **Power Saving GPU** (Integrated GPU — best for laptops and gaming), **Auto**, **High Performance GPU** (Dedicated RTX/RX), and **CPU Only** (0% GPU usage)[cite: 14, 22].
- **Smooth Motion Tracking & True Idle Sleep:** Decoupled async AI thread paired with pure-Rust block-matching motion tracking (up to 30 FPS), Adaptive Scan Rate, and automatic idle sleep on still screens[cite: 14, 21, 23].
- **Bilingual Settings UI (English & العربية RTL):** Built on Windows 11 Edge WebView2 with dark mode, customizable blur strength, sensitivity, anti-flicker hold time, padding, and per-category NudeNet toggles[cite: 14, 21].
- **System Tray Controls:** Left-click the tray icon for Settings; right-click to Pause/Resume Protection or Quit[cite: 21].

---

## Download & Installation

1. Go to the [**Releases**](https://github.com/AmgedAlfakih/CleanScreen-Guard/releases) page and download `CleanScreenGuard_Setup_2.0.0.exe`.
2. Run the installer (no administrator rights required).
3. Launch **CleanScreen Guard**—it runs quietly in the Windows system tray.

> **Security & Trust:** All official installers are **cloud-built directly from this public repository** via GitHub Actions, with SHA-256 checksums published in each release's notes.

---

## Building from Source (Windows 11)

**Prerequisites:** Rust stable toolchain (`rustup`), Visual Studio C++ Build Tools + Windows 11 SDK, and optionally Inno Setup 6/7 for packaging.

1. Download the official Microsoft ONNX Runtime DirectML DLLs into `runtime/`:
   ```powershell
   powershell -ExecutionPolicy Bypass -File .\scripts\fetch_runtime.ps1