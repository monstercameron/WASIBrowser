<#
    build-wincairo.ps1
    Clones and builds the WinCairo (open-source Windows) port of WebKit2, Release/x64.
    Produces MiniBrowser.exe + the WebKit DLLs the DOM agent links against.

    Parallelism defaults to (logical cores - 1) via -Jobs. WebKit's build scripts
    read NUMBER_OF_PROCESSORS to choose the ninja -j count, so we override it.

    x64 build runs under emulation on Windows-ARM64 (the X2). Intentional for the
    prototype: validate the no-JS native-DOM agent first; native ARM64 port later.

    Prereqs (install once, elevated): Visual Studio 2022 with the
    "Desktop development with C++" workload, plus cmake, ninja, perl, ruby, gperf,
    llvm, and Python 3.11 (NOT 3.12+). Pass -SkipDeps if you installed them
    yourself; otherwise this choco-installs the CLI tools (needs admin).

    NOTE: keep this file ASCII-only. Windows PowerShell 5.1 misreads UTF-8 files
    without a BOM, so non-ASCII chars (em dashes, arrows) break parsing.
#>

param(
    [string]$WebKitDir = "C:\src\WebKit",
    [int]$Jobs = ([Environment]::ProcessorCount - 1),
    [switch]$SkipDeps
)

$ErrorActionPreference = "Stop"
if ($Jobs -lt 1) { $Jobs = 1 }

Write-Host "==> Parallelism: $Jobs jobs (logical cores: $([Environment]::ProcessorCount))" -ForegroundColor Cyan

# Installers (and choco) often do not update PATH for the current shell. Probe the
# usual locations and prepend any we find, so off-PATH tools still work. No admin.
$probe = @('C:\Python311','C:\Python311\Scripts','C:\Program Files\CMake\bin',
           'C:\Strawberry\perl\bin','C:\Strawberry\c\bin')
$probe += (Get-ChildItem 'C:\tools' -Directory -Filter 'ruby*' -ErrorAction SilentlyContinue | ForEach-Object { Join-Path $_.FullName 'bin' })
$probe += (Get-ChildItem 'C:\' -Directory -Filter 'Ruby*' -ErrorAction SilentlyContinue | ForEach-Object { Join-Path $_.FullName 'bin' })
# Git for Windows bundles perl (build-webkit is a perl script); use it as a fallback.
$gitcmd = Get-Command git -ErrorAction SilentlyContinue
if ($gitcmd) { $probe += (Join-Path (Split-Path (Split-Path $gitcmd.Source)) 'usr\bin') }
foreach ($d in $probe) {
    if ((Test-Path $d) -and (";$env:PATH;" -notlike "*;$d;*")) {
        $env:PATH = "$d;$env:PATH"
        Write-Host "    PATH += $d" -ForegroundColor DarkGray
    }
}

if (-not $SkipDeps) {
    Write-Host "==> Installing CLI build tools (choco; needs admin)..." -ForegroundColor Cyan
    # Python 3.11 specifically - WebKit's build scripts do NOT support 3.12+.
    choco install -y python311 strawberryperl ruby cmake gperf llvm ninja
    python -m pip install pywin32
}

# --- toolchain check (ADVISORY only; build-webkit is the real authority) -----
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $installs = & $vswhere -all -prerelease -property installationPath 2>$null
    $vc = & $vswhere -all -prerelease -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($vc) {
        Write-Host "==> Visual Studio C++ tools: $vc" -ForegroundColor Green
    } elseif ($installs) {
        Write-Warning ("VS found ({0}) but the 'Desktop development with C++' workload was not detected. If you installed it, ignore this; otherwise add it in the VS Installer." -f $installs)
    } else {
        Write-Warning "No Visual Studio detected. WebKit needs VS 2022 + 'Desktop development with C++'. Continuing; build-webkit will error if the compiler is truly missing."
    }
} else {
    Write-Warning "vswhere not found; skipping VS detection."
}

