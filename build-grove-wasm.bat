@echo off
setlocal

rem ===========================================================================
rem  build-grove-wasm.bat
rem  Compiles the grove Tree-sitter grammar to WebAssembly and places the two
rem  .wasm files the editor loads into static\grove\ (create or overwrite):
rem    - tree-sitter-grove.wasm  (the grove grammar, built from the crate)
rem    - tree-sitter.wasm        (the web-tree-sitter runtime core, copied)
rem
rem  Prerequisites:
rem    - tree-sitter CLI on PATH                 (tree-sitter --version)
rem    - Emscripten (emcc) on PATH OR Docker Desktop running
rem    - "yarn install" already run              (provides the runtime core)
rem  Re-run after changing grammar.js / scanner.c (after tree-sitter generate).
rem ===========================================================================

set "ROOT=%~dp0"
set "CRATE=%ROOT%crates\grove\arbor-grove-lang"
set "DEST=%ROOT%static\grove"
set "CORE=%ROOT%node_modules\web-tree-sitter\tree-sitter.wasm"

echo.
echo === grove wasm build ===
echo Root: %ROOT%

if not exist "%DEST%" mkdir "%DEST%"

rem --- 1) compile the grammar -> tree-sitter-grove.wasm -----------------------
echo.
echo [1/2] Building grammar wasm (Docker/Emscripten)...
pushd "%CRATE%"
tree-sitter build --wasm
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :build_failed
if not exist "%CRATE%\tree-sitter-grove.wasm" goto :build_failed
move /Y "%CRATE%\tree-sitter-grove.wasm" "%DEST%\tree-sitter-grove.wasm" >nul
if errorlevel 1 goto :build_failed
echo       OK -^> static\grove\tree-sitter-grove.wasm

rem --- 2) copy the web-tree-sitter runtime core ------------------------------
echo.
echo [2/2] Copying runtime core...
if not exist "%CORE%" goto :no_core
copy /Y "%CORE%" "%DEST%\tree-sitter.wasm" >nul
if errorlevel 1 goto :copy_failed
echo       OK -^> static\grove\tree-sitter.wasm

echo.
echo === Done. static\grove\ now contains: ===
dir /b "%DEST%\*.wasm"
echo.
echo Reload the grove window (Ctrl+R) to pick up the new wasm.
goto :end

:build_failed
echo.
echo ERROR: grammar wasm build failed (exit %RC%).
echo   - Is the tree-sitter CLI on PATH?         tree-sitter --version
echo   - Is Emscripten (emcc) on PATH OR is Docker Desktop running?
echo   - Does crates\grove\arbor-grove-lang\tree-sitter.json exist?
goto :end

:no_core
echo.
echo ERROR: runtime core not found:
echo   %CORE%
echo   Run "yarn install" first (it ships the web-tree-sitter runtime core).
goto :end

:copy_failed
echo.
echo ERROR: failed to copy the runtime core into static\grove\.
goto :end

:end
echo.
pause
endlocal
