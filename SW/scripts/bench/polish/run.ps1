# One polish bench run: start llama-server (local only, 127.0.0.1), wait for it, run the cases, stop it.
# Needs the models in Models\llm and llama.cpp unpacked in bench\llama-cpu and bench\llama-vulkan
# under %LOCALAPPDATA%\ProjectOASIS (see docs/VOICE_BENCH.md). Server logs go beside this script.
#   run.ps1 <s1|speako> <cpu|vulkan> <label> [affinity hex, e.g. 5] [threads] [gpu layers]
param([string]$Model, [string]$Backend, [string]$Label, [string]$Affinity = "", [int]$Threads = 4, [int]$Gpu = 0)
$ErrorActionPreference = "Stop"
$root = "$env:LOCALAPPDATA\ProjectOASIS"
$gguf = @{ s1 = "$root\Models\llm\s1-mini\s1-mini-q4_k_m.gguf"; speako = "$root\Models\llm\speakoflow-mini\SpeakoFlow-Mini-0.8B-Q4_K_M.gguf" }[$Model]
$server = "$root\bench\llama-$Backend\llama-server.exe"
$port = 8091
$serverArgs = @("-m", "`"$gguf`"", "--host", "127.0.0.1", "--port", $port, "-c", "4096", "-t", $Threads, "-ngl", $Gpu, "--jinja", "-np", "1", "--no-webui")
$p = Start-Process -FilePath $server -ArgumentList $serverArgs -PassThru -WindowStyle Hidden -RedirectStandardError "$PSScriptRoot\server-$Model-$Label.log"
try {
    if ($Affinity) { $p.ProcessorAffinity = [IntPtr][Convert]::ToInt64($Affinity, 16) }
    $deadline = (Get-Date).AddSeconds(90)
    do {
        Start-Sleep -Milliseconds 500
        try { $ok = (Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$port/health" -TimeoutSec 2).StatusCode -eq 200 } catch { $ok = $false }
    } until ($ok -or (Get-Date) -gt $deadline -or $p.HasExited)
    if (-not $ok) { throw "llama-server did not start; see server-$Model-$Label.log" }
    $ErrorActionPreference = "Continue"
    & cmd /c "python -I `"$PSScriptRoot\bench_polish.py`" $Model $Label 5 $port 2>&1"
} finally {
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
}
