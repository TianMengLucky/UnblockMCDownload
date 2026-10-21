$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$repository = 'https://github.com/TianMengLucky/UnblockMCDownload'
function Get-Release($prefix) {
    $release = Invoke-RestMethod -Uri ($prefix + 'https://api.github.com/repos/TianMengLucky/UnblockMCDownload/releases/latest') -Headers @{'User-Agent'='UnblockMCDownload'} -TimeoutSec 20
    if ($release.tag_name -notmatch '^v?(\d+\.\d+\.\d+)$' -or $release.draft -or $release.prerelease) { throw 'Invalid release version' }
    return $Matches[1]
}
try {
    if ($env:UBMC_UPDATE_MODE -eq 'check') {
        try { $version = Get-Release 'https://gh-proxy.com/' }
        catch { $version = Get-Release '' }
        Write-Output $version
    } else {
        $version = $env:UBMC_UPDATE_VERSION
        if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid release version' }
        $destination = $env:UBMC_UPDATE_DESTINATION
        $temporary = $destination + '.' + [Guid]::NewGuid().ToString('N') + '.tmp'
        try {
            foreach ($prefix in @('https://gh-proxy.com/', '')) {
                try {
                    $url = $prefix + $repository + '/releases/download/v' + $version
                    $checksum = (Invoke-WebRequest -UseBasicParsing -Uri ($url + '/MC.exe.sha256') -TimeoutSec 20).Content
                    if ($checksum -notmatch '\A([a-fA-F0-9]{64})\s+\*?解锁MC下载限制\.exe\s*\z') { throw 'Invalid checksum' }
                    $expected = $Matches[1]
                    Invoke-WebRequest -UseBasicParsing -Uri ($url + '/MC.exe') -OutFile $temporary -TimeoutSec 120
                    $length = (Get-Item -LiteralPath $temporary).Length
                    if ($length -lt 2 -or $length -gt 268435456 -or (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash -ne $expected) { throw 'Release verification failed' }
                    $stream = [IO.File]::OpenRead($temporary)
                    try { if ($stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) { throw 'Invalid executable' } } finally { $stream.Dispose() }
                    [IO.File]::Move($temporary, $destination)
                    Write-Output $destination
                    break
                } catch { if ($prefix -eq '') { throw } }
            }
        } finally { if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) } }
    }
} catch { [Console]::Error.WriteLine($_.ToString()); exit 1 }
