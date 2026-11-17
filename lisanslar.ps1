# Varak'a derlenen Rust kutuphanelerinin lisans dosyalarini "cargo metadata" ile toplar.
# Kullanim: powershell -ExecutionPolicy Bypass -File lisanslar.ps1 <hedef klasor>
param([Parameter(Mandatory = $true)][string]$Hedef)
$ErrorActionPreference = 'Stop'

# ConvertFrom-Json, Windows PowerShell 5.1'de buyuk JSON'da sinira takilir; bu yuzden .NET cozucusu
Add-Type -AssemblyName System.Web.Extensions
$cozucu = New-Object System.Web.Script.Serialization.JavaScriptSerializer
$cozucu.MaxJsonLength = [int]::MaxValue
$json = cargo metadata --format-version 1 --filter-platform x86_64-pc-windows-msvc | Out-String
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata basarisiz' }
$veri = $cozucu.DeserializeObject($json)

New-Item -ItemType Directory -Force -Path $Hedef | Out-Null
$liste = @()
foreach ($p in ($veri['packages'] | Sort-Object { $_['name'] }, { $_['version'] })) {
    if ($null -eq $p['source']) { continue }  # Varak'in kendisi
    $dizin = Split-Path -Parent $p['manifest_path']
    $dosyalar = Get-ChildItem -LiteralPath $dizin -File | Where-Object { $_.Name -match '^(LICEN[CS]E|COPYING|NOTICE|UNLICENSE)' }
    if ($dosyalar) {
        $k = Join-Path $Hedef ($p['name'] + '-' + $p['version'])
        New-Item -ItemType Directory -Force -Path $k | Out-Null
        $dosyalar | Copy-Item -Destination $k
    }
    $liste += ($p['name'] + ' ' + $p['version'] + "`t" + $p['license'])
}
$liste | Set-Content -Encoding UTF8 -Path (Join-Path $Hedef 'KUTUPHANELER.txt')
Write-Output ("{0} kutuphanenin lisansi toplandi." -f $liste.Count)
