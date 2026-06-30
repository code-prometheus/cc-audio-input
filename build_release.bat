@echo off
REM ============================================
REM audio-input release 打包脚本
REM ============================================
echo === audio-input Release Builder ===

set RELEASE_DIR=target\release
set PACKAGE_DIR=release\audio-input

REM 1. 编译
echo [1/5] Building release...
cargo build --release
if %ERRORLEVEL% NEQ 0 (
    echo ERROR: Build failed!
    exit /b 1
)

REM 2. 创建打包目录
echo [2/5] Creating package directory...
if exist "%PACKAGE_DIR%" rmdir /s /q "%PACKAGE_DIR%"
mkdir "%PACKAGE_DIR%"
mkdir "%PACKAGE_DIR%\models\sense-voice-int8"

REM 3. 复制文件
echo [3/5] Copying files...
copy "%RELEASE_DIR%\audio-input.exe" "%PACKAGE_DIR%\"
copy "%RELEASE_DIR%\sherpa-onnx-c-api.dll" "%PACKAGE_DIR%\"
copy "%RELEASE_DIR%\onnxruntime.dll" "%PACKAGE_DIR%\"
copy "%RELEASE_DIR%\onnxruntime_providers_shared.dll" "%PACKAGE_DIR%\"
copy "assets\hotwords.yaml" "%PACKAGE_DIR%\"

REM 4. 复制/提示模型
echo [4/5] Checking model...
if exist "F:\models\sense-voice-int8\model.int8.onnx" (
    copy "F:\models\sense-voice-int8\model.int8.onnx" "%PACKAGE_DIR%\models\sense-voice-int8\"
    copy "F:\models\sense-voice-int8\tokens.txt" "%PACKAGE_DIR%\models\sense-voice-int8\"
    echo   ^> Model copied from F:\models\sense-voice-int8\
) else (
    echo   ^> WARNING: Model not found! Download from:
    echo     https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2
    echo     Extract to F:\models\sense-voice-int8\ then re-run this script.
)

REM 5. 生成 setup 信息
echo [5/5] Package ready: %PACKAGE_DIR%
echo.
echo === Done! ===
echo Package: %cd%\%PACKAGE_DIR%
echo Run: %cd%\%PACKAGE_DIR%\audio-input.exe
dir "%PACKAGE_DIR%"
