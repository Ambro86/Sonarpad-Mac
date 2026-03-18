param(
    [string]$BrokenJsonPath = '',
    [int]$TimeoutSeconds = 8,
    [string]$OutputJsonPath = '',
    [string]$OutputTxtPath = ''
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($BrokenJsonPath)) {
    $BrokenJsonPath = Join-Path $PSScriptRoot 'radio-it-check-results.json'
}
if ([string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    $OutputJsonPath = Join-Path $PSScriptRoot 'radiogiuste.json'
}
if ([string]::IsNullOrWhiteSpace($OutputTxtPath)) {
    $OutputTxtPath = Join-Path $PSScriptRoot 'radiogiuste.txt'
}

function Remove-Diacritics {
    param([string]$Value)

    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ''
    }

    $normalized = $Value.Normalize([Text.NormalizationForm]::FormD)
    $builder = New-Object System.Text.StringBuilder
    foreach ($ch in $normalized.ToCharArray()) {
        if ([Globalization.CharUnicodeInfo]::GetUnicodeCategory($ch) -ne [Globalization.UnicodeCategory]::NonSpacingMark) {
            [void]$builder.Append($ch)
        }
    }
    $builder.ToString().Normalize([Text.NormalizationForm]::FormC)
}

function Normalize-StationName {
    param([string]$Name)

    if ([string]::IsNullOrWhiteSpace($Name)) {
        return ''
    }

    $value = (Remove-Diacritics $Name).Trim().ToLowerInvariant().Replace('&', ' and ')
    $builder = New-Object System.Text.StringBuilder
    $previousKind = 'space'

    foreach ($ch in $value.ToCharArray()) {
        if ([char]::IsLetter($ch)) {
            if ($previousKind -eq 'digit') {
                [void]$builder.Append(' ')
            }
            [void]$builder.Append($ch)
            $previousKind = 'letter'
            continue
        }

        if ([char]::IsDigit($ch)) {
            if ($previousKind -eq 'letter') {
                [void]$builder.Append(' ')
            }
            [void]$builder.Append($ch)
            $previousKind = 'digit'
            continue
        }

        if ($previousKind -ne 'space') {
            [void]$builder.Append(' ')
            $previousKind = 'space'
        }
    }

    $words = New-Object System.Collections.ArrayList
    foreach ($word in ($builder.ToString().Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries))) {
        $mapped = switch ($word) {
            'uno' { '1' }
            'due' { '2' }
            'tre' { '3' }
            'quattro' { '4' }
            'cinque' { '5' }
            'sei' { '6' }
            'sette' { '7' }
            'otto' { '8' }
            'nove' { '9' }
            'dieci' { '10' }
            default { $word }
        }

        if ($mapped -in @('m3u', 'playlist', 'station')) {
            continue
        }

        [void]$words.Add($mapped)
    }

    ($words -join ' ').Trim()
}

function Get-CompactName {
    param([string]$Name)

    (Normalize-StationName $Name).Replace(' ', '')
}

function Get-NameTokens {
    param([string]$Name)

    $stopWords = @(
        'radio', 'fm', 'am', 'italia', 'italiano', 'italiana',
        'stereo', 'network', 'station', 'stream', 'live',
        'il', 'sole', 'ore', 'the', 'hq', 'hd'
    )

    @(
        (Normalize-StationName $Name).Split(' ', [System.StringSplitOptions]::RemoveEmptyEntries) |
            Where-Object { $_ -and $_ -notin $stopWords }
    )
}

function Format-ResponseText {
    param($Response)

    $lines = New-Object System.Collections.ArrayList
    [void]$lines.Add("HTTP/$($Response.Version) $([int]$Response.StatusCode) $($Response.ReasonPhrase)")
    foreach ($header in $Response.Headers) {
        [void]$lines.Add("$($header.Key): $([string]::Join(', ', $header.Value))")
    }
    foreach ($header in $Response.Content.Headers) {
        [void]$lines.Add("$($header.Key): $([string]::Join(', ', $header.Value))")
    }
    ($lines -join [Environment]::NewLine).Trim()
}

