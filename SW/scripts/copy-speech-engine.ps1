# Copies the two speech-engine files OASIS needs for push-to-talk into a `native` folder beside the
# program. They are the same files the C# OASIS app ships (sherpa-onnx 1.13.5); OASIS never
# downloads them. Run once after building:
#
#   .\scripts\copy-speech-engine.ps1 -From "C:\path\to\the\C#\OASIS\folder"
#
# -From is the folder that contains sherpa-onnx-c-api.dll and onnxruntime.dll (the C# build keeps
# them in runtimes\win-x64\native, or beside OASIS.exe).
param(
    [Parameter(Mandatory = $true)][string]$From,
    [string]$To = (Join-Path $PSScriptRoot "..\target\debug\native")
)

$files = "sherpa-onnx-c-api.dll", "onnxruntime.dll"
$source = @($From, (Join-Path $From "runtimes\win-x64\native")) |
    Where-Object { Test-Path (Join-Path $_ $files[0]) } | Select-Object -First 1
if (-not $source) { throw "Could not find $($files[0]) under $From." }

New-Item -ItemType Directory -Force $To | Out-Null
foreach ($f in $files) {
    Copy-Item (Join-Path $source $f) (Join-Path $To $f) -Force
    "copied $f"
}
"Done. Speech engine files are in $To"
