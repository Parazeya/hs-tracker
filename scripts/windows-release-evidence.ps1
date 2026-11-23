[CmdletBinding()]
param(
    [string]$File = (Join-Path $PSScriptRoot '..\src-tauri\target\release\hs-tracker.exe'),
    [string]$Output = (Join-Path $PSScriptRoot '..\tools\data\windows-release-evidence.json'),
    [switch]$Scan
)
$ErrorActionPreference = 'Stop'
$target = (Resolve-Path -LiteralPath $File).Path
$artifact = Get-Item -LiteralPath $target
if ($artifact.PSIsContainer) { throw 'Select one release file, not a directory.' }
$signature = Get-AuthenticodeSignature -LiteralPath $target
$evidence = [ordered]@{
    schema_version = 1
    checked_at_utc = [DateTime]::UtcNow.ToString('o')
    file = $target
    bytes = $artifact.Length
    sha256 = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash.ToLowerInvariant()
    authenticode = [ordered]@{
        status = $signature.Status.ToString()
        publisher = if ($signature.SignerCertificate) { $signature.SignerCertificate.Subject } else { $null }
        timestamped = [bool]$signature.TimeStamperCertificate
    }
    scan = [ordered]@{ requested = [bool]$Scan; remediation_enabled = $false; manual_sample_submission = $false }
    limits = @('A local scan is not a guarantee for other antivirus engines or future signatures.',
        'Authenticode, updater signatures, SmartScreen reputation and malware verdicts are separate checks.',
        'The scanner retains the machine''s existing cloud policy; this script changes no protection settings.')
}
try {
    $defender = Get-MpComputerStatus
    $evidence.defender = [ordered]@{ engine = $defender.AMProductVersion; signatures = $defender.AntivirusSignatureVersion; enabled = $defender.AntivirusEnabled }
} catch { $evidence.defender = @{ status = 'unavailable'; reason = $_.Exception.Message } }
if ($Scan) {
    $platform = Join-Path $env:ProgramData 'Microsoft\Windows Defender\Platform'
    $candidate = Get-ChildItem -LiteralPath $platform -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^\d+\.' } | Sort-Object Name -Descending |
        ForEach-Object { Join-Path $_.FullName 'MpCmdRun.exe' } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    if (-not $candidate) { $candidate = Join-Path $env:ProgramFiles 'Windows Defender\MpCmdRun.exe' }
    if (-not (Test-Path -LiteralPath $candidate)) { throw 'Microsoft Defender command-line scanner was not found.' }
    $scanText = & $candidate -Scan -ScanType 3 -File $target -DisableRemediation 2>&1 | Out-String
    $evidence.scan.exit_code = $LASTEXITCODE
    $evidence.scan.output = $scanText.Trim()
}
$destination = [IO.Path]::GetFullPath($Output)
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($destination)) | Out-Null
$evidence | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $destination -Encoding UTF8
[pscustomobject]@{ report = $destination; sha256 = $evidence.sha256; signature = $evidence.authenticode.status; scan_exit_code = $evidence.scan.exit_code } | ConvertTo-Json
