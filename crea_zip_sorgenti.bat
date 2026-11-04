@echo off
setlocal EnableExtensions

rem Crea uno ZIP modificabile del progetto, senza repository Git e file compilati.
set "PROJECT_DIR=%~dp0"
set "PROJECT_NAME=Sonarpad minimal"
set "STAGE_DIR=%TEMP%\sonarpad_sources_%RANDOM%_%RANDOM%"
set "ZIP_FILE=%PROJECT_DIR%sonarpad_minimal_source.zip"

echo Preparazione dei sorgenti...
mkdir "%STAGE_DIR%\%PROJECT_NAME%" >nul 2>&1
if errorlevel 1 goto :error

rem Copia tutto, comprese cartelle come src, i18n, scripts e .github.
rem Esclude Git, gli artefatti Cargo e lo ZIP prodotto da questo script.
robocopy "%PROJECT_DIR%." "%STAGE_DIR%\%PROJECT_NAME%" /E /R:1 /W:1 /NFL /NDL /NJH /NJS /NP ^
  /XD "%PROJECT_DIR%.git" "%PROJECT_DIR%target" ^
  /XF "sonarpad_minimal_source.zip"

if errorlevel 8 goto :error

rem Elimina il vecchio archivio, se esiste.
if exist "%ZIP_FILE%" del /F /Q "%ZIP_FILE%"
if exist "%ZIP_FILE%" goto :error

echo Creazione di:
echo "%ZIP_FILE%"
powershell -NoProfile -ExecutionPolicy Bypass -Command ^
  "$ErrorActionPreference = 'Stop'; Compress-Archive -LiteralPath '%STAGE_DIR%\%PROJECT_NAME%' -DestinationPath '%ZIP_FILE%' -CompressionLevel Optimal -Force"
if errorlevel 1 goto :error

rmdir /S /Q "%STAGE_DIR%"
echo.
echo ZIP creato correttamente.
pause
exit /b 0

:error
echo.
echo ERRORE: non e' stato possibile creare lo ZIP.
if exist "%STAGE_DIR%" rmdir /S /Q "%STAGE_DIR%"
pause
exit /b 1