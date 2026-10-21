$ErrorActionPreference = 'Stop'
$script = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '../src/self-update.ps1')
$script = $script.Replace('[Console]::Error.WriteLine($_.ToString()); exit 1', 'throw')
$script:requests = @()
function Invoke-RestMethod($Uri, $Headers, $TimeoutSec) {
    $script:requests += $Uri
    if ($Uri.StartsWith('https://gh-proxy.com/')) { throw 'Proxy unavailable' }
    return @{tag_name='v0.0.3'; draft=$false; prerelease=$false}
}
$env:UBMC_UPDATE_MODE = 'check'
$result = & ([scriptblock]::Create($script))
if ($result -ne '0.0.3' -or $script:requests.Count -ne 2 -or !$script:requests[1].StartsWith('https://api.github.com/')) { throw 'Check fallback failed' }
$folder = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($folder) | Out-Null
$env:UBMC_UPDATE_MODE = 'download'
$env:UBMC_UPDATE_VERSION = '0.0.3'
$env:UBMC_UPDATE_DESTINATION = Join-Path $folder 'new.exe'
$payload = [byte[]](77,90,1,2)
$sha = [Security.Cryptography.SHA256]::Create()
$script:hash = [BitConverter]::ToString($sha.ComputeHash($payload)).Replace('-', '')
$sha.Dispose()
$script:requests = @()
function Invoke-WebRequest($Uri, [switch]$UseBasicParsing, $TimeoutSec, $OutFile) {
    $script:requests += $Uri
    if ($Uri.EndsWith('.sha256')) { return @{Content=($script:hash + ' *解锁MC下载限制.exe')} }
    if ($Uri.StartsWith('https://gh-proxy.com/')) { [IO.File]::WriteAllBytes($OutFile, [byte[]](77,90,9)) }
    else { [IO.File]::WriteAllBytes($OutFile, [byte[]](77,90,1,2)) }
}
try {
    $result = & ([scriptblock]::Create($script))
    if ($result -ne $env:UBMC_UPDATE_DESTINATION -or $script:requests.Count -ne 4) { throw 'Verification fallback failed' }
    if ((Get-FileHash -LiteralPath $result).Hash -ne $script:hash) { throw 'Wrong download' }
    [IO.File]::Delete($result)
    $script:hash = '0' * 64
    $failed = $false
    try { & ([scriptblock]::Create($script)) } catch { $failed = $true }
    if (!$failed -or [IO.File]::Exists($env:UBMC_UPDATE_DESTINATION) -or (Get-ChildItem -LiteralPath $folder).Count) { throw 'Corrupt download was retained' }
} finally { [IO.Directory]::Delete($folder, $false) }
Write-Output 'Self-update checks passed'
$replacement = Get-Content -Raw -Encoding UTF8 (Join-Path $PSScriptRoot '../src/replace-update.ps1')
$folder = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($folder) | Out-Null
$env:UBMC_UPDATE_TARGET = Join-Path $folder 'tool.exe'
$env:UBMC_UPDATE_STAGED = Join-Path $folder 'staged.exe'
$env:UBMC_UPDATE_PID = '2147483647'
$script:started = $false
function Start-Process($FilePath, $WorkingDirectory) {
    if ([IO.File]::ReadAllText($FilePath) -ne 'new') { throw 'Wrong executable restarted' }
    $script:started = $true
}
try {
    [IO.File]::WriteAllText($env:UBMC_UPDATE_TARGET, 'old')
    [IO.File]::WriteAllText($env:UBMC_UPDATE_STAGED, 'new')
    & ([scriptblock]::Create($replacement))
    if (!$script:started -or [IO.File]::Exists($env:UBMC_UPDATE_STAGED) -or [IO.File]::Exists($env:UBMC_UPDATE_TARGET + '.update-backup')) { throw 'Replacement or restart failed' }
    [IO.File]::Delete($env:UBMC_UPDATE_TARGET)
} finally { [IO.Directory]::Delete($folder, $false) }
Write-Output 'Replacement and restart checks passed'
