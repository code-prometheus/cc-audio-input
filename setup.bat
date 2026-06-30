@echo off
setlocal enabledelayedexpansion
title audio-input Setup

echo =============================================
echo    audio-input 安装器
echo    语音+LLM CLI输入工具
echo =============================================
echo.

REM ── 检查管理员权限（可选） ──
net session >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo [提示] 建议以管理员身份运行，否则托盘可能受限
    echo.
)

set INSTALL_DIR=%ProgramFiles%\audio-input
set MODEL_DIR=F:\models\sense-voice-int8

echo 安装目录: %INSTALL_DIR%
echo 模型目录: %MODEL_DIR%
echo.

REM ── 1. 复制程序文件 ──
echo [1/4] 安装程序文件...
if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"

copy /Y "target\release\audio-input.exe" "%INSTALL_DIR%\" >nul
copy /Y "target\release\sherpa-onnx-c-api.dll" "%INSTALL_DIR%\" >nul
copy /Y "target\release\onnxruntime.dll" "%INSTALL_DIR%\" >nul
copy /Y "target\release\onnxruntime_providers_shared.dll" "%INSTALL_DIR%\" >nul
copy /Y "assets\hotwords.yaml" "%INSTALL_DIR%\" >nul
copy /Y "sherpa_dll\sherpa-onnx-v1.13.3-win-x64-shared-MD-Release\bin\sherpa-onnx-offline.exe" "%INSTALL_DIR%\sherpa-onnx-offline.exe" >nul

echo    ^> audio-input.exe
echo    ^> sherpa-onnx-c-api.dll
echo    ^> onnxruntime.dll
echo    ^> sherpa-onnx-offline.exe
echo    ^> hotwords.yaml

REM ── 2. 复制模型 ──
echo [2/4] 安装模型文件...
if exist "%MODEL_DIR%\model.int8.onnx" (
    echo    ^> 模型已存在，跳过复制
) else (
    if not exist "%MODEL_DIR%" mkdir "%MODEL_DIR%"
    if exist "..\..\models\sense-voice-int8\model.int8.onnx" (
        copy /Y "..\..\models\sense-voice-int8\model.int8.onnx" "%MODEL_DIR%\" >nul
        copy /Y "..\..\models\sense-voice-int8\tokens.txt" "%MODEL_DIR%\" >nul
        echo    ^> 模型已从项目目录复制
    ) else (
        echo    ^> 模型未找到！请运行 download_model.bat 下载
    )
)

REM ── 3. 创建快捷方式 ──
echo [3/4] 创建快捷方式...
powershell -Command "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('%USERPROFILE%\Desktop\audio-input.lnk'); $s.TargetPath = '%INSTALL_DIR%\audio-input.exe'; $s.WorkingDirectory = '%INSTALL_DIR%'; $s.Description = '语音+LLM CLI输入工具'; $s.Save()"
echo    ^> 桌面快捷方式已创建

REM ── 4. 运行程序 ──
echo [4/4] 安装完成！
echo.
echo =============================================
echo   安装完成！
echo   程序: %INSTALL_DIR%\audio-input.exe
echo   桌面快捷方式: audio-input
echo.
echo   使用方法:
echo     按住鼠标左键 3 秒 → 说话 → 松开 → 自动粘贴到 CLI
echo     右下角托盘右键可拷贝结果/退出
echo.
echo   切换输入设备 (环境变量):
echo     set AUDIO_INPUT_DEVICE_ID=N
echo =============================================
echo.
choice /C YN /M "是否现在启动?"
if errorlevel 2 goto :end
if errorlevel 1 start "" "%INSTALL_DIR%\audio-input.exe"

:end
echo.
pause
