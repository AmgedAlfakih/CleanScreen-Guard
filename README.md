# CleanScreen Guard — Real-Time NSFW Blur for Windows 11

**CleanScreen Guard** (حارس الشاشة النظيفة) is a free, 100% local, GPU-accelerated Windows 11 desktop app that automatically detects and blurs NSFW and suggestive content in real time across **games, browsers, videos, and every connected monitor**—without modifying any game files.

---

## Security & Windows SmartScreen Notice
> **100% Offline, Open-Source, and No Malware:** CleanScreen Guard makes **zero network connections** and never uploads or saves screenshots. Everything runs locally on your PC's GPU.
>
> Because this is a free, indie open-source app and is not yet signed with a paid Microsoft code-signing certificate, Windows Defender SmartScreen may display *"Windows protected your PC"* when running the installer. Click **More info → Run anyway** to install—or inspect the full source code in `src/` and build `CleanScreenGuard.exe` yourself using `build_exe.bat`.
> *(Link will be provided later when my internet is stable)*

---

## Features
- **Works Everywhere (Games, Videos & Browsers):** Universal click-through overlay using Windows DXGI capture (`WDA_EXCLUDEFROMCAPTURE`)—no game file modifications and no anti-cheat tampering. *(Use **Borderless Windowed** mode in games.)*
- **GPU-Accelerated (DirectML):** Runs smoothly on NVIDIA, AMD, and Intel GPUs with ~10% CPU usage.
- **Anti-Flicker Tracking & Live Settings:** Tune sensitivity, anti-flicker hold time, blur strength, scan FPS, and individual categories (nudity, bikinis/swimwear, exposed belly) live from the system tray.
- **Custom & Dual-Model Support:** Uses NudeNet `320n` out of the box, with a built-in Model Selector to load `640m.onnx` (higher 3D/realistic accuracy) or anime YOLO `.onnx` models.
- **English & Arabic (العربية):** Instant Right-to-Left (RTL) interface switching inside Settings.
- **Native Windows 11 App:** Standard Setup wizard, Start Menu & Installed Apps integration, Single-Instance lock, and optional **Start with Windows**.

---

## Installation (For Users — No Python Needed)
1. Download **`CleanScreenGuard_Setup_1.0.0.exe`** from **Releases**.
2. Run the installer (**Next → Next → Install**).
3. Click the **CleanScreen Guard icon** in the Windows system tray (bottom-right corner) to open **Settings** or switch the language to **العربية (Arabic)**.

### التثبيت السريع (للمستخدمين)
1. حمّل ملف **`CleanScreenGuard_Setup_1.0.0.exe`** من قسم **Releases**.
2. شغّل المثبّت واضغط **Next ← Install** *(إذا ظهرت رسالة Windows SmartScreen لأن البرنامج مفتوح المصدر وغير موقّع رقمياً، اضغط More info ثم Run anyway)*.
3. اضغط على أيقونة **CleanScreen Guard** في شريط المهام أسفل الشاشة لفتح الإعدادات وتحويل الواجهة إلى **العربية**.

---

## Building from Source (For Developers)
1. Install **Python 3.10–3.12** (check **Add python.exe to PATH**).
2. Double-click **`setup_and_run.bat`** on first run.
3. Double-click **`build_exe.bat`** to generate `dist\CleanScreenGuard\CleanScreenGuard.exe`.
4. Install **Inno Setup 7** (`https://jrsoftware.org/inno-setup/downloads`) and double-click **`build_installer.bat`** to compile **`dist\CleanScreenGuard_Setup_1.0.0.exe`**.

---

## Credits & License
Open-source under **AGPL-3.0**. Built with [NudeNet](https://github.com/notAI-tech/NudeNet) (AGPL-3.0), [PySide6](https://doc.qt.io/qtforpython-6/) (LGPL-3.0), [ONNX Runtime DirectML](https://onnxruntime.ai/), and [DXCam](https://github.com/ra1nty/DXcam).
