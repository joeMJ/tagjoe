<#
.SYNOPSIS
    Builds tagjoe for Windows: programs, staging folder with licence texts, signed MSI.

.DESCRIPTION
    After tvjoe/tools/build-windows.ps1, without the MSYS2/GTK parts (tagjoe needs no DLLs):
      1. cargo build --release with the MSVC toolchain; the program icon is embedded by build.rs
         (rc.exe from the Windows SDK). Build-machine paths are removed with --remap-path-prefix and
         checked afterwards.
      2. Staging folder lokal\windows\stage: bin\ (tagjoe.exe, tagjoe-gui.exe), LICENSE.txt,
         THIRD_PARTY.txt, THIRD_PARTY_LICENSES.txt.
      3. Signature of the programs with the documentjoe certificate (Cert:\CurrentUser\My).
      4. MSI with WiX 3.14 (packaging\windows\tagjoe.wxs), per user without administrator rights;
         MSI signed, SHA-256 checksum next to it (<msi>.sha256).
    Every error is reported with its cause and the next step. Result: lokal\windows\ (not versioned).

.PARAMETER WixDir
    Folder with candle.exe and light.exe (default: environment variable WIX or its bin, else
    C:\Program Files (x86)\WiX Toolset v3.14\bin).

.PARAMETER Thumbprint
    Thumbprint of the signing certificate in Cert:\CurrentUser\My (with private key).

.PARAMETER TimestampUrl
    Timestamp service (RFC 3161).

.PARAMETER NoSign
    Do not sign (trial runs; uncommitted changes are allowed then).

.PARAMETER NoMsi
    Only build the programs and the staging folder, no MSI.
#>
param(
    [string]$WixDir = '',
    [string]$Thumbprint = 'C595450070432A501AADD72366C837AFB268740A',
    [string]$TimestampUrl = 'http://timestamp.digicert.com',
    [switch]$NoSign,
    [switch]$NoMsi
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Fehler([string]$Text, [string]$Hilfe = '') {
    Write-Host "FEHLER: $Text" -ForegroundColor Red
    if ($Hilfe) { Write-Host "  Abhilfe: $Hilfe" -ForegroundColor Yellow }
    exit 1
}
function Schritt([string]$Text) { Write-Host "== $Text" -ForegroundColor Cyan }
function Ausfuehren([string]$Was, [scriptblock]$Block) {
    & $Block
    if ($LASTEXITCODE -ne 0) { Fehler "$Was ist fehlgeschlagen (Rückgabewert $LASTEXITCODE)." 'Meldungen oberhalb prüfen.' }
}

$Root = Split-Path -Parent $PSScriptRoot
$Out = Join-Path $Root 'lokal\windows'
$Stage = Join-Path $Out 'stage'
$Toolchain = 'stable-x86_64-pc-windows-msvc'

Schritt 'Version, Stand und Werkzeuge'
$versionLine = Select-String -Path (Join-Path $Root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
if (-not $versionLine) { Fehler 'Version in Cargo.toml nicht gefunden.' }
$Version = $versionLine.Matches[0].Groups[1].Value
if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fehler "Version „$Version“ passt nicht zu einer MSI (nur a.b.c)." }
$head = (git -C $Root rev-parse --short HEAD).Trim()
$geaendert = [bool](git -C $Root status --porcelain)
Write-Host "Version $Version, Stand $head$(if ($geaendert) { ' (mit nicht eingecheckten Änderungen)' })"
if (-not $NoSign -and $geaendert) {
    Fehler 'Nicht eingecheckte Änderungen; so wird nicht signiert.' 'Änderungen einchecken oder -NoSign für einen Probelauf.'
}
if (-not ((rustup toolchain list) -match [regex]::Escape($Toolchain))) {
    Fehler "Rust-Toolchain $Toolchain fehlt." "rustup toolchain install $Toolchain"
}

$signtool = $null
if (-not $NoSign) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1 -ExpandProperty FullName
    if (-not $signtool) { Fehler 'signtool.exe nicht gefunden.' 'Windows SDK installieren oder -NoSign.' }
    $cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object Thumbprint -eq $Thumbprint
    if (-not $cert) { Fehler "Zertifikat $Thumbprint fehlt in Cert:\CurrentUser\My." 'Zertifikat mit privatem Schlüssel einspielen oder -Thumbprint angeben.' }
    if (-not $cert.HasPrivateKey) { Fehler "Zum Zertifikat $Thumbprint fehlt der private Schlüssel." }
}