foreach ($t in 'cmake','ninja','perl','python','ruby') {
    if (-not (Get-Command $t -ErrorAction SilentlyContinue)) {
        Write-Warning ("{0} not on PATH - build may fail without it." -f $t)
    }
}

# --- source -----------------------------------------------------------------
# A partial/aborted clone leaves an unusable dir; start clean if it is not a repo.
if ((Test-Path $WebKitDir) -and -not (Test-Path (Join-Path $WebKitDir ".git\HEAD"))) {
    Write-Host "==> Removing incomplete clone at $WebKitDir..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $WebKitDir
}

if (-not (Test-Path $WebKitDir)) {
    # Shallow, single-branch: latest snapshot only (~a few GB) instead of WebKit's
    # full ~15 GB+ history. Retry on disconnect; bigger buffer for the large pack.
    Write-Host "==> Shallow-cloning WebKit into $WebKitDir (latest only; retries on disconnect)..." -ForegroundColor Cyan
    $ok = $false
    for ($i = 1; $i -le 4 -and -not $ok; $i++) {
        if ($i -gt 1) { Write-Host "    retry $i/4..." -ForegroundColor Yellow; Start-Sleep -Seconds 3 }
        git -c http.postBuffer=1073741824 -c core.compression=0 clone --depth 1 --single-branch --branch main https://github.com/WebKit/WebKit.git $WebKitDir
        if ($LASTEXITCODE -eq 0) { $ok = $true }
        elseif (Test-Path $WebKitDir) { Remove-Item -Recurse -Force $WebKitDir }
    }
    if (-not $ok) { Write-Error "WebKit clone failed after 4 attempts (network). Just re-run the script."; return }
} else {
    Write-Host "==> WebKit present at $WebKitDir, fetching latest (shallow)..." -ForegroundColor Cyan
    git -C $WebKitDir fetch --depth 1 origin main
    git -C $WebKitDir reset --hard origin/main
}

