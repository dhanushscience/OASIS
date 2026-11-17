# Publish the voice model files (docs/VOICE_MODEL_SETUP.md section 4): copy each under its release name,
# hash it, write the manifest the app is built with (and models.json, the same list), and (with -Publish) upload them to a models
# release in the public releases repo. That release is a pre-release, so it can never be "Latest"
# (--latest=false alone is ignored while it is the newest release): the app updater reads
# releases/latest (docs/UPDATES.md).
#
#   scripts\publish-models.ps1 -LlamaZip <llama-b12304-bin-win-vulkan-x64.zip> -LlamaLicense <LICENSE>           # prepare only
#   scripts\publish-models.ps1 -LlamaZip ... -LlamaLicense ... -Publish                                           # and upload
#
# Sources are the models as installed under %LOCALAPPDATA%\ProjectOASIS\Models. The manifest goes to
# crates\oasis-core\src\voice\model_files.rs; commit it with the app version that should use these files.
param(
    [Parameter(Mandatory = $true)][string]$LlamaZip,
    [Parameter(Mandatory = $true)][string]$LlamaLicense,
    [string]$Tag = "models-2026.11",
    [string]$Repo = "dhanushscience/OASIS-releases",
    [string]$Stage = "$env:TEMP\oasis-models-stage",
    [switch]$Publish
)
$ErrorActionPreference = "Stop"
$models = "$env:LOCALAPPDATA\ProjectOASIS\Models"
$v2 = "$models\stt\parakeet-tdt-0.6b-v2"
$ctc = "$models\stt\sherpa-onnx-nemo-parakeet_tdt_ctc_110m-en-36000-int8"
$s1 = "$models\llm\s1-mini"
$notice = "$PSScriptRoot\models\parakeet.NOTICE.txt"

# Source, release asset name, path under Models (forward slashes), group, unzip into that folder.
$files = @(
    @("$v2\encoder.int8.onnx", "parakeet-tdt-0.6b-v2.encoder.int8.onnx", "stt/parakeet-tdt-0.6b-v2/encoder.int8.onnx", "SpeechBalanced", $false),
    @("$v2\decoder.int8.onnx", "parakeet-tdt-0.6b-v2.decoder.int8.onnx", "stt/parakeet-tdt-0.6b-v2/decoder.int8.onnx", "SpeechBalanced", $false),
    @("$v2\joiner.int8.onnx", "parakeet-tdt-0.6b-v2.joiner.int8.onnx", "stt/parakeet-tdt-0.6b-v2/joiner.int8.onnx", "SpeechBalanced", $false),
    @("$v2\tokens.txt", "parakeet-tdt-0.6b-v2.tokens.txt", "stt/parakeet-tdt-0.6b-v2/tokens.txt", "SpeechBalanced", $false),
    @($notice, "parakeet-tdt-0.6b-v2.NOTICE.txt", "stt/parakeet-tdt-0.6b-v2/NOTICE.txt", "SpeechBalanced", $false),
    @("$ctc\model.int8.onnx", "parakeet-tdt-ctc-110m.model.int8.onnx", "stt/parakeet-tdt-ctc-110m/model.int8.onnx", "SpeechLight", $false),
    @("$ctc\tokens.txt", "parakeet-tdt-ctc-110m.tokens.txt", "stt/parakeet-tdt-ctc-110m/tokens.txt", "SpeechLight", $false),
    @($notice, "parakeet-tdt-ctc-110m.NOTICE.txt", "stt/parakeet-tdt-ctc-110m/NOTICE.txt", "SpeechLight", $false),
    # The OASIS cleanup fine-tune of S1-mini (finetune/ is private, not in git). It must stay named S1-mini (LICENSE, additional term).
    @("$PSScriptRoot\..\..\finetune\oasis_cleanup\gguf\oasis-cleanup-padded.gguf", "s1-mini-oasis.q4_k_m.gguf", "llm/s1-mini/s1-mini-q4_k_m.gguf", "Polish", $false),
    @("$s1\LICENSE", "s1-mini.LICENSE", "llm/s1-mini/LICENSE", "Polish", $false),
    @("$PSScriptRoot\models\s1-mini-oasis.NOTICE", "s1-mini-oasis.NOTICE", "llm/s1-mini/NOTICE", "Polish", $false),
    @($LlamaZip, "llama-vulkan-b12304.zip", "runtime/llama-vulkan", "Polish", $true),
    @($LlamaLicense, "llama.cpp.LICENSE.txt", "runtime/llama-vulkan/LICENSE-llama.cpp.txt", "Polish", $false)
)

