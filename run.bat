@echo off
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
set SONARPAD_ROUTE_CLIENT_TOKEN=8d3b0a3e96524765a1d5e91863b4c2736fc9c9a7e4f0526daa8dc576927cb019
cargo run
