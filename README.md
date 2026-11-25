# CleanScreen Guard (حارس الشاشة النظيفة)

**CleanScreen Guard** is a free, 100% local, GPU-accelerated Windows 11 desktop app written in **native Rust** that detects and blurs NSFW and suggestive content in real time across **games, videos, and browsers**.

Everything runs completely offline on your machine—zero cloud uploads, zero telemetry, and no Python, Qt, or OpenCV runtime bloat.

---

## Features

- **Native Rust Engine (`v2.1`):** Low RAM footprint, zero garbage-collection pauses, native Windows DXGI Desktop Duplication capture, and per-pixel-alpha click-through overlays (`WDA_EXCLUDEFROMCAPTURE` prevents blur feedback loops).
- **Two-Stage AI Cascade (`320n_fp16` → `640m_fp16`):** Lightweight `320n_fp16` scouts clean frames at ultra-low GPU cost, waking high-accuracy `640m_fp16` only when a candidate is detected or during periodic safety sweeps. Drop any compatible YOLO/NudeNet `.onnx` model into the `models/` folder to add custom models.
- **D3D11 GPU-Side Downscaling & DirectML `IoBinding`:** Hardware `GenerateMips` downscales frames directly on the GPU (cutting clean-screen PCIe readback by 4×–16×), skips cursor-only desktop updates, and reuses pre-bound DirectML tensors with zero VRAM leaks.
- **Flexible Hardware Acceleration:** Supports **Power Saving GPU** (Integrated GPU — keeps dedicated GPUs at 0% while gaming), **Auto**, **High Performance GPU** (Dedicated RTX/RX), and **CPU Only**.
- **Smooth Motion Tracking & True Idle Sleep:** Decoupled async AI thread paired with pure-Rust block-matching motion tracking (up to 30 FPS), Adaptive Scan Rate, zero-allocation frame recycling, and automatic idle sleep on still screens.
- **Bilingual Settings UI (English & العربية RTL):** Native Windows 11 Edge WebView2 dark-mode UI with live sliders for blur strength, sensitivity, anti-flicker hold time, padding, scan rate, and per-category NudeNet toggles.
- **System Tray Controls:** Left-click the tray icon for Settings; right-click to Pause/Resume Protection or Quit.

---

## Real-World Performance Benchmarks (`v2.1.0`)

Measured on a **Windows 11 laptop with an NVIDIA RTX 3050M (60W, 4 GB VRAM)** using DirectML:

### Model Inference Speed (DirectML)

| Model | FP32 Baseline | Static FP16 (`v2.1`) | Speedup |
| :--- | :---: | :---: | :---: |
| **`320n` (Scout)** | `5.65 ms/frame` | **`2.52 ms/frame`** | **2.24× faster** |
| **`640m` (Verifier)** | `31.23 ms/frame` | **`13.24 ms/frame`** | **2.36× faster** |

### Live System Impact (2-Minute GPU-Z Log, Active Detection at `4 Hz` + `24 FPS` Tracking)

| Metric | Measured Result |
| :--- | :--- |
| **GPU Chip Power Draw** | **0.93 W avg** (`0.2–2.9 W`) |
| **Total GPU Board Power** | **6.31 W avg** (`3.5–10.8 W`, ~10.5% of 60W TDP) |
| **GPU Voltage / Clocks** | Locked at **`0.631 V`** idle floor (`210–645 MHz`, `PerfCap = Idle`) |
| **GPU Temperature** | **46.5 °C avg** (`45.3–47.5 °C`) — silent laptop fans |
| **VRAM Footprint** | **386 MB** (completely flat across entire run, zero leaks) |
| **PCIe Bus Load** | **0–1%** (`≤ 0.04 GB/s` TX, `≤ 0.23 GB/s` RX) |
| **Clock-Normalized GPU Load** | **~4.0% avg** under continuous active tracking (`< 1%` on clean screens) |

---

## Download & Installation

1. Go to the [**Releases**](https://github.com/AmgedAlfakih/CleanScreen-Guard/releases) page and download `CleanScreenGuard_Setup_2.1.0.exe`.
2. Run the installer (no administrator rights required).
3. Launch **CleanScreen Guard**—it runs quietly in the Windows system tray.

> **Security & Trust:** All official installers are **cloud-built directly from this public repository** via GitHub Actions, with SHA-256 checksums published in each release's notes.

---

## Building from Source (Windows 11)

**Prerequisites:** Rust stable toolchain (`rustup`), Visual Studio C++ Build Tools + Windows 11 SDK, and optionally Inno Setup 6/7 for packaging.

1. Download the official Microsoft ONNX Runtime DirectML DLLs into `runtime/`:
   ```powershell
   powershell -ExecutionPolicy Bypass -File .\scriptsetch_runtime.ps1
   ```
2. Place `320n.onnx`, `320n_fp16.onnx`, and `640m_fp16.onnx` inside `runtime/` (you can generate static FP16 models from FP32 `.onnx` files using `scripts/optimize_models.py`).
3. Build and launch in Release mode:
   ```powershell
   powershell -ExecutionPolicy Bypass -File .uild_and_run.ps1
   ```
4. *(Optional)* Compile the Windows installer with Inno Setup 6/7:
   ```powershell
   ISCC.exe /DMyAppVersion=2.1.0 installer.iss
   ```

---

## License

Open-source under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.
