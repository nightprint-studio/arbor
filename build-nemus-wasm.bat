@echo off
setlocal

rem ===========================================================================
rem  build-nemus-wasm.bat
rem  Compiles the nemus Tree-sitter grammar to WebAssembly and places the two
rem  .wasm files the editor loads into static\nemus\ (create or overwrite):
rem    - tree-sitter-nemus.wasm  (the nemus grammar, built from the crate)
rem    - tree-sitter.wasm        (the web-tree-sitter runtime core, copied)
rem
rem  Prerequisites:
rem    - tree-sitter CLI on PATH                 (tree-sitter --version)
rem    - Emscripten (emcc) on PATH OR Docker Desktop running
rem    - "yarn install" already run              (provides the runtime core)
rem  Re-run after changing grammar.js / scanner.c (after tree-sitter generate).
rem ===========================================================================

set "ROOT=%~dp0"
set "CRATE=%ROOT%crates\nemus\arbor-nemus-lang"
set "DEST=%ROOT%static\nemus"
set "CORE=%ROOT%node_modules\web-tree-sitter\tree-sitter.wasm"
set "CORE_MAP=%ROOT%node_modules\web-tree-sitter\tree-sitter.wasm.map"

echo.
echo === nemus wasm build ===
echo Root: %ROOT%

if not exist "%DEST%" mkdir "%DEST%"

rem --- 1) compile the grammar -> tree-sitter-nemus.wasm -----------------------
echo.
echo [1/2] Building grammar wasm (Docker/Emscripten)...
pushd "%CRATE%"
tree-sitter build --wasm
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :build_failed
if not exist "%CRATE%\tree-sitter-nemus.wasm" goto :build_failed
move /Y "%CRATE%\tree-sitter-nemus.wasm" "%DEST%\tree-sitter-nemus.wasm" >nul
if errorlevel 1 goto :build_failed
echo       OK -^> static\nemus\tree-sitter-nemus.wasm

rem --- 2) copy the web-tree-sitter runtime core ------------------------------
echo.
echo [2/2] Copying runtime core...
if not exist "%CORE%" goto :no_core
copy /Y "%CORE%" "%DEST%\tree-sitter.wasm" >nul
if errorlevel 1 goto :copy_failed
echo       OK -^> static\nemus\tree-sitter.wasm

rem  Also copy the runtime sourcemap so devtools stop 404ing on
rem  /nemus/tree-sitter.wasm.map (the wasm embeds a sourceMappingURL).
rem  Optional: a missing map only costs a harmless devtools 404, never fail here.
if exist "%CORE_MAP%" (
  copy /Y "%CORE_MAP%" "%DEST%\tree-sitter.wasm.map" >nul
  echo       OK -^> static\nemus\tree-sitter.wasm.map
) else (
  echo       (no tree-sitter.wasm.map in node_modules - skipping^)
)

echo.
echo === Done. static\nemus\ now contains: ===
dir /b "%DEST%\*.wasm"
echo.
echo Reload the nemus window (Ctrl+R) to pick up the new wasm.
goto :end

:build_failed
echo.
echo ERROR: grammar wasm build failed (exit %RC%).
echo   - Is the tree-sitter CLI on PATH?         tree-sitter --version
echo   - Is Emscripten (emcc) on PATH OR is Docker Desktop running?
echo   - Does crates\nemus\arbor-nemus-lang\tree-sitter.json exist?
goto :end

:no_core
echo.
echo ERROR: runtime core not found:
echo   %CORE%
echo   Run "yarn install" first (it ships the web-tree-sitter runtime core).
goto :end

:copy_failed
echo.
echo ERROR: failed to copy the runtime core into static\nemus\.
goto :end

:end
echo.
pause
endlocal
