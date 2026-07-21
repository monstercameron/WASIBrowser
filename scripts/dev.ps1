# dev.ps1 — build everything and launch the browser + the three demo backends.
#
# Usage:
#   scripts\dev.ps1                  build + launch, opens web://shop.local
#   scripts\dev.ps1 -Target search    same, but opens web://search.local
#   scripts\dev.ps1 -SkipBuild        skip all builds, just (re)launch
#
# Servers are left running in the background across repeated runs — each one
# is only (re)built and (re)started if its port isn't already listening.

param(
    [ValidateSet("shop", "search", "retailer")]
    [string]$Target = "shop",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Test-Port($port) {
    $conn = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
    return $null -ne $conn
}

function Build-Step($label, $block) {
    Write-Host "==> $label" -ForegroundColor Cyan
    & $block
    if ($LASTEXITCODE -and $LASTEXITCODE -ne 0) {
        throw "$label failed (exit $LASTEXITCODE)"
    }
}

if (-not $SkipBuild) {
    Build-Step "renderer (cargo build --release)" {
        Push-Location "$root\renderer"
        cargo build --release
        Pop-Location
    }

    Build-Step "shop-server (go build)" {
        Push-Location "$root\server"
        go build -o shop-server.exe .
        Pop-Location
    }
    Build-Step "search-server (cargo build --release)" {
        Push-Location "$root\search-server"
        cargo build --release
        Pop-Location
    }
    Build-Step "retailer-server (go build)" {
        Push-Location "$root\retailer-server"
        go build -o retailer-server.exe .
        Pop-Location
    }

    Build-Step "shop.wasm (clang, freestanding)" {
        Push-Location "$root\examples\shop-c"
        $clangArgs = @(
            "--target=wasm32-unknown-unknown", "-O2", "-nostdlib", "-fno-builtin",
            "-Wl,--no-entry", "-Wl,--export-memory",
            "-I", "$root\sdk-c", "-o", "shop.wasm", "shop.c"
        )
        & "C:\Program Files\LLVM\bin\clang.exe" @clangArgs
        Copy-Item shop.wasm "$root\renderer\shop.wasm" -Force
        Pop-Location
    }
    Build-Step "search.wasm (cargo build --release, wasm32-wasip1)" {
        Push-Location "$root\examples\search-rs"
        cargo build --release --target wasm32-wasip1
        Copy-Item target\wasm32-wasip1\release\search_rs.wasm "$root\renderer\search.wasm" -Force
        Pop-Location
    }
    Build-Step "retailer.wasm (go build, wasip1 reactor)" {
        Push-Location "$root\examples\retailer-go"
        $env:GOOS = "wasip1"; $env:GOARCH = "wasm"
        go build -buildmode=c-shared -o retailer.wasm .
        Remove-Item Env:\GOOS, Env:\GOARCH
        Copy-Item retailer.wasm "$root\renderer\retailer.wasm" -Force
        Pop-Location
    }
}

$servers = @(
    @{ Name = "shop-server";     Port = 8787; Exe = "$root\server\shop-server.exe";                  Dir = "$root\server" },
    @{ Name = "search-server";   Port = 8788; Exe = "$root\search-server\target\release\search-server.exe"; Dir = "$root\search-server" },
    @{ Name = "retailer-server"; Port = 8789; Exe = "$root\retailer-server\retailer-server.exe";      Dir = "$root\retailer-server" }
)

foreach ($s in $servers) {
    if (Test-Port $s.Port) {
        Write-Host "==> $($s.Name) already listening on :$($s.Port), leaving it alone" -ForegroundColor DarkGray
        continue
    }
    Write-Host "==> starting $($s.Name) on :$($s.Port)" -ForegroundColor Cyan
    Start-Process -FilePath $s.Exe -WorkingDirectory $s.Dir -WindowStyle Hidden
}

Write-Host "==> launching renderer -> web://$Target.local" -ForegroundColor Cyan
Push-Location "$root\renderer"
Start-Process -FilePath ".\target\release\renderer.exe" `
    -ArgumentList "web://$Target.local", "--manifest-root", "..\manifests"
Pop-Location
