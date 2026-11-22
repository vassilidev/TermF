# Installs TermF from its latest GitHub release, checked against the release's SHA256SUMS.
#
#   irm https://vassilidev.github.io/TermF/install.ps1 | iex
#
# Set TERMF_BETA=1 to take the pre-releases too, TERMF_DRY_RUN=1 to only say what would be installed.
# The installer is the per-user one (no administrator rights) and runs silently. Downloaded by PowerShell, it
# carries no "mark of the web", so SmartScreen does not stop it although it is not signed yet.
& {
  $ErrorActionPreference = 'Stop'
  $ProgressPreference = 'SilentlyContinue'
  [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

  $repo = 'vassilidev/TermF'
  $api = if ($env:TERMF_API) { $env:TERMF_API } else { 'https://api.github.com' }
  $arch = if ($env:TERMF_ARCH) { $env:TERMF_ARCH } else { [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString() }
  $suffix = switch ($arch) {
    { $_ -in 'X64', 'AMD64' } { '_x64-setup.exe' }
    { $_ -in 'Arm64', 'ARM64' } { '_arm64-setup.exe' }
    default { throw "TermF install: unsupported processor: $arch" }
  }

  function Get-Json($path) {
    (Invoke-WebRequest -UseBasicParsing -Headers @{ Accept = 'application/vnd.github+json' } -Uri "$api/repos/$repo/$path").Content | ConvertFrom-Json
  }

  # The latest release is the newest that is not a pre-release; until there is one, the newest pre-release.
  $release = $null
  if ($env:TERMF_BETA -ne '1') {
    try { $release = Get-Json 'releases/latest' } catch { $release = $null }
  }
  if (-not $release) { $release = @(Get-Json 'releases?per_page=1')[0] }
  if (-not $release) { throw 'TermF install: no TermF release found' }

  $tag = $release.tag_name
  $asset = $release.assets | Where-Object { $_.name.EndsWith($suffix) } | Select-Object -First 1
  if (-not $asset) { throw "TermF install: TermF $tag has no file ending in $suffix" }
  $sums = $release.assets | Where-Object { $_.name -eq 'SHA256SUMS' } | Select-Object -First 1
  if (-not $sums) { throw "TermF install: TermF $tag publishes no SHA256SUMS: nothing to check the download against, so nothing is installed" }

  if ($env:TERMF_DRY_RUN -eq '1') {
    Write-Host "TermF $tag"
    Write-Host "  download $($asset.browser_download_url)"
    Write-Host "  check it against $($sums.browser_download_url)"
    return
  }

  $dir = Join-Path ([IO.Path]::GetTempPath()) ("termf-" + [Guid]::NewGuid())
  New-Item -ItemType Directory -Path $dir | Out-Null
  try {
    $file = Join-Path $dir $asset.name
    Write-Host "Downloading TermF $tag ($($asset.name))..."
    Invoke-WebRequest -UseBasicParsing -Uri $asset.browser_download_url -OutFile $file
    $list = (Invoke-WebRequest -UseBasicParsing -Uri $sums.browser_download_url).Content
    if ($list -is [byte[]]) { $list = [Text.Encoding]::UTF8.GetString($list) }
    $expected = $list -split "`n" | ForEach-Object {
      $f = $_.Trim() -split '\s+', 2
      if ($f.Count -eq 2 -and $f[1].TrimStart('*') -eq $asset.name) { $f[0] }
    } | Select-Object -First 1
    if (-not $expected) { throw "TermF install: $($asset.name) is not listed in SHA256SUMS" }
    if ((Get-FileHash -Algorithm SHA256 $file).Hash -ne $expected.ToUpper()) {
      throw "TermF install: $($asset.name) does not match its SHA-256 sum: not installed"
    }
    Start-Process -FilePath $file -ArgumentList '/S' -Wait
    Write-Host "TermF $tag is installed: find it in the Start menu."
  } finally {
    Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
  }
}