New-Item -ItemType Directory -Force $Stage | Out-Null
$rows = @()
$entries = @()
foreach ($f in $files) {
    $source, $asset, $path, $group, $unzip = $f
    if (-not (Test-Path $source)) { throw "Missing: $source" }
    $staged = Join-Path $Stage $asset
    Copy-Item $source $staged -Force
    $size = (Get-Item $staged).Length
    $sha = (Get-FileHash $staged -Algorithm SHA256).Hash.ToLower()
    Write-Host ("{0,-42} {1,12:N0} bytes  {2}" -f $asset, $size, $sha)
    $rows += "    ModelFile { asset: `"$asset`", path: `"$path`", bytes: $size, sha256: `"$sha`", group: Group::$group, unzip: $(if ($unzip) { 'true' } else { 'false' }) },"
    $entries += [ordered]@{ asset = $asset; path = $path; bytes = $size; sha256 = $sha; group = $group; unzip = [bool]$unzip }
}

$base = "https://github.com/$Repo/releases/download/$Tag/"
$rust = @"
// Written by scripts/publish-models.ps1; do not edit by hand. The files of release $Tag in $Repo,
// with the SHA-256 each download must match (docs/VOICE_MODEL_SETUP.md section 4.3).

/// The release the files below come from.
pub const RELEASE: &str = "$Tag";
/// Where they are downloaded from: this, then the asset name.
pub const BASE_URL: &str = "$base";

/// Every file, in the order they are downloaded.
pub const FILES: &[ModelFile] = &[
$($rows -join "`n")
];
"@
$out = Join-Path $PSScriptRoot "..\crates\oasis-core\src\voice\model_files.rs"
[IO.File]::WriteAllText((Resolve-Path (Split-Path $out)).Path + "\model_files.rs", $rust.Replace("`r`n", "`n") + "`n")
Write-Host "Manifest written: $out"
# The same list as models.json in the release, so the set's version and hashes can be read without the app.
$modelsJson = Join-Path $Stage "models.json"
$set = [ordered]@{ release = $Tag; base_url = $base; files = $entries }
# no BOM, like latest.json
[IO.File]::WriteAllText($modelsJson, ($set | ConvertTo-Json -Depth 4))
Write-Host "Models manifest: $modelsJson"

if (-not $Publish) {
    Write-Host "Prepared in $Stage. Run again with -Publish to upload to $Repo ($Tag)."
    return
}
# Windows PowerShell turns a native program's stderr into an error under "Stop"; gh is judged by its exit code.
$ErrorActionPreference = "Continue"
gh release view $Tag -R $Repo 2>$null | Out-Null
if ($LASTEXITCODE -ne 0) {
    gh release create $Tag -R $Repo --prerelease --latest=false --title "Voice models ($Tag)" `
        --notes "Voice model files that OASIS downloads on first start. Not an app release: the app checks each file's SHA-256 before using it. Licences and credits are beside each model (NOTICE, LICENSE files)."
    if ($LASTEXITCODE -ne 0) { throw "Could not create release $Tag" }
}
$assets = @($files | ForEach-Object { Join-Path $Stage $_[1] }) + $modelsJson
gh release upload $Tag -R $Repo --clobber @assets
if ($LASTEXITCODE -ne 0) { throw "Upload failed" }
Write-Host "Published $($files.Count) files to $Repo ($Tag)."
