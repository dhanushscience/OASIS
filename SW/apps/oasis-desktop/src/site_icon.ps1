$ErrorActionPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.Drawing
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls13
# Sites often turn away a script's default identity, so the page is asked as a browser would.
$ua = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36'
function Get-Logo($x) {
  $c = @()
  $base = [Uri]"https://$x/"
  $r = Invoke-WebRequest -Uri $base -UseBasicParsing -TimeoutSec 4 -UserAgent $ua
  if ($r) {
    $page = $r.BaseResponse.ResponseUri; if (-not $page) { $page = $base }
    foreach ($m in [regex]::Matches([string]$r.Content, '<link[^>]+>')) {
      $t = $m.Value
      if ($t -notmatch 'rel=["'']?([^"''>]*)') { continue }
      $rel = $Matches[1].ToLower(); if ($rel -notmatch 'icon') { continue }
      if ($t -notmatch 'href=["'']?([^"'' >]+)') { continue }
      $href = $Matches[1]
      # Windows' image library cannot read these, and they would use up the few tries
      if ($href -match '\.(svg|webp)(\?|$)' -or $href -match '^data:') { continue }
      $size = 0; if ($t -match 'sizes=["'']?(\d+)x') { $size = [int]$Matches[1] }
      $score = $size; if ($rel -match 'apple-touch') { $score += 1000 }
      try { $c += [pscustomobject]@{ u = (New-Object Uri -ArgumentList $page, $href).AbsoluteUri; s = $score } } catch {}
    }
  }
  $c += [pscustomobject]@{ u = "https://$x/apple-touch-icon.png"; s = 500 }
  $c += [pscustomobject]@{ u = "https://$x/favicon.ico"; s = 1 }
  foreach ($k in ($c | Sort-Object s -Descending | Select-Object -First 5)) {
    $w = Invoke-WebRequest -Uri $k.u -UseBasicParsing -TimeoutSec 4 -UserAgent $ua
    if (-not $w -or $w.Content -isnot [byte[]] -or $w.Content.Length -lt 100) { continue }
    try {
      $s = New-Object IO.MemoryStream (, [byte[]]$w.Content)
      # an .ico holds several sizes: ask for the 64-pixel one rather than the first
      if ($w.Content[0] -eq 0 -and $w.Content[1] -eq 0 -and $w.Content[2] -eq 1) { $i = (New-Object Drawing.Icon -ArgumentList $s, 64, 64).ToBitmap() } else { $i = [Drawing.Image]::FromStream($s) }
      $b = New-Object Drawing.Bitmap 64, 64
      $g = [Drawing.Graphics]::FromImage($b); $g.Clear([Drawing.Color]::Transparent); $g.InterpolationMode = 'HighQualityBicubic'
      $g.DrawImage($i, 0, 0, 64, 64)
      $o = New-Object IO.MemoryStream; $b.Save($o, [Drawing.Imaging.ImageFormat]::Png)
      return [Convert]::ToBase64String($o.ToArray())
    } catch {}
  }
}
# the name as given, then with or without www, then the main domain (web.whatsapp.com -> whatsapp.com)
$h = $env:OASIS_SITE
$tries = @($h)
if ($h -like 'www.*') { $tries += $h.Substring(4) } else { $tries += "www.$h" }
$parts = $h.Split('.'); if ($parts.Count -gt 2) { $tries += ($parts[1..($parts.Count - 1)] -join '.') }
foreach ($x in $tries) { $png = Get-Logo $x; if ($png) { $png; exit 0 } }
exit 1
