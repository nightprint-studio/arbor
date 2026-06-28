@echo off
setlocal

rem ===========================================================================
rem  build-merula-wasm.bat
rem  Compiles the merula Tree-sitter grammar to WebAssembly and places the two
rem  .wasm files the editor loads into static\merula\ (create or overwrite):
rem    - tree-sitter-merula.wasm  (the merula grammar, built from the crate)
rem    - tree-sitter.wasm        (the web-tree-sitter runtime core, copied)
rem
rem  Prerequisites:
rem    - tree-sitter CLI on PATH                 (tree-sitter --version)
rem    - Emscripten (emcc) on PATH OR Docker Desktop running
rem    - "yarn install" already run              (provides the runtime core)
rem  Re-run after changing grammar.js / scanner.c — it regenerates the parser
rem  (tree-sitter generate) and then rebuilds the wasm.
rem ===========================================================================

set "ROOT=%~dp0"
set "CRATE=%ROOT%crates\merula\merula-lang"
set "DEST=%ROOT%static\merula"
set "CORE=%ROOT%node_modules\web-tree-sitter\tree-sitter.wasm"
set "CORE_MAP=%ROOT%node_modules\web-tree-sitter\tree-sitter.wasm.map"

echo.
echo === merula wasm build ===
echo Root: %ROOT%

if not exist "%DEST%" mkdir "%DEST%"

rem --- 1) regenerate the parser from grammar.js ------------------------------
rem  Picks up grammar.js / scanner.c changes (parser.c + the external-scanner
rem  wiring) so the wasm + the Rust build compile the current grammar.
echo.
echo [1/3] Generating parser (tree-sitter generate)...
pushd "%CRATE%"
tree-sitter generate
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :generate_failed

rem --- 2) compile the grammar -> tree-sitter-merula.wasm -----------------------
echo.
echo [2/3] Building grammar wasm (Docker/Emscripten)...
pushd "%CRATE%"
tree-sitter build --wasm
set "RC=%ERRORLEVEL%"
popd
if not "%RC%"=="0" goto :build_failed
if not exist "%CRATE%\tree-sitter-merula.wasm" goto :build_failed
move /Y "%CRATE%\tree-sitter-merula.wasm" "%DEST%\tree-sitter-merula.wasm" >nul
if errorlevel 1 goto :build_failed
echo       OK -^> static\merula\tree-sitter-merula.wasm

rem --- 3) copy the web-tree-sitter runtime core ------------------------------
echo.
echo [3/3] Copying runtime core...
if not exist "%CORE%" goto :no_core
copy /Y "%CORE%" "%DEST%\tree-sitter.wasm" >nul
if errorlevel 1 goto :copy_failed
echo       OK -^> static\merula\tree-sitter.wasm

rem  Also copy the runtime sourcemap so devtools stop 404ing on
rem  /merula/tree-sitter.wasm.map (the wasm embeds a sourceMappingURL).
rem  Optional: a missing map only costs a harmless devtools 404, never fail here.
if exist "%CORE_MAP%" (
  copy /Y "%CORE_MAP%" "%DEST%\tree-sitter.wasm.map" >nul
  echo       OK -^> static\merula\tree-sitter.wasm.map
) else (
  echo       (no tree-sitter.wasm.map in node_modules - skipping^)
)

echo.
echo === Done. static\merula\ now contains: ===
dir /b "%DEST%\*.wasm"
echo.
echo Reload the merula window (Ctrl+R) to pick up the new wasm.
goto :end

:generate_failed
echo.
echo ERROR: tree-sitter generate failed (exit %RC%).
echo   - Is the tree-sitter CLI on PATH?         tree-sitter --version
echo   - Does crates\merula\merula-lang\grammar.js parse?
echo   - Does crates\merula\merula-lang\tree-sitter.json exist?
goto :end

:build_failed
echo.
echo ERROR: grammar wasm build failed (exit %RC%).
echo   - Is the tree-sitter CLI on PATH?         tree-sitter --version
echo   - Is Emscripten (emcc) on PATH OR is Docker Desktop running?
echo   - Does crates\merula\merula-lang\tree-sitter.json exist?
goto :end

:no_core
echo.
echo ERROR: runtime core not found:
echo   %CORE%
echo   Run "yarn install" first (it ships the web-tree-sitter runtime core).
goto :end

:copy_failed
echo.
echo ERROR: failed to copy the runtime core into static\merula\.
goto :end

:end
echo.
pause
endlocal
