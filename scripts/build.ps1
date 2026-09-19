param([ValidateSet('build', 'dev', 'check')][string]$Mode = 'build')
$ErrorActionPreference = 'Stop'
$rbxSource = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$rbxCache = Join-Path $env:LOCALAPPDATA 'RBXTools-build'
$env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
$env:CARGO_BUILD_JOBS = '2'
New-Item -ItemType Directory -Path $rbxCache -Force | Out-Null
$rbxCache = (& node -p "require('fs').realpathSync.native(process.argv[1])" $rbxCache).Trim()
# Build outside Documents so Windows Controlled Folder Access need not be disabled.
foreach ($rbxItem in @('package.json','package-lock.json','tsconfig.json','vite.config.ts','index.html','src','assets')) {
    $rbxPath = Join-Path $rbxSource $rbxItem
    if (Test-Path -LiteralPath $rbxPath) { Copy-Item -LiteralPath $rbxPath -Destination $rbxCache -Recurse -Force }
}
$rbxNative = Join-Path $rbxCache 'src-tauri'
New-Item -ItemType Directory -Path $rbxNative -Force | Out-Null
foreach ($rbxItem in @('Cargo.toml','Cargo.lock','build.rs','app.manifest','tauri.conf.json','capabilities','src','icons','examples')) {
    $rbxPath = Join-Path $rbxSource ('src-tauri\' + $rbxItem)
    if (Test-Path -LiteralPath $rbxPath) { Copy-Item -LiteralPath $rbxPath -Destination $rbxNative -Recurse -Force }
}
Push-Location $rbxCache
try {
    & npm.cmd install --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'Dependency installation failed.' }
    if (!(Test-Path 'src-tauri\icons\icon.ico')) {
        & npm.cmd run tauri -- icon assets/app-icon.svg
        if ($LASTEXITCODE -ne 0) { throw 'Icon generation failed.' }
    }
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
            $rbxInstaller = Get-ChildItem 'src-tauri\target\release\bundle\nsis' -Filter '*-setup.exe' | Select-Object -First 1
            if ($rbxInstaller) { Copy-Item -LiteralPath $rbxInstaller.FullName -Destination (Join-Path $rbxRelease 'RBX-Tools-Setup.exe') -Force }
            Write-Host "Ready: $rbxRelease"
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "$Mode failed ($LASTEXITCODE)." }
} finally { Pop-Location }
