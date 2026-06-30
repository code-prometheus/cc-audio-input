@echo off
REM ============================================
REM 下载 SenseVoice int8 模型到 F:\models
REM ============================================
echo === SenseVoice Model Downloader ===
echo.
echo Target: F:\models\sense-voice-int8\
echo Size: ~117MB (compressed) / ~228MB (extracted)
echo Source: GitHub Releases
echo.

set MODEL_DIR=F:\models\sense-voice-int8
set URL=https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2

if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"

echo [1/3] Downloading model (~117MB)...
echo     This may take a few minutes depending on your network.
echo.

:: Try with proxy first
set PROXY=http://localhost:60130
curl -sk --proxy %PROXY% -L -o "%MODEL_DIR%\model.tar.bz2" "%URL%" --connect-timeout 30 --max-time 1800 --retry 5 --retry-delay 15
if %ERRORLEVEL% EQU 0 goto :extract

:: Fallback: try without proxy
echo [1/3 retry] Trying direct download...
curl -sk -L -o "%MODEL_DIR%\model.tar.bz2" "%URL%" --connect-timeout 30 --max-time 1800 --retry 5 --retry-delay 15
if %ERRORLEVEL% EQU 0 goto :extract

echo ERROR: Download failed! Please download manually:
echo   %URL%
echo   Extract to: %MODEL_DIR%
pause
exit /b 1

:extract
echo [2/3] Extracting...
tar -xjf "%MODEL_DIR%\model.tar.bz2" -C "%MODEL_DIR%"
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Extraction failed. File may be corrupted.
    del "%MODEL_DIR%\model.tar.bz2"
    pause
    exit /b 1
)

echo [3/3] Cleaning up...
del "%MODEL_DIR%\model.tar.bz2"

echo.
echo === Done! ===
echo Model files:
dir "%MODEL_DIR%\*.onnx" "%MODEL_DIR%\*.txt"
echo.
echo You can now run audio-input.exe
pause