if (-not $NoMsi) {
    if (-not $WixDir) {
        $kandidaten = @()
        if ($env:WIX) { $kandidaten += @((Join-Path $env:WIX 'bin'), $env:WIX) }
        $kandidaten += @('C:\Program Files (x86)\WiX Toolset v3.14\bin')
        $WixDir = $kandidaten | Where-Object { Test-Path (Join-Path $_ 'candle.exe') } | Select-Object -First 1
        if (-not $WixDir) { $WixDir = $kandidaten[-1] }
    }
    $candle = Join-Path $WixDir 'candle.exe'
    $light = Join-Path $WixDir 'light.exe'
    if (-not (Test-Path $candle) -or -not (Test-Path $light)) {
        Fehler "WiX nicht gefunden unter $WixDir." 'wix314-binaries.zip (github.com/wixtoolset/wix3/releases) auspacken und -WixDir angeben, oder -NoMsi.'
    }
}

Schritt 'Programme bauen'
# No build-machine paths inside the programs (Rust writes source paths for error messages):
# user profile, cargo registry and project folder are renamed, each also in short (8.3) form.
$fso = New-Object -ComObject Scripting.FileSystemObject
function Formen([string]$Pfad) {
    if (-not $Pfad -or -not (Test-Path $Pfad)) { return @() }
    $voll = (Resolve-Path $Pfad).ProviderPath.TrimEnd('\')
    @($voll, $fso.GetFolder($voll).ShortPath.TrimEnd('\'), $Pfad.TrimEnd('\')) | Select-Object -Unique
}
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
$maps = @()
foreach ($p in Formen $env:USERPROFILE) { $maps += , @($p, '~') }
foreach ($p in Formen (Join-Path $cargoHome 'registry\src')) { $maps += , @($p, 'cargo-registry') }
foreach ($p in (@(Formen $Root) + @([IO.Directory]::GetCurrentDirectory()) | Select-Object -Unique)) { $maps += , @($p, 'tagjoe') }
$env:CARGO_ENCODED_RUSTFLAGS = @($maps | ForEach-Object { "--remap-path-prefix=$($_[0])=$($_[1])" }) -join [char]0x1f
Push-Location $Root
try {
    Ausfuehren 'cargo build' { cargo "+$Toolchain" build --release --locked }
} finally {
    Pop-Location
}
$bauOrdner = Join-Path $Root 'target\release'
$verboten = @($maps | ForEach-Object { $_[0] }) + @($env:USERNAME, $env:COMPUTERNAME) | Where-Object { $_ } | Select-Object -Unique
foreach ($exe in @('tagjoe.exe', 'tagjoe-gui.exe')) {
    $text = [Text.Encoding]::GetEncoding(28591).GetString([IO.File]::ReadAllBytes((Join-Path $bauOrdner $exe)))
    foreach ($v in $verboten) {
        if ($text.IndexOf($v, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
            Fehler "$exe enthält eine Angabe des Bau-Rechners („$v“)." 'Zuordnungen für --remap-path-prefix prüfen (cargo clean und neu bauen).'
        }
    }
}
Write-Host 'Keine Pfade, Benutzer- oder Rechnernamen des Bau-Rechners in den Programmen.'

Schritt 'Bereitstellungsordner'
if (Test-Path $Stage) {
    try { Remove-Item $Stage -Recurse -Force }
    catch { Fehler "Alter Bereitstellungsordner lässt sich nicht löschen: $($_.Exception.Message)" 'Laufendes tagjoe beenden und keine Konsole in lokal\windows\stage stehen lassen.' }
}
$bin = Join-Path $Stage 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null
foreach ($exe in @('tagjoe.exe', 'tagjoe-gui.exe')) {
    $p = Join-Path $bauOrdner $exe
    if (-not (Test-Path $p)) { Fehler "$exe fehlt nach dem Bau." }
    Copy-Item $p $bin
}
# Licence texts as .txt files next to the programs.
foreach ($pair in @(@('LICENSE', 'LICENSE.txt'), @('THIRD_PARTY.md', 'THIRD_PARTY.txt'), @('THIRD_PARTY_LICENSES.txt', 'THIRD_PARTY_LICENSES.txt'))) {
    $p = Join-Path $Root $pair[0]
    if (-not (Test-Path $p)) { Fehler "Lizenzdatei fehlt: $p" }
    Copy-Item $p (Join-Path $Stage $pair[1])
}
$quellen = @(
    "tagjoe $Version für Windows",
    '',
    'tagjoe.exe und tagjoe-gui.exe stehen unter der Apache-2.0-Lizenz (LICENSE.txt); Quelltext:',
    "  https://github.com/joeMJ/tagjoe/tree/v$Version",
    '',
    'Eingebaute Bibliotheken Dritter: THIRD_PARTY.txt (Liste) und THIRD_PARTY_LICENSES.txt (Lizenztexte).',
    'fpcalc (Chromaprint, LGPL-2.1) ist nicht enthalten; tagjoe lädt es auf Wunsch von',
    '  https://github.com/acoustid/chromaprint/releases (Einstellungen → Quellen → AcoustID).'
)
[IO.File]::WriteAllLines((Join-Path $Stage 'QUELLEN.txt'), $quellen, (New-Object Text.UTF8Encoding $true))

if (-not $NoSign) {
    Schritt 'Eigene Programme signieren'
    $eigene = Get-ChildItem $bin -Filter 'tagjoe*.exe'
    if ($eigene.Count -ne 2) { Fehler "Erwartet tagjoe.exe und tagjoe-gui.exe, gefunden: $($eigene.Name -join ', ')." }
    Ausfuehren 'signtool sign (Programme)' { & $signtool sign /sha1 $Thumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 /d 'tagjoe' $eigene.FullName }
    foreach ($e in $eigene) {
        Ausfuehren "Signatur prüfen ($($e.Name))" { & $signtool verify /pa /q $e.FullName }
    }
}

$groesse = (Get-ChildItem $Stage -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ("Paketordner: {0} ({1} Dateien, {2:N1} MB)" -f $Stage, (Get-ChildItem $Stage -Recurse -File).Count, ($groesse / 1MB))
if ($NoMsi) { Write-Host 'Fertig (ohne MSI).'; exit 0 }

Schritt 'Dateiliste für WiX'
# One component per folder with a key in HKCU (required per user) and RemoveFolder; fixed GUIDs from
# the relative path so updates recognise the same components.
$md5 = [Security.Cryptography.MD5]::Create()
function Guid-Aus([string]$Text) {
    $b = $md5.ComputeHash([Text.Encoding]::UTF8.GetBytes("tagjoe-msi:$Text"))
    $b[6] = ($b[6] -band 0x0f) -bor 0x30; $b[8] = ($b[8] -band 0x3f) -bor 0x80
    return ([guid]::new([byte[]]$b)).ToString().ToUpper()
}
function Id-Aus([string]$Prefix, [string]$Text) {
    $h = -join ($md5.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text))[0..7] | ForEach-Object { $_.ToString('x2') })
    return "${Prefix}_$h"
}
function Xml([string]$s) { [Security.SecurityElement]::Escape($s) }
$sb = New-Object Text.StringBuilder
[void]$sb.AppendLine('<?xml version="1.0" encoding="utf-8"?>')
[void]$sb.AppendLine('<!-- Generated by tools/build-windows.ps1, do not edit. -->')
[void]$sb.AppendLine('<Wix xmlns="http://schemas.microsoft.com/wix/2006/wi"><Fragment>')
$comps = New-Object Collections.Generic.List[string]
function Ordner([string]$Pfad, [string]$Rel) {
    $files = @(Get-ChildItem $Pfad -File | Sort-Object Name)
    $compId = Id-Aus 'c' "dir:$Rel"
    $comps.Add($compId)
    [void]$sb.AppendLine("<Component Id=`"$compId`" Guid=`"$(Guid-Aus "dir:$Rel")`" Win64=`"yes`">")
    [void]$sb.AppendLine("<RegistryValue Root=`"HKCU`" Key=`"Software\tagjoe\Installer\Ordner`" Name=`"$(Xml ($(if ($Rel) { $Rel } else { '.' })))`" Type=`"integer`" Value=`"1`" KeyPath=`"yes`" />")
    [void]$sb.AppendLine("<RemoveFolder Id=`"$(Id-Aus 'r' "dir:$Rel")`" On=`"uninstall`" />")
    foreach ($f in $files) {
        $relFile = if ($Rel) { "$Rel\$($f.Name)" } else { $f.Name }
        [void]$sb.AppendLine("<File Id=`"$(Id-Aus 'f' "file:$relFile")`" Name=`"$(Xml $f.Name)`" Source=`"$(Xml $f.FullName)`" KeyPath=`"no`" />")
    }
    [void]$sb.AppendLine('</Component>')
    foreach ($d in (Get-ChildItem $Pfad -Directory | Sort-Object Name)) {
        $relDir = if ($Rel) { "$Rel\$($d.Name)" } else { $d.Name }
        [void]$sb.AppendLine("<Directory Id=`"$(Id-Aus 'd' "dir:$relDir")`" Name=`"$(Xml $d.Name)`">")
        Ordner $d.FullName $relDir
        [void]$sb.AppendLine('</Directory>')
    }
}
[void]$sb.AppendLine('<DirectoryRef Id="INSTALLDIR">')
Ordner $Stage ''
[void]$sb.AppendLine('</DirectoryRef>')
[void]$sb.AppendLine('<ComponentGroup Id="AppFiles">')
foreach ($c in $comps) { [void]$sb.AppendLine("<ComponentRef Id=`"$c`" />") }
[void]$sb.AppendLine('</ComponentGroup>')
[void]$sb.AppendLine('</Fragment></Wix>')
$filesWxs = Join-Path $Out 'files.wxs'
[IO.File]::WriteAllText($filesWxs, $sb.ToString(), (New-Object Text.UTF8Encoding $false))
Write-Host "$($comps.Count) Ordner"

Schritt 'MSI bauen (candle, light)'
$wxs = Join-Path $Root 'packaging\windows\tagjoe.wxs'
$ico = Join-Path $Root 'packaging\windows\tagjoe.ico'
$obj = Join-Path $Out 'obj'
New-Item -ItemType Directory -Force $obj | Out-Null
$msiName = "tagjoe-$Version-windows-x86_64.msi"
$msi = Join-Path $Out $msiName
Ausfuehren 'candle' { & $candle -nologo -arch x64 -ext WixUtilExtension "-dVersion=$Version" "-dIconFile=$ico" -out "$obj\" $wxs $filesWxs }
# ICE91: files in the user profile for a per-user installation (intended).
# ICE61: the same version replaces itself (AllowSameVersionUpgrades; intended).
Ausfuehren 'light' { & $light -nologo -spdb -ext WixUtilExtension -sice:ICE91 -sice:ICE61 -out $msi "$obj\tagjoe.wixobj" "$obj\files.wixobj" }

if (-not $NoSign) {
    Schritt 'MSI signieren'
    Ausfuehren 'signtool sign (MSI)' { & $signtool sign /sha1 $Thumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 /d "tagjoe $Version" $msi }
}

Schritt 'Prüfsumme'
$hash = (Get-FileHash $msi -Algorithm SHA256).Hash.ToLower()
[IO.File]::WriteAllText("$msi.sha256", "$hash  $msiName`n", (New-Object Text.UTF8Encoding $false))
Write-Host ("Fertig: {0} ({1:N1} MB)" -f $msi, ((Get-Item $msi).Length / 1MB))
Write-Host "SHA-256: $hash"
