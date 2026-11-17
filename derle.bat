@echo off
cd /d "%~dp0"
rem GitHub Actions gibi otomatik derlemelerde (CI tanimli) pencere beklemez
set BEKLE=pause
if defined CI set BEKLE=echo.
where cargo >nul 2>&1 || (echo Rust bulunamadi: https://rustup.rs & %BEKLE% & exit /b 1)
if not exist araclar\gs\bin\gswin64c.exe (echo Once kurulum.bat dosyasini calistirin. & %BEKLE% & exit /b 1)
if not exist LISANS.txt (echo Once kurulum.bat dosyasini calistirin. & %BEKLE% & exit /b 1)
echo Derleniyor. Ilk derleme 5-15 dakika surebilir...
cargo build --release > derleme_log.txt 2>&1 || (echo Derleme basarisiz. derleme_log.txt dosyasini bana gonderin. & %BEKLE% & exit /b 1)
for /f "tokens=3" %%v in ('findstr /b /c:"version = " Cargo.toml') do set SURUM=%%~v
set HEDEF=dagitim\Varak

rem Eski surumlerin dagitim\ altina birakti dosyalar (yeni yapi: dagitim\Varak\)
del /q dagitim\varak.exe 2>nul
rmdir /s /q dagitim\araclar 2>nul

mkdir %HEDEF% 2>nul
xcopy /e /i /y /q /d araclar %HEDEF%\araclar >nul
copy /y target\release\varak.exe %HEDEF%\Varak.exe >nul
for %%f in (LISANS.txt BENIOKU.txt UCUNCU_TARAF.txt) do copy /y %%f %HEDEF%\ >nul

rem Microsoft Visual C++ calisma zamani: Varak.exe, mutool ve Ghostscript buna bagli.
rem Yuklu olmayan bilgisayarlarda da calissin diye her birinin yanina konur.
for %%f in (vcruntime140.dll vcruntime140_1.dll msvcp140.dll) do (
  if not exist "%SystemRoot%\System32\%%f" (echo %%f bulunamadi: https://aka.ms/vs/17/release/vc_redist.x64.exe adresinden Visual C++ calisma zamanini kurun. & %BEKLE% & exit /b 1)
  copy /y "%SystemRoot%\System32\%%f" %HEDEF%\ >nul
  copy /y "%SystemRoot%\System32\%%f" %HEDEF%\araclar\mupdf\ >nul
  copy /y "%SystemRoot%\System32\%%f" %HEDEF%\araclar\gs\bin\ >nul
)

echo Lisans dosyalari toplaniyor...
rmdir /s /q %HEDEF%\lisanslar 2>nul
powershell -NoProfile -ExecutionPolicy Bypass -File lisanslar.ps1 %HEDEF%\lisanslar\rust >> derleme_log.txt 2>&1 || (echo Lisans dosyalari toplanamadi. derleme_log.txt dosyasini bana gonderin. & %BEKLE% & exit /b 1)
xcopy /i /y /q ui\fonts\OFL-*.txt %HEDEF%\lisanslar\yazi-tipleri >nul
xcopy /i /y /q lisanslar\* %HEDEF%\lisanslar >nul

rem Kaynak kodu (AGPL geregi programla birlikte verilir)
rmdir /s /q %HEDEF%\kaynak 2>nul
for %%d in (src ui capabilities icons lisanslar) do xcopy /e /i /y /q %%d %HEDEF%\kaynak\%%d >nul
for %%f in (Cargo.toml Cargo.lock build.rs tauri.conf.json kurulum.bat derle.bat lisanslar.ps1 LISANS.txt BENIOKU.txt UCUNCU_TARAF.txt) do copy /y %%f %HEDEF%\kaynak\ >nul

echo ZIP hazirlaniyor...
del /q dagitim\Varak-*.zip 2>nul
tar -a -c -f dagitim\Varak-%SURUM%.zip -C dagitim Varak || (echo ZIP olusturulamadi. & %BEKLE% & exit /b 1)
echo.
echo Hazir: %HEDEF%\Varak.exe
echo Dagitim paketi: dagitim\Varak-%SURUM%.zip
%BEKLE%
