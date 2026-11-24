# CleanScreen Guard v2.0 (Native Rust Edition)

**CleanScreen Guard** (حارس الشاشة النظيفة) is a free, 100% local, GPU-accelerated Windows 11 desktop app written in **native Rust** (~18–24 MB installer, ~40–60 MB RAM, zero Python/Qt/OpenCV dependencies) that detects and blurs NSFW and suggestive content in real time across **games, videos, and browsers**.

## Local Build Steps (Windows 11 + Rust + Visual Studio C++ Build Tools)
1. Run `.\scripts\fetch_runtime.ps1` once in PowerShell to download `onnxruntime.dll` and `DirectML.dll` into `runtime\`.
2. Copy `320n_fp16.onnx` (or `320n.onnx`) into `runtime\` (or `models\`).
3. Run:
   ```powershell
   cargo build --release
   ```
4. Copy `runtime\onnxruntime.dll` and `runtime\DirectML.dll` next to `target\release\CleanScreenGuard.exe` (or compile `installer.iss` with Inno Setup to generate `dist\CleanScreenGuard_Setup_2.0.0.exe`).

## Cloud Build (Zero Upload Bandwidth)
Push this folder to GitHub (e.g., on a `v2-rust` branch) and run **Actions → Build & Release CleanScreen Guard (Rust v2) → Run workflow**. GitHub's Windows runners automatically download DirectML, generate `320n_fp16.onnx`, compile `CleanScreenGuard.exe`, and publish `CleanScreenGuard_Setup_2.0.0.exe`.

## License
Open-source under **AGPL-3.0**.