function Test-RadioUrl {
    param([string]$Url, [int]$TimeoutSeconds = 8)

    if ([string]::IsNullOrWhiteSpace($Url)) {
        return [pscustomobject]@{ ok = $false; mode = 'HEAD'; detail = 'Empty URL' }
    }
    if ($Url.Length -gt 2048) {
        return [pscustomobject]@{ ok = $false; mode = 'HEAD'; detail = 'URL too long' }
    }

    $uri = $null
    if (-not [System.Uri]::TryCreate($Url, [System.UriKind]::Absolute, [ref]$uri)) {
        return [pscustomobject]@{ ok = $false; mode = 'HEAD'; detail = 'Invalid absolute URL' }
    }

    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $true
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.Timeout = [TimeSpan]::FromSeconds($TimeoutSeconds)

    try {
        $headRequest = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Head, $uri)
        try {
            $headResponse = $client.SendAsync($headRequest, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
            $headText = Format-ResponseText $headResponse
            if (
                [int]$headResponse.StatusCode -ge 200 -and [int]$headResponse.StatusCode -lt 300 -and (
                    $headText.Contains('Content-Type: audio/') -or
                    $headText.Contains('Content-Type: application/vnd.apple.mpegurl') -or
                    $headText.Contains('Content-Type: application/x-mpegurl') -or
                    $headText.Contains('icy-')
                )
            ) {
                return [pscustomobject]@{ ok = $true; mode = 'HEAD'; detail = $headText }
            }
        } catch {
            $headText = $_.Exception.Message
        } finally {
            if ($null -ne $headResponse) { $headResponse.Dispose() }
            $headRequest.Dispose()
        }

        $rangeRequest = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Get, $uri)
        $rangeRequest.Headers.Range = [System.Net.Http.Headers.RangeHeaderValue]::new(0, 0)
        try {
            $rangeResponse = $client.SendAsync($rangeRequest, [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead).GetAwaiter().GetResult()
            $rangeText = Format-ResponseText $rangeResponse
            if (
                [int]$rangeResponse.StatusCode -ge 200 -and [int]$rangeResponse.StatusCode -lt 300 -and (
                    $rangeText.Contains('Content-Type: audio/') -or
                    $rangeText.Contains('Content-Type: application/vnd.apple.mpegurl') -or
                    $rangeText.Contains('Content-Type: application/x-mpegurl') -or
                    $rangeText.Contains('icy-')
                )
            ) {
                return [pscustomobject]@{ ok = $true; mode = 'RANGE'; detail = $rangeText }
            }
        } catch {
            $rangeText = $_.Exception.Message
        } finally {
            if ($null -ne $rangeResponse) { $rangeResponse.Dispose() }
            $rangeRequest.Dispose()
        }

        return [pscustomobject]@{
            ok = $false
            mode = if ([string]::IsNullOrWhiteSpace($rangeText)) { 'HEAD' } else { 'RANGE' }
            detail = if ([string]::IsNullOrWhiteSpace($rangeText)) { $headText } else { $rangeText }
        }
    } finally {
        $client.Dispose()
        $handler.Dispose()
    }
}

function New-PoolEntry {
    param(
        [string]$Name,
        [string]$StreamUrl,
        [string]$Source,
        [int]$Votes = 0,
        [int]$Bitrate = 0
    )

    $normalizedName = Normalize-StationName $Name
    [pscustomobject]@{
        name = $Name
        stream_url = $StreamUrl.Trim()
        source = $Source
        votes = $Votes
        bitrate = $Bitrate
        normalized_name = $normalizedName
        compact_name = $normalizedName.Replace(' ', '')
        tokens = @(Get-NameTokens $Name)
    }
}

