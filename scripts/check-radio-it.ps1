param(
    [string]$RadioJsonPath = '',
    [int]$TimeoutSeconds = 8,
    [int]$ThrottleLimit = 24,
    [string]$OutputJsonPath = '',
    [string]$OutputTxtPath = ''
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($RadioJsonPath)) {
    $RadioJsonPath = Join-Path $PSScriptRoot '..\i18n\radio.json'
}

if ([string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    $OutputJsonPath = Join-Path $PSScriptRoot 'radio-it-check-results.json'
}

if ([string]::IsNullOrWhiteSpace($OutputTxtPath)) {
    $OutputTxtPath = Join-Path $PSScriptRoot 'radio-it-broken.txt'
}

if (-not (Test-Path $RadioJsonPath)) {
    throw "radio.json non trovato: $RadioJsonPath"
}

$radioCatalog = Get-Content $RadioJsonPath -Raw | ConvertFrom-Json -AsHashtable
$italianStations = @($radioCatalog['it'])

if ($italianStations.Count -eq 0) {
    throw "Nessuna radio italiana trovata in $RadioJsonPath"
}

$seen = New-Object 'System.Collections.Generic.HashSet[string]'
$stationsToCheck = New-Object System.Collections.ArrayList

foreach ($station in $italianStations) {
    $name = [string]$station.name
    $url = [string]$station.stream_url
    $key = ($name.Trim().ToLowerInvariant() + '|' + $url.Trim())
    if (-not $seen.Add($key)) {
        continue
    }
    [void]$stationsToCheck.Add([pscustomobject]@{
        name = $name
        stream_url = $url
    })
}

$results = @($stationsToCheck | ForEach-Object -Parallel {
    $name = $_.name
    $url = $_.stream_url
    $commonArgs = @(
        '--silent',
        '--show-error',
        '--location',
        '--max-time', $using:TimeoutSeconds,
        '--connect-timeout', ([Math]::Min($using:TimeoutSeconds, 5))
    )

    $headOutput = & curl.exe @commonArgs '--head' $url 2>&1
    $headText = ($headOutput | Out-String).Trim()
    if ($headText -match 'HTTP/\d+(?:\.\d+)?\s+2\d\d' -or $headText -match '(?im)^content-type:\s*(audio/|application/vnd\.apple\.mpegurl|application/x-mpegurl)' -or $headText -match '(?im)^icy-') {
        return [pscustomobject]@{
            name = $name
            stream_url = $url
            ok = $true
            mode = 'HEAD'
            detail = $headText
        }
    }

    $rangeOutput = & curl.exe @commonArgs '--range' '0-0' '--output' 'NUL' '--dump-header' '-' $url 2>&1
    $rangeText = ($rangeOutput | Out-String).Trim()
    if ($rangeText -match 'HTTP/\d+(?:\.\d+)?\s+2\d\d' -or $rangeText -match '(?im)^content-type:\s*(audio/|application/vnd\.apple\.mpegurl|application/x-mpegurl)' -or $rangeText -match '(?im)^icy-') {
        return [pscustomobject]@{
            name = $name
            stream_url = $url
            ok = $true
            mode = 'RANGE'
            detail = $rangeText
        }
    }

    [pscustomobject]@{
        name = $name
        stream_url = $url
        ok = $false
        mode = if ([string]::IsNullOrWhiteSpace($rangeText)) { 'HEAD' } else { 'RANGE' }
        detail = if ([string]::IsNullOrWhiteSpace($rangeText)) { $headText } else { $rangeText }
    }
} -ThrottleLimit $ThrottleLimit)

$results = @($results | Sort-Object name, stream_url)
$results | ConvertTo-Json -Depth 6 | Set-Content $OutputJsonPath -Encoding UTF8

$broken = @($results | Where-Object { -not $_.ok })
$working = @($results | Where-Object { $_.ok })

$txtLines = New-Object System.Collections.ArrayList
[void]$txtLines.Add("Totale controllate: $($results.Count)")
[void]$txtLines.Add("Funzionanti: $($working.Count)")
[void]$txtLines.Add("Non funzionanti: $($broken.Count)")
[void]$txtLines.Add('')
if ($broken.Count -gt 0) {
    foreach ($item in $broken) {
        [void]$txtLines.Add("NAME: $($item.name)")
        [void]$txtLines.Add("URL: $($item.stream_url)")
        [void]$txtLines.Add("MODE: $($item.mode)")
        [void]$txtLines.Add("DETAIL: $($item.detail)")
        [void]$txtLines.Add('')
    }
} else {
    [void]$txtLines.Add('Nessuna radio italiana non funzionante rilevata.')
}
$txtLines | Set-Content $OutputTxtPath -Encoding UTF8

Write-Host "Totale controllate: $($results.Count)"
Write-Host "Funzionanti: $($working.Count)"
Write-Host "Non funzionanti: $($broken.Count)"
Write-Host "Report JSON: $OutputJsonPath"
Write-Host "Report TXT: $OutputTxtPath"
