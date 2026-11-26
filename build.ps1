# Build the Windows x64 portable launcher with its download tool.
param([string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist/The Isle Version Manager'))
$ErrorActionPreference = 'Stop'
$manifest = Join-Path $PSScriptRoot 'src-tauri/Cargo.toml'
$target = Join-Path $PSScriptRoot 'src-tauri/target'
$zip = Join-Path $target 'DepotDownloader-3.4.0-win-x64.zip'
$url = 'https://github.com/SteamRE/DepotDownloader/releases/download/DepotDownloader_3.4.0/DepotDownloader-windows-x64.zip'
# Digest published in Microsoft's WinGet manifest for SteamRE.DepotDownloader 3.4.0.
$digest = '41C9E9F0DF54B3AD02E67A11726756E5C73283BD7C2E1B04ACFA5AE4C2ED3767'

$steamDll = $null
cargo build --bins --release --locked --manifest-path $manifest --target-dir $target --message-format=json-render-diagnostics | ForEach-Object {
    $event = $_ | ConvertFrom-Json
    if ($event.reason -eq 'build-script-executed' -and $event.package_id -match '#steamworks-sys@') {
        $steamDll = Join-Path $event.out_dir 'steam_api64.dll'
    }
}
if ($LASTEXITCODE -ne 0) { throw 'Launcher compilation failed.' }
if (!$steamDll -or !(Test-Path -LiteralPath $steamDll)) { throw 'Steamworks runtime missing from the build.' }
if (!(Test-Path -LiteralPath $zip) -or (Get-FileHash -LiteralPath $zip).Hash -ne $digest) {
    Invoke-WebRequest -Uri $url -OutFile $zip
}
if ((Get-FileHash -LiteralPath $zip).Hash -ne $digest) { throw 'DepotDownloader checksum mismatch.' }

$toolsDir = Join-Path $OutputDirectory 'tools'
New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
Expand-Archive -LiteralPath $zip -DestinationPath $toolsDir -Force
Copy-Item -LiteralPath $steamDll -Destination $toolsDir -Force
Copy-Item -LiteralPath (Join-Path $target 'release/isle-steam-favorite.exe') -Destination $toolsDir -Force
$launcher = Join-Path $OutputDirectory 'The Isle Version Manager.exe'
Copy-Item -LiteralPath (Join-Path $target 'release/isle-launcher.exe') -Destination $launcher -Force
Write-Output $launcher
