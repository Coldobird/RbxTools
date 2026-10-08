param([ValidateSet('build', 'dev', 'check')][string]$Mode = 'build')
$ErrorActionPreference = 'Stop'
$rbxSource = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$rbxCache = Join-Path $env:LOCALAPPDATA 'RBXTools-build'
$env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
New-Item -ItemType Directory -Path $rbxCache -Force | Out-Null
$rbxCache = (& node -p "require('fs').realpathSync.native(process.argv[1])" $rbxCache).Trim()

function Sync-RbxSource([string]$Source, [string]$Destination) {
    if (Test-Path -LiteralPath $Source -PathType Container) {
        New-Item -ItemType Directory -Path $Destination -Force | Out-Null
        foreach ($rbxFile in Get-ChildItem -LiteralPath $Source -Recurse -File) {
            $rbxRelative = $rbxFile.FullName.Substring($Source.Length).TrimStart('\')
            Sync-RbxSource $rbxFile.FullName (Join-Path $Destination $rbxRelative)
        }
        # Only prune files inside this source-owned staging directory.
        foreach ($rbxFile in Get-ChildItem -LiteralPath $Destination -Recurse -File) {
            $rbxRelative = $rbxFile.FullName.Substring($Destination.Length).TrimStart('\')
            if (-not (Test-Path -LiteralPath (Join-Path $Source $rbxRelative))) {
                Remove-Item -LiteralPath $rbxFile.FullName -Force
            }
        }
    } else {
        if ((Test-Path -LiteralPath $Destination -PathType Leaf) -and
            (Get-FileHash -LiteralPath $Source).Hash -eq (Get-FileHash -LiteralPath $Destination).Hash) {
            return
        }
        New-Item -ItemType Directory -Path (Split-Path -Parent $Destination) -Force | Out-Null
        Copy-Item -LiteralPath $Source -Destination $Destination -Force
    }
}
# Build outside Documents so Windows Controlled Folder Access need not be disabled.
foreach ($rbxItem in @('package.json','package-lock.json','tsconfig.json','vite.config.ts','index.html','src','assets','public','scripts')) {
    $rbxPath = Join-Path $rbxSource $rbxItem
    if (Test-Path -LiteralPath $rbxPath) { Sync-RbxSource $rbxPath (Join-Path $rbxCache $rbxItem) }
}
$rbxNative = Join-Path $rbxCache 'src-tauri'
New-Item -ItemType Directory -Path $rbxNative -Force | Out-Null
foreach ($rbxItem in @('Cargo.toml','Cargo.lock','build.rs','app.manifest','tauri.conf.json','capabilities','src','icons','examples')) {
    $rbxPath = Join-Path $rbxSource ('src-tauri\' + $rbxItem)
    if (Test-Path -LiteralPath $rbxPath) { Sync-RbxSource $rbxPath (Join-Path $rbxNative $rbxItem) }
}
Push-Location $rbxCache
try {
    $rbxDependencyMarker = Join-Path $rbxCache '.dependency-hash'
    $rbxDependencyHash = (Get-FileHash -LiteralPath 'package-lock.json').Hash + ':' +
        (& node -p 'JSON.stringify([process.versions.modules,process.platform,process.arch])')
    if (-not (Test-Path -LiteralPath $rbxDependencyMarker) -or
        (Get-Content -LiteralPath $rbxDependencyMarker -Raw).Trim() -ne $rbxDependencyHash -or
        -not (Test-Path -LiteralPath 'node_modules\@tauri-apps\cli\tauri.js') -or
        -not (Test-Path -LiteralPath 'node_modules\.bin\vite.cmd')) {
        & npm.cmd ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed.' }
        Set-Content -LiteralPath $rbxDependencyMarker -Value $rbxDependencyHash
    } else {
        Write-Host 'Dependencies are up to date.'
    }
    & npm.cmd run icons
    if ($LASTEXITCODE -ne 0) { throw 'Icon generation failed.' }
    switch ($Mode) {
        'dev' { & npm.cmd run desktop }
        'check' {
            & npm.cmd run build
            if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
            & npm.cmd test
            if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed.' }
            & cargo check --manifest-path src-tauri/Cargo.toml
        }
        'build' {
            & npm.cmd run desktop:build
            if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed.' }
            $rbxRelease = Join-Path $rbxCache 'release'
            New-Item -ItemType Directory -Path $rbxRelease -Force | Out-Null
            Copy-Item -LiteralPath 'src-tauri\target\release\rbx-tools.exe' -Destination (Join-Path $rbxRelease 'RBX-Tools.exe') -Force
            $rbxVersion = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
            $rbxInstaller = Get-ChildItem 'src-tauri\target\release\bundle\nsis' -Filter "*_$($rbxVersion)_*-setup.exe" |
                Sort-Object LastWriteTime -Descending | Select-Object -First 1
            if ($rbxInstaller) { Copy-Item -LiteralPath $rbxInstaller.FullName -Destination (Join-Path $rbxRelease 'RBX-Tools-Setup.exe') -Force }
            Write-Host "Ready: $rbxRelease"
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "$Mode failed ($LASTEXITCODE)." }
} finally { Pop-Location }
