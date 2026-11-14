# Measure Gridshot's CPU, memory and GPU use while it plays a round by itself.
#   powershell -ExecutionPolicy Bypass -File tools\measure.ps1 [-Exe path\to\gridshot.exe] [-Seconds 20]
# The game runs in its built-in bench mode (GRIDSHOT_DEMO=bench): it flicks to targets
# and shoots like a player, at your monitor's refresh rate (vsync on).
param(
    [string]$Exe = "dist\gridshot.exe",
    [int]$Seconds = 20
)
$ErrorActionPreference = "Stop"
$Exe = (Resolve-Path $Exe).ProviderPath
$cores = [Environment]::ProcessorCount
$env:GRIDSHOT_DEMO = "bench:$($Seconds + 6)"
$p = Start-Process -FilePath $Exe -PassThru
Remove-Item Env:GRIDSHOT_DEMO
Start-Sleep -Seconds 4   # let it warm up

$cpu = @(); $ws = @(); $priv = @(); $gpu = @()
$lastCpu = $p.TotalProcessorTime.TotalSeconds; $lastT = Get-Date
for ($i = 0; $i -lt $Seconds; $i++) {
    Start-Sleep -Seconds 1
    $p.Refresh()
    $now = Get-Date
    $c = $p.TotalProcessorTime.TotalSeconds
    $cpu += ($c - $lastCpu) / ($now - $lastT).TotalSeconds / $cores * 100
    $lastCpu = $c; $lastT = $now
    $ws += $p.WorkingSet64 / 1MB
    $priv += $p.PrivateMemorySize64 / 1MB
    try {
        $g = (Get-Counter "\GPU Engine(pid_$($p.Id)_*)\Utilization Percentage" -ErrorAction Stop).CounterSamples |
            Measure-Object CookedValue -Sum
        $gpu += $g.Sum
    } catch {}
}
if (-not $p.HasExited) { Stop-Process -Id $p.Id }

function Stat($xs) { $m = $xs | Measure-Object -Average -Maximum; "{0:N1} avg / {1:N1} peak" -f $m.Average, $m.Maximum }
$exeMb = (Get-Item $Exe).Length / 1MB
$cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
$gpuCtl = Get-CimInstance Win32_VideoController | Where-Object { $_.CurrentRefreshRate -gt 0 } | Select-Object -First 1

"Exe size          : {0:N2} MB" -f $exeMb
"CPU (% of all $cores threads): $(Stat $cpu)"
"RAM working set MB: $(Stat $ws)"
"RAM private MB    : $(Stat $priv)"
if ($gpu.Count) { "GPU 3D/copy %     : $(Stat $gpu)" }
"Machine           : $cpuName, $($gpuCtl.Name), $($gpuCtl.CurrentRefreshRate) Hz"
