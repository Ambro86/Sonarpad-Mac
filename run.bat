@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if "%SONARPAD_ROUTE_CLIENT_TOKEN%"=="" echo SONARPAD_ROUTE_CLIENT_TOKEN non impostato: alcune funzioni online potrebbero non funzionare.
if "%SONARPAD_TV_CLIENT_TOKEN%"=="" echo SONARPAD_TV_CLIENT_TOKEN non impostato: il catalogo TV remoto potrebbe non funzionare.
cargo run
