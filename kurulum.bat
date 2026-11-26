@echo off
cd /d "%~dp0"
rem GitHub Actions gibi otomatik derlemelerde (CI tanimli) pencere beklemez
set BEKLE=pause
if defined CI set BEKLE=echo.
set MUPDF=mupdf-1.28.0-windows
set TESS=tesseract-ocr-w64-setup-5.5.3.20260724.exe
set GS=gs10071w64.exe
set TESSDATA=https://github.com/tesseract-ocr/tessdata/raw/main
set GECICI=%TEMP%\varak_kurulum
mkdir "%GECICI%" 2>nul

if not exist araclar\mupdf\mutool.exe (
  echo MuPDF indiriliyor...
  curl -fL -o "%GECICI%\mupdf.zip" https://mupdf.com/downloads/archive/%MUPDF%.zip || goto hata
  tar -xf "%GECICI%\mupdf.zip" -C "%GECICI%" || goto hata
  mkdir araclar\mupdf 2>nul
  for /f "delims=" %%f in ('dir /s /b "%GECICI%\mutool.exe"') do copy /y "%%f" araclar\mupdf\ >nul
)
if not exist araclar\mupdf\mutool.exe goto hata

if not exist araclar\tesseract\tesseract.exe (
  echo Tesseract indiriliyor...
  curl -fL -o "%GECICI%\%TESS%" https://github.com/tesseract-ocr/tesseract/releases/download/5.5.3/%TESS% || goto hata
  echo Windows izin isterse onaylayin.
  start /wait "" "%GECICI%\%TESS%" /S /D=%CD%\araclar\tesseract
)
if not exist araclar\tesseract\tesseract.exe goto hata

for %%L in (eng osd tur deu ara) do (
  if not exist araclar\tesseract\tessdata\%%L.traineddata (
    echo %%L dil modeli indiriliyor...
    curl -fL -o araclar\tesseract\tessdata\%%L.traineddata %TESSDATA%/%%L.traineddata || goto hata
  )
)
rem Osmanlica profili: Arapca "best", Farsca "fast" model (olcumle secildi)
if not exist araclar\tesseract\tessdata\ara_best.traineddata (
  echo ara_best dil modeli indiriliyor...
  curl -fL -o araclar\tesseract\tessdata\ara_best.traineddata https://github.com/tesseract-ocr/tessdata_best/raw/main/ara.traineddata || goto hata
)
if not exist araclar\tesseract\tessdata\fas_fast.traineddata (
  echo fas_fast dil modeli indiriliyor...
  curl -fL -o araclar\tesseract\tessdata\fas_fast.traineddata https://github.com/tesseract-ocr/tessdata_fast/raw/main/fas.traineddata || goto hata
)

rem Ghostscript kurucusu sessiz modda da pencere actigindan, 7-Zip varsa kurulmadan acilir
set YEDIZIP=
for %%z in (7z.exe) do if not "%%~$PATH:z"=="" set YEDIZIP=%%~$PATH:z
if exist "%ProgramFiles%\7-Zip\7z.exe" set YEDIZIP=%ProgramFiles%\7-Zip\7z.exe
if not exist araclar\gs\bin\gswin64c.exe (
  echo Ghostscript indiriliyor...
  curl -fL -o "%GECICI%\%GS%" https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/download/gs10071/%GS% || goto hata
  if defined YEDIZIP (
    "%YEDIZIP%" x "%GECICI%\%GS%" -oaraclar\gs -y -x!$PLUGINSDIR -x!uninstgs.exe.nsis -x!vcredist_x64.exe >nul || goto hata
  ) else (
    echo Ghostscript kurulum penceresi acilacak: "Install" ile devam edin. Windows izin isterse onaylayin.
    start /wait "" "%GECICI%\%GS%" /S /D=%CD%\araclar\gs
  )
)
if not exist araclar\gs\bin\gswin64c.exe goto hata

rem Lisans metinleri: AGPL-3.0 (Varak, MuPDF, Ghostscript) ve Apache-2.0 (Tesseract)
rem AGPL metni depodaki LICENSE dosyasindan alinir; yoksa gnu.org'dan indirilir
if not exist LISANS.txt if exist LICENSE copy /y LICENSE LISANS.txt >nul
if not exist LISANS.txt (
  echo Lisans metinleri indiriliyor...
  curl -fL --retry 3 -o LISANS.txt https://www.gnu.org/licenses/agpl-3.0.txt || goto hata
)
mkdir lisanslar 2>nul
if not exist lisanslar\Tesseract-LICENSE.txt (
  curl -fL -o lisanslar\Tesseract-LICENSE.txt https://raw.githubusercontent.com/tesseract-ocr/tesseract/main/LICENSE || goto hata
)

rem Arayuz yazi tipleri (SIL Open Font License): uygulamanin icine gomulur, cevrim disi calisir
set FONTS=https://raw.githubusercontent.com/google/fonts/main/ofl
mkdir ui\fonts 2>nul
if not exist ui\fonts\Literata.ttf (
  echo Yazi tipleri indiriliyor...
  curl -fgL -o ui\fonts\Literata.ttf "%FONTS%/literata/Literata[opsz,wght].ttf" || goto hata
  curl -fgL -o ui\fonts\OFL-Literata.txt "%FONTS%/literata/OFL.txt" || goto hata
)
if not exist ui\fonts\SourceSans3.ttf (
  curl -fgL -o ui\fonts\SourceSans3.ttf "%FONTS%/sourcesans3/SourceSans3[wght].ttf" || goto hata
  curl -fgL -o ui\fonts\OFL-SourceSans3.txt "%FONTS%/sourcesans3/OFL.txt" || goto hata
)
if not exist ui\fonts\NotoNaskhArabic.ttf (
  curl -fgL -o ui\fonts\NotoNaskhArabic.ttf "%FONTS%/notonaskharabic/NotoNaskhArabic[wght].ttf" || goto hata
  curl -fgL -o ui\fonts\OFL-NotoNaskhArabic.txt "%FONTS%/notonaskharabic/OFL.txt" || goto hata
)

rmdir /s /q "%GECICI%" 2>nul
echo Kurulum tamam. Simdi derle.bat dosyasini calistirin.
%BEKLE%
exit /b 0

:hata
echo Bir hata olustu. Bu penceredeki mesajlari bana iletin.
%BEKLE%
exit /b 1