function Add-PoolEntries {
    param(
        [System.Collections.ArrayList]$Target,
        [System.Collections.Generic.HashSet[string]]$Seen,
        [object[]]$Items,
        [string]$Source
    )

    foreach ($item in $Items) {
        $url = [string]$item.stream_url
        $name = [string]$item.name
        if ([string]::IsNullOrWhiteSpace($url) -or [string]::IsNullOrWhiteSpace($name)) {
            continue
        }

        $key = '{0}|{1}' -f $name.Trim().ToLowerInvariant(), $url.Trim().ToLowerInvariant()
        if (-not $Seen.Add($key)) {
            continue
        }

        [void]$Target.Add((New-PoolEntry -Name $name -StreamUrl $url -Source $Source))
    }
}

function Get-CandidateItems {
    param(
        $BrokenItem,
        [object[]]$Pool
    )

    $brokenNorm = Normalize-StationName $BrokenItem.name
    $brokenCompact = Get-CompactName $BrokenItem.name
    $brokenTokens = @(Get-NameTokens $BrokenItem.name)
    $tokenSet = New-Object 'System.Collections.Generic.HashSet[string]'
    foreach ($token in $brokenTokens) {
        [void]$tokenSet.Add($token)
    }

    $rows = foreach ($candidate in $Pool) {
        if ($candidate.stream_url -eq $BrokenItem.stream_url) {
            continue
        }

        $overlap = 0
        foreach ($candidateToken in $candidate.tokens) {
            if ($tokenSet.Contains($candidateToken)) {
                $overlap++
            }
        }

        $score = 0
        if ($candidate.normalized_name -eq $brokenNorm) {
            $score += 1000
        }
        if ($candidate.compact_name -eq $brokenCompact) {
            $score += 950
        }
        if ($candidate.normalized_name.StartsWith($brokenNorm) -or $brokenNorm.StartsWith($candidate.normalized_name)) {
            $score += 400
        }
        if ($candidate.compact_name.Contains($brokenCompact) -or $brokenCompact.Contains($candidate.compact_name)) {
            $score += 300
        }
        if ($overlap -gt 0) {
            $score += 100 + ($overlap * 25)
        }
        if ($candidate.source -eq 'verified-working-it') {
            $score += 50
        }

        if ($score -le 0) {
            continue
        }

        [pscustomobject]@{
            candidate = $candidate
            score = $score
            overlap = $overlap
            extra_tokens = [Math]::Abs($candidate.tokens.Count - $brokenTokens.Count)
        }
    }

    @(
        $rows |
            Sort-Object @{ Expression = { -$_.score } }, @{ Expression = { -$_.overlap } }, extra_tokens, @{ Expression = { -$_.candidate.votes } }, @{ Expression = { -$_.candidate.bitrate } } |
            Select-Object -First 20
    )
}

if (-not (Test-Path $BrokenJsonPath)) {
    throw "Report non trovato: $BrokenJsonPath"
}

$allResults = @(Get-Content $BrokenJsonPath -Raw | ConvertFrom-Json)
$broken = @($allResults | Where-Object { -not $_.ok })
$working = @($allResults | Where-Object { $_.ok })

$poolList = New-Object System.Collections.ArrayList
$seenPoolKeys = New-Object 'System.Collections.Generic.HashSet[string]'

Add-PoolEntries -Target $poolList -Seen $seenPoolKeys -Items $working -Source 'verified-working-it'

