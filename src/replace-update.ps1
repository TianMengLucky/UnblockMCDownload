$ErrorActionPreference = 'Stop'
$target = $env:UBMC_UPDATE_TARGET
$staged = $env:UBMC_UPDATE_STAGED
$backup = $target + '.update-backup'
try {
    $process = Get-Process -Id ([int]$env:UBMC_UPDATE_PID) -ErrorAction SilentlyContinue
    if ($process -and !$process.WaitForExit(60000)) { throw 'Application did not exit' }
    $replaced = $false
    for ($attempt = 0; $attempt -lt 20; $attempt++) {
        try {
            [IO.File]::Replace($staged, $target, $backup)
            $replaced = $true
            break
        } catch { Start-Sleep -Milliseconds 500 }
    }
    if (!$replaced) { throw 'Could not replace executable' }
    try { Start-Process -FilePath $target -WorkingDirectory ([IO.Path]::GetDirectoryName($target)) }
    catch {
        [IO.File]::Replace($backup, $target, $null)
        throw
    }
    [IO.File]::Delete($backup)
} catch {
    try { Start-Process -FilePath $target -WorkingDirectory ([IO.Path]::GetDirectoryName($target)) } catch { }
    try { [IO.File]::AppendAllText($target + '.update.log', $_.ToString() + [Environment]::NewLine) } catch { }
} finally {
    if ([IO.File]::Exists($staged)) { [IO.File]::Delete($staged) }
}
