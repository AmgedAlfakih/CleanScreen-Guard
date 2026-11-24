# Downloads official Microsoft ONNX Runtime DirectML 1.24.4 DLLs into runtime/
# Run once before local `cargo run --release` or `installer.iss`
$ErrorActionPreference = "Stop"

$ortVer = "1.24.4"
$dmlVer = "1.15.4"
New-Item -ItemType Directory -Force -Path "runtime" | Out-Null

Write-Host "[1/2] Downloading Microsoft.ML.OnnxRuntime.DirectML $ortVer (NuGet)..."
$ortZip = "runtime\ort_dml.zip"
Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/Microsoft.ML.OnnxRuntime.DirectML/$ortVer" -OutFile $ortZip
Expand-Archive -Path $ortZip -DestinationPath "runtime\ort_tmp" -Force
Copy-Item "runtime\ort_tmp\runtimes\win-x64\native\onnxruntime.dll" "runtime\onnxruntime.dll" -Force

Write-Host "[2/2] Downloading Microsoft.AI.DirectML $dmlVer (NuGet)..."
$dmlZip = "runtime\dml.zip"
Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/Microsoft.AI.DirectML/$dmlVer" -OutFile $dmlZip
Expand-Archive -Path $dmlZip -DestinationPath "runtime\dml_tmp" -Force
Copy-Item "runtime\dml_tmp\bin\x64-win\DirectML.dll" "runtime\DirectML.dll" -Force

Remove-Item -Recurse -Force "runtime\ort_tmp", "runtime\dml_tmp", $ortZip, $dmlZip
Write-Host "Done! runtime\onnxruntime.dll and runtime\DirectML.dll are ready."
Write-Host "Tip: Also place 320n_fp16.onnx (or 320n.onnx) into runtime\ or models\."