$radioBrowserRaw = @(Invoke-RestMethod -Uri 'https://de1.api.radio-browser.info/json/stations/search?countrycode=IT&hidebroken=true&order=clickcount&reverse=true&limit=100000' -TimeoutSec 60)
$browserItems = foreach ($raw in $radioBrowserRaw) {
    $streamUrl = if ([string]::IsNullOrWhiteSpace($raw.url_resolved)) { [string]$raw.url } else { [string]$raw.url_resolved }
    if ([string]::IsNullOrWhiteSpace($streamUrl)) {
        continue
    }

    [pscustomobject]@{
        name = [string]$raw.name
        stream_url = $streamUrl.Trim()
        votes = [int](@($raw.votes) | Select-Object -First 1)
        bitrate = [int](@($raw.bitrate) | Select-Object -First 1)
    }
}
foreach ($item in $browserItems) {
    $url = [string]$item.stream_url
    $name = [string]$item.name
    if ([string]::IsNullOrWhiteSpace($url) -or [string]::IsNullOrWhiteSpace($name)) {
        continue
    }

    $key = '{0}|{1}' -f $name.Trim().ToLowerInvariant(), $url.Trim().ToLowerInvariant()
    if (-not $seenPoolKeys.Add($key)) {
        continue
    }

    [void]$poolList.Add((New-PoolEntry -Name $name -StreamUrl $url -Source 'radio-browser-it' -Votes $item.votes -Bitrate $item.bitrate))
}
$pool = @($poolList)

$fixed = New-Object System.Collections.ArrayList
$unresolved = New-Object System.Collections.ArrayList

foreach ($item in $broken) {
    Write-Host "Cerco sostituti per: $($item.name)"
    $candidates = @(Get-CandidateItems -BrokenItem $item -Pool $pool)
    $resolved = $null

    foreach ($candidateRow in $candidates) {
        $candidate = $candidateRow.candidate
        $probe = Test-RadioUrl -Url $candidate.stream_url -TimeoutSeconds $TimeoutSeconds
        if ($probe.ok) {
            $resolved = [pscustomobject]@{
                name = [string]$item.name
                old_url = [string]$item.stream_url
                new_url = $candidate.stream_url
                candidate_name = $candidate.name
                source = $candidate.source
                verified = $true
                verify_mode = $probe.mode
            }
            break
        }
    }

    if ($null -ne $resolved) {
        [void]$fixed.Add($resolved)
    } else {
        [void]$unresolved.Add([pscustomobject]@{
            name = [string]$item.name
            old_url = [string]$item.stream_url
            candidates_tried = $candidates.Count
        })
    }
}

$resultObject = [pscustomobject]@{
    fixed = @($fixed | Sort-Object name, old_url)
    unresolved = @($unresolved | Sort-Object name, old_url)
}
$resultObject | ConvertTo-Json -Depth 6 | Set-Content $OutputJsonPath -Encoding UTF8

$txt = New-Object System.Collections.ArrayList
[void]$txt.Add("Trovate sostituzioni: $($fixed.Count)")
[void]$txt.Add("Senza sostituzione: $($unresolved.Count)")
[void]$txt.Add('')
foreach ($row in @($fixed | Sort-Object name, old_url)) {
    [void]$txt.Add("NAME: $($row.name)")
    [void]$txt.Add("OLD_URL: $($row.old_url)")
    [void]$txt.Add("NEW_URL: $($row.new_url)")
    [void]$txt.Add("CANDIDATE_NAME: $($row.candidate_name)")
    [void]$txt.Add("SOURCE: $($row.source)")
    [void]$txt.Add("VERIFY_MODE: $($row.verify_mode)")
    [void]$txt.Add('')
}
if ($unresolved.Count -gt 0) {
    [void]$txt.Add('NON RISOLTE:')
    [void]$txt.Add('')
    foreach ($row in @($unresolved | Sort-Object name, old_url)) {
        [void]$txt.Add("NAME: $($row.name)")
        [void]$txt.Add("OLD_URL: $($row.old_url)")
        [void]$txt.Add("CANDIDATES_TRIED: $($row.candidates_tried)")
        [void]$txt.Add('')
    }
}
$txt | Set-Content $OutputTxtPath -Encoding UTF8

Write-Host "Trovate sostituzioni: $($fixed.Count)"
Write-Host "Senza sostituzione: $($unresolved.Count)"
Write-Host "Report JSON: $OutputJsonPath"
Write-Host "Report TXT: $OutputTxtPath"
