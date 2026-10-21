$ErrorActionPreference = 'Stop'
$tool = '__UNBLOCKMC_TOOL__'
$release = '__UNBLOCKMC_RELEASE__'
try {
    $marker = '#UBMCARGS:'
    $raw = [Environment]::CommandLine
    $position = $raw.IndexOf($marker, [StringComparison]::Ordinal)
    if ($position -lt 0) { throw 'Update arguments are missing.' }
    $arguments = $raw.Substring($position + $marker.Length)
    if (![IO.File]::Exists($tool)) {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $directory = [IO.Path]::GetDirectoryName($tool)
        [IO.Directory]::CreateDirectory($directory) | Out-Null
        $temporary = [IO.Path]::Combine($directory, '.unblockmc-' + [Guid]::NewGuid().ToString('N') + '.tmp')
        $client = New-Object Net.WebClient
        $client.Encoding = [Text.Encoding]::UTF8
        try {
            $sources = @($release)
            if ($release.StartsWith('https://github.com/')) { $sources = @(('https://gh-proxy.com/' + $release), $release) }
            foreach ($source in $sources) {
            try {
            $checksum = $client.DownloadString($source + '/MC.exe.sha256')
            if ($checksum -notmatch '\A([a-fA-F0-9]{64})\s+\*?解锁MC下载限制\.exe\s*\z') {
                throw 'The release checksum is invalid.'
            }
            $expected = $Matches[1]
            $client.DownloadFile($source + '/MC.exe', $temporary)
            $length = ([IO.FileInfo]::new($temporary)).Length
            if ($length -lt 2 -or $length -gt 268435456) { throw 'The release size is invalid.' }
            $stream = [IO.File]::OpenRead($temporary)
            $sha = [Security.Cryptography.SHA256]::Create()
            try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '') }
            finally { $stream.Dispose(); $sha.Dispose() }
            if ($actual -ne $expected) {
                throw 'The downloaded tool failed SHA-256 verification.'
            }
            [IO.File]::Move($temporary, $tool)
            break
            } catch { if ($source -eq $release) { throw } }
            }
        } finally {
            $client.Dispose()
            if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
        }
    }
    $info = New-Object Diagnostics.ProcessStartInfo
    $info.FileName = $tool
    $info.Arguments = $arguments
    $info.WorkingDirectory = '__UNBLOCKMC_DIRECTORY__'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $process = [Diagnostics.Process]::Start($info)
    $process.WaitForExit()
    if ($process.ExitCode -ne 0) { throw 'The update could not be patched; installation was stopped.' }
    exit 0
} catch {
    # PowerShell runs hidden in the launcher workflow. Keep a readable failure beside the tool.
    try { [IO.File]::AppendAllText($tool + '.update.log', $_.ToString() + [Environment]::NewLine) } catch { }
    exit 1
}
