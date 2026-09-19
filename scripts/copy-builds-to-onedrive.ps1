$sharedOneDriveRoot = "C:\Users\twues\OneDrive\Work"
$sharedBuildDirectory = Join-Path $sharedOneDriveRoot "1-My Own\7 - RBX\RBxTools\Builds"

# GitHub-hosted runners do not have access to the user's personal OneDrive.
if (-not (Test-Path -LiteralPath $sharedOneDriveRoot)) {
    Write-Host "Shared OneDrive root is unavailable; skipping local build publish."
    exit 0
}

$projectRoot = Split-Path -Parent $PSScriptRoot
$releaseDirectory = Join-Path $projectRoot "src-tauri\target\release"
$executable = Join-Path $releaseDirectory "rbx-tools.exe"
$installerDirectory = Join-Path $releaseDirectory "bundle\nsis"

if (-not (Test-Path -LiteralPath $executable)) {
    throw "Expected desktop executable was not created: $executable"
}

$installer = Get-ChildItem -LiteralPath $installerDirectory -Filter "*-setup.exe" -File |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if (-not $installer) {
    throw "Expected NSIS installer was not created in: $installerDirectory"
}

New-Item -ItemType Directory -Force -Path $sharedBuildDirectory | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $sharedBuildDirectory "RBX-Tools.exe") -Force
Copy-Item -LiteralPath $installer.FullName -Destination (Join-Path $sharedBuildDirectory $installer.Name) -Force

$package = Get-Content -LiteralPath (Join-Path $projectRoot "package.json") -Raw | ConvertFrom-Json
$manifest = [ordered]@{
    version = $package.version
    installer = $installer.Name
    notes = "Shared OneDrive build"
    publishedAt = (Get-Date).ToUniversalTime().ToString("o")
} | ConvertTo-Json
Set-Content -LiteralPath (Join-Path $sharedBuildDirectory "latest.json") -Value $manifest -Encoding utf8

Write-Host "Published executable, installer, and update manifest to: $sharedBuildDirectory"