# --- build ------------------------------------------------------------------
Push-Location $WebKitDir
try {
    # Modern WinCairo WebKit manages its C/C++ deps via vcpkg; build-webkit requires
    # VCPKG_ROOT. Bootstrap a local vcpkg (no admin needed) if one isn't configured.
    if (-not $env:VCPKG_ROOT -or -not (Test-Path (Join-Path $env:VCPKG_ROOT 'vcpkg.exe'))) {
        $vcpkgDir = 'C:\src\vcpkg'
        if (-not (Test-Path (Join-Path $vcpkgDir '.git'))) {
            Write-Host "==> Cloning vcpkg into $vcpkgDir..." -ForegroundColor Cyan
            git clone --depth 1 https://github.com/microsoft/vcpkg.git $vcpkgDir
        }
        if (-not (Test-Path (Join-Path $vcpkgDir 'vcpkg.exe'))) {
            Write-Host "==> Bootstrapping vcpkg..." -ForegroundColor Cyan
            & (Join-Path $vcpkgDir 'bootstrap-vcpkg.bat') -disableMetrics
        }
        $env:VCPKG_ROOT = $vcpkgDir
    }
    Write-Host "==> VCPKG_ROOT = $env:VCPKG_ROOT" -ForegroundColor Green

    # Set up the MSVC build environment. build-webkit (ninja generator) and vcpkg
    # need cl.exe + INCLUDE/LIB in the ambient env, or the compile fails. Host is
    # arm64 (X2), target is x64 -> use the arm64_x64 cross toolchain. Glob for
    # vcvarsall.bat because vswhere does not recognize VS 18 here.
    $vcvars = @(
        'C:\Program Files (x86)\Microsoft Visual Studio\*\*\VC\Auxiliary\Build\vcvarsall.bat',
        'C:\Program Files\Microsoft Visual Studio\*\*\VC\Auxiliary\Build\vcvarsall.bat'
    ) | ForEach-Object { Get-ChildItem $_ -ErrorAction SilentlyContinue } | Select-Object -First 1 -ExpandProperty FullName
    if (-not $vcvars) { Write-Error "vcvarsall.bat not found - is the VS 'Desktop development with C++' workload installed?"; return }
    Write-Host "==> MSVC env: `"$vcvars`" arm64_x64" -ForegroundColor Cyan
    cmd /c "`"$vcvars`" arm64_x64 > nul 2>&1 && set" | ForEach-Object {
        $i = $_.IndexOf('='); if ($i -gt 0) { Set-Item -Path "env:$($_.Substring(0,$i))" -Value $_.Substring($i+1) -ErrorAction SilentlyContinue }
    }
    $clcmd = Get-Command cl.exe -ErrorAction SilentlyContinue
    if ($clcmd) { Write-Host "    cl.exe: $($clcmd.Source)" -ForegroundColor Green } else { Write-Warning "cl.exe not on PATH after vcvars" }

    # Workaround: the VS ARM64 linker crashes (LNK1000 IncrCalcPtrs, access
    # violation) during the INCREMENTAL link of DEBUG vcpkg dep builds. Forcing
    # release-only deps skips the debug/incremental link entirely (release links
    # non-incrementally) and halves dep build time. WebKit Release needs only
    # release deps. Re-applied every run because git reset --hard wipes it.
    $triplet = Join-Path $WebKitDir 'WebKitLibraries\triplets\x64-windows-webkit.cmake'
    if (Test-Path $triplet) {
        if ((Get-Content $triplet -Raw) -notmatch 'GWB: release-only') {
            Add-Content -Path $triplet -Value "`nset(VCPKG_BUILD_TYPE release)  # GWB: release-only (ARM64 debug-link LNK1000 workaround)"
            Write-Host "==> Patched triplet: VCPKG_BUILD_TYPE release (ARM64 link workaround)" -ForegroundColor Cyan
        } else {
            Write-Host "==> Triplet already release-only" -ForegroundColor DarkGray
        }
    }

    # WebKit (OptionsMSVC.cmake) unconditionally links clang's compiler-rt
    # builtins, searching next to the compiler and in WebKitLibraries/windows.
    # The x86_64 lib ships with our LLVM; stage it where WebKit looks.
    $rtSrc = Get-ChildItem 'C:\Program Files\LLVM\lib\clang\*\lib\windows\clang_rt.builtins-x86_64.lib' -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty FullName
    if ($rtSrc) {
        $rtDst = Join-Path $WebKitDir 'WebKitLibraries\windows'
        New-Item -ItemType Directory -Force -Path $rtDst | Out-Null
        Copy-Item $rtSrc (Join-Path $rtDst 'clang_rt.builtins-x86_64.lib') -Force
        Write-Host "==> Staged clang_rt.builtins-x86_64.lib into WebKitLibraries\windows" -ForegroundColor Cyan
    } else {
        Write-Warning "clang_rt.builtins-x86_64.lib not found under C:\Program Files\LLVM"
    }

    # Cap parallelism at cores-1. WebKit's numberOfCPUs() reads NUMBER_OF_PROCESSORS
    # on Windows and passes it as ninja -j.
    $env:NUMBER_OF_PROCESSORS = "$Jobs"
    Write-Host "==> Building WebKit (Release, x64, -j$Jobs). Hours on first run." -ForegroundColor Cyan
    perl Tools/Scripts/build-webkit --release

    $miniBrowser = Join-Path $WebKitDir "WebKitBuild\Release\bin64\MiniBrowser.exe"
    if (Test-Path $miniBrowser) {
        Write-Host "==> DONE. MiniBrowser: $miniBrowser" -ForegroundColor Green
        Write-Host "    DLLs for the DOM agent are in the same bin64 folder." -ForegroundColor Green
    } else {
        Write-Warning "Build finished but MiniBrowser.exe not found. Check WebKitBuild\Release\bin64."
    }
}
finally {
    Pop-Location
}

# Incremental rebuilds: perl Tools/Scripts/build-webkit --release --skip-library-update
