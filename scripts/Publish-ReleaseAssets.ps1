<#
.SYNOPSIS
  Idempotently attach release assets to a GitHub Release.

.DESCRIPTION
  Shared by .github/workflows/rust-core.yml (native tray) and
  .github/workflows/build-windows.yml (Electron compatibility build).
  Both workflows can run for the same tag at the same time, so:

  * the release is created only if it does not exist yet; if two workflows
    race to create it, the loser falls back to uploading into the winner's
    release;
  * assets are uploaded with --clobber, and each workflow only ever uploads
    file names it owns, so neither overwrites the other's assets;
  * -AppendNote adds a paragraph to the release body once (guarded by a
    marker) instead of replacing the body.

  Requires the gh CLI and GH_TOKEN with contents: write.
#>
param(
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string[]]$Files,
    [string]$Title,
    [string]$Notes,
    [switch]$GenerateNotes,
    [string]$Target,
    [switch]$Prerelease,
    [string]$AppendNote,
    [string]$NoteMarker,
    [string]$Repo = $env:GITHUB_REPOSITORY
)

$ErrorActionPreference = 'Stop'

foreach ($file in $Files) {
    if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
        throw "Release asset not found: $file"
    }
}

function Test-ReleaseExists {
    gh release view $Tag --repo $Repo --json tagName *> $null
    return ($LASTEXITCODE -eq 0)
}

if (-not (Test-ReleaseExists)) {
    $create = @('release', 'create', $Tag, '--repo', $Repo)
    if ($Title) { $create += @('--title', $Title) }
    if ($Notes) { $create += @('--notes', $Notes) }
    if ($GenerateNotes) { $create += '--generate-notes' }
    if ($Target) { $create += @('--target', $Target) }
    if ($Prerelease) { $create += '--prerelease' }

    gh @create
    if ($LASTEXITCODE -ne 0) {
        # The other workflow may have created the release a moment ago.
        if (-not (Test-ReleaseExists)) {
            throw "gh release create failed for $Tag and the release does not exist."
        }
        Write-Host "Release $Tag was created concurrently; uploading into it."
    }
}

gh release upload $Tag @Files --repo $Repo --clobber
if ($LASTEXITCODE -ne 0) {
    throw "gh release upload failed for $Tag."
}

if ($AppendNote) {
    if (-not $NoteMarker) {
        throw "-AppendNote requires -NoteMarker so the note is only added once."
    }
    $body = (gh release view $Tag --repo $Repo --json body --jq .body) -join "`n"
    if ($LASTEXITCODE -ne 0) {
        throw "gh release view failed for $Tag."
    }
    if ($body -notlike "*$NoteMarker*") {
        $updated = "$body`n`n$NoteMarker`n$AppendNote"
        gh release edit $Tag --repo $Repo --notes $updated
        if ($LASTEXITCODE -ne 0) {
            throw "gh release edit failed for $Tag."
        }
    }
}
