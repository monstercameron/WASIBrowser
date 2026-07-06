@echo off
rem build-c.cmd SOURCE.c OUT.wasm — freestanding GWB C guest build.
rem Buries the mandatory flags (-fno-builtin: clang idiom-recognizes strlen/
rem memcpy loops into libc calls that don't exist in freestanding wasm).
if "%~2"=="" (
    echo usage: build-c.cmd SOURCE.c OUT.wasm
    exit /b 2
)
"C:\Program Files\LLVM\bin\clang.exe" --target=wasm32-unknown-unknown -O2 ^
    -nostdlib -fno-builtin -Wl,--no-entry -Wl,--export-memory ^
    -I"%~dp0..\sdk-c" -o %2 %1
