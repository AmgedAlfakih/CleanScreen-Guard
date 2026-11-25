$ErrorActionPreference = "Stop"

Write-Host "1. Stopping any running CleanScreenGuard instance..." -ForegroundColor Cyan
Stop-Process -Name "CleanScreenGuard" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

Write-Host "2. Checking runtime DLLs & FP16-IO models..." -ForegroundColor Cyan
$required = @(
    "runtime\onnxruntime.dll",
    "runtime\DirectML.dll",
    "runtime\320n.onnx",
    "runtime\320n_fp16.onnx",
    "runtime\640m_fp16.onnx"
)
foreach ($f in $required) {
    if (-not (Test-Path $f)) {
        Write-Warning "Missing expected file: $f"
    }
}

Write-Host "3. Building CleanScreenGuard v2.1 (Release)..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Error "Cargo build failed!"
    exit 1
}

Write-Host "4. Copying runtime DLLs & True FP16-IO models into target\release\..." -ForegroundColor Cyan
Copy-Item "runtime\onnxruntime.dll", "runtime\DirectML.dll" "target\release\" -Force
New-Item -ItemType Directory -Force -Path "target\release\models" | Out-Null
Copy-Item "runtime\320n.onnx", "runtime\320n_fp16.onnx", "runtime\640m_fp16.onnx" "target\release\models\" -Force

Write-Host "5. Launching CleanScreenGuard v2.1..." -ForegroundColor Green
Start-Process -FilePath ".\target\release\CleanScreenGuard.exe"
Write-Host "Done! Left-click the tray icon and verify the status shows: Cascade 320n_fp16.onnx -> 640m_fp16.onnx (640px FP16-IO)" -ForegroundColor Green
