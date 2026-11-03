# Install tagjoe on Windows

**Requirements:** Windows 10 or 11 (64 bit), no administrator rights needed.

**Install:** download the file `tagjoe-<version>-windows-x86_64.msi` from the
[release page](https://github.com/joeMJ/tagjoe/releases/latest), compare the checksum with the
file `….msi.sha256` next to it and double-click the MSI. It installs for your own user account only
into `%LOCALAPPDATA%\Programs\tagjoe`, adds a shortcut to the start menu and shows up under
"Apps & features". The licence texts (`LICENSE.txt`, `THIRD_PARTY.txt`,
`THIRD_PARTY_LICENSES.txt`) are in the program folder.

The MSI and the programs are signed with the same self-issued certificate as tvjoe, audiojoe and
remotejoe. If you have installed it for one of them already, there is nothing more to do.

**After the first start:** **Settings → Sources and keys** – every online source is off until you
switch it on ([privacy](DATENSCHUTZ.en.md)). Acoustic fingerprints (AcoustID) need `fpcalc`
(Chromaprint); it is not included. Under **AcoustID** tagjoe shows "fpcalc missing" and the button
**Download fpcalc**: only when clicked, tagjoe downloads the Chromaprint release from GitHub,
checks its SHA-256 sum against the value compiled into the program and puts `fpcalc.exe` into
`%LOCALAPPDATA%\tagjoe\bin\`.

**Update:** the update check (**Settings → General → Updates**, off by default) reports a new
version, downloads the MSI if you want, verifies it against the `.sha256` file and starts
`msiexec /i … /passive`; tagjoe closes for that, the new MSI replaces the old one. By hand: install
the new MSI over the old one.

**Uninstall:** under "Apps & features" (or "Installed apps") at **tagjoe**. Settings, journal and
log (`%APPDATA%\tagjoe`) as well as cache and `fpcalc` (`%LOCALAPPDATA%\tagjoe`) are kept.

> [!WARNING]
> **Publisher certificate: optional and with a risk.**
> Without the certificate installed, Windows reports an "unknown publisher" during installation –
> tagjoe works the same anyway, you only confirm the warning by hand. Installing it puts a
> self-issued certificate for the **whole computer** into the trusted root certification
> authorities. Only install it if you trust the publisher; what that means is described under
> [Publisher certificate](#publisher-certificate). If in doubt, do not install it and compare the
> MSI's checksum instead.

> [!NOTE]
> **Warning "Windows protected your PC":** Microsoft Defender SmartScreen shows this message for
> files that have rarely been downloaded so far – even when they are signed. How to continue:
>
> 1. Click **More info**.
> 2. Check that **Publisher** says: `documentjoe, documentjoe Code Signing (privat)`.
> 3. Only then choose **Run anyway**.
>
> If it says "Unknown publisher", either the [certificate](#publisher-certificate) is not installed
> or the file was changed. Then, before running it, compare the file's checksum with the value on
> the release page (PowerShell in the download folder):
>
> ```powershell
> Get-FileHash .\<file name> -Algorithm SHA256
> ```

## Publisher certificate

The Windows packages are signed with a **self-issued** certificate. So that Windows does not report
an "unknown publisher" during installation, you can install the public part of this certificate as
trusted once.

- File: [`docs/zertifikat/documentjoe-codesigning.cer`](zertifikat/documentjoe-codesigning.cer)
- Name: `CN=documentjoe Code Signing (privat), O=documentjoe`, valid until 07.10.2041
- Use: **code signing only** (key usage "Digital Signature", extended key usage "Code Signing").
  It is no certification authority and cannot issue other certificates; it is no use for websites
  (TLS) or e-mail.
- Thumbprint (SHA-1): `C595450070432A501AADD72366C837AFB268740A`
- The same certificate also signs tvjoe, audiojoe and remotejoe.

**What installing it means:** the script puts the certificate into the stores
**"Trusted Root Certification Authorities"** and **"Trusted Publishers"** of the computer, that is
for all users. From then on Windows considers **every** program trustworthy that is signed with the
matching private key – not only tagjoe. As long as only the publisher has this key, that is
intended; if it got lost, malware could also appear "trusted" with it. The restriction to code
signing (see above) prevents misuse for websites or e-mail, not for programs. Therefore:

- Only install it if you trust the publisher and use tagjoe regularly.
- Without it everything stays usable; you then check that the MSI is genuine through the checksum
  on the release page (`Get-FileHash`, see above).
- When the certificate is no longer needed, remove it with the command under **Remove again**
  (below).

**Install** – open PowerShell **as administrator**, copy the whole block and paste it (Windows 10
and 11, also in constrained language mode). The script downloads the file, checks the checksum
(SHA-256) and the thumbprint and installs the certificate only if both match. If it is installed
already (e.g. for tvjoe, audiojoe or remotejoe), it does nothing.

```powershell
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference    = 'SilentlyContinue'
    $url    = 'https://raw.githubusercontent.com/joeMJ/tagjoe/main/docs/zertifikat/documentjoe-codesigning.cer'
    $thumb  = 'C595450070432A501AADD72366C837AFB268740A'
    $sha256 = '6F672EFA23A39A4C26693BA4F979A4368E657400BCB9EA8E05214E29C80E5DAE'
    $stores = [ordered]@{ 'Root' = 'Trusted Root Certification Authorities'; 'TrustedPublisher' = 'Trusted Publishers' }
    $file   = Join-Path $env:TEMP 'documentjoe-codesigning.cer'
    function Step($t) { Write-Host "  ..   $t" -ForegroundColor Gray }
    function Ok($t)   { Write-Host "  OK   $t" -ForegroundColor Green }
    function Fail($t) { Write-Host "  ERROR  $t" -ForegroundColor Red }

    Write-Host ''
    Write-Host "  tagjoe - install the publisher's certificate" -ForegroundColor Cyan
    Write-Host '  ==============================================' -ForegroundColor Cyan
    Write-Host ''

    if (-not (whoami /groups | Select-String -SimpleMatch 'S-1-16-12288' -Quiet)) {
        Fail 'This PowerShell is not running as administrator.'
        Write-Host '         Please open PowerShell with "Run as administrator" and paste again.' -ForegroundColor Yellow
        Write-Host ''
        return
    }

    $missing = @($stores.Keys | Where-Object { -not (Test-Path "Cert:\LocalMachine\$_\$thumb") })
    if ($missing.Count -eq 0) {
        Ok 'The certificate is installed already. There is nothing to do.'
        Write-Host ''
        return
    }

    try {
        Step 'Downloading the certificate from GitHub'
        try {
            Invoke-WebRequest -Uri $url -OutFile $file -UseBasicParsing
        } catch {
            & curl.exe -fsSL -o $file $url
            if ($LASTEXITCODE -ne 0) { throw 'The certificate could not be downloaded. Please check the internet connection.' }
        }
        Ok 'Downloaded'

        Step 'Checking that it is genuine'
        if ((Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash -ne $sha256) {
            throw 'The checksum does not match. The certificate is NOT installed.'
        }
        $cert = Get-PfxCertificate -FilePath $file
        if (-not $cert -or $cert.Thumbprint -ne $thumb) {
            throw 'The thumbprint does not match. The certificate is NOT installed.'
        }
        Ok "Genuine: $($cert.Subject)"
        Write-Host "         Thumbprint $thumb, valid until $($cert.NotAfter.ToString('dd.MM.yyyy'))" -ForegroundColor Gray

        foreach ($s in $missing) {
            Step "Installing into: $($stores[$s])"
            Import-Certificate -FilePath $file -CertStoreLocation "Cert:\LocalMachine\$s" | Out-Null
            Ok 'Installed'
        }
        Write-Host ''
        Write-Host '  Done. Programs and packages of tagjoe (and tvjoe, audiojoe, remotejoe) are now recognised as trusted.' -ForegroundColor Green
    } catch {
        Fail $_.Exception.Message
    } finally {
        if (Test-Path -LiteralPath $file) { Remove-Item -LiteralPath $file -Force }
    }
    Write-Host ''
}
```

Installing it is **optional**. Because the certificate is self-issued, Windows needs it in two
places: under "Trusted Root Certification Authorities", so that the signature counts as valid at
all, and under "Trusted Publishers", so that Windows knows the publisher. After that Windows trusts
everything signed with this key – as a program signature only, not for websites or e-mail. The
remaining risk: whoever had the private key could sign programs this computer then trusts. If you
do not want that, leave it out and confirm the warning during installation by hand. The SmartScreen
warning about rarely downloaded files can appear anyway; it does not depend on the certificate.

**Remove again** – also paste into a PowerShell as administrator. This removes the certificate for
all programs signed with it, so also for tvjoe, audiojoe and remotejoe:

```powershell
& {
    $thumb  = 'C595450070432A501AADD72366C837AFB268740A'
    $stores = [ordered]@{ 'Root' = 'Trusted Root Certification Authorities'; 'TrustedPublisher' = 'Trusted Publishers' }

    Write-Host ''
    Write-Host "  tagjoe - remove the publisher's certificate" -ForegroundColor Cyan
    Write-Host '  =============================================' -ForegroundColor Cyan
    Write-Host ''

    if (-not (whoami /groups | Select-String -SimpleMatch 'S-1-16-12288' -Quiet)) {
        Write-Host '  ERROR  This PowerShell is not running as administrator.' -ForegroundColor Red
        Write-Host '         Please open PowerShell with "Run as administrator" and paste again.' -ForegroundColor Yellow
        Write-Host ''
        return
    }

    foreach ($s in $stores.Keys) {
        $path = "Cert:\LocalMachine\$s\$thumb"
        if (Test-Path $path) {
            try {
                Remove-Item $path -ErrorAction Stop
                Write-Host "  OK   Removed from: $($stores[$s])" -ForegroundColor Green
            } catch {
                Write-Host "  ERROR  $($stores[$s]): $($_.Exception.Message)" -ForegroundColor Red
            }
        } else {
            Write-Host "  --   Not present in: $($stores[$s])" -ForegroundColor Gray
        }
    }
    Write-Host ''
}
```

## From source

On Windows with `tools\build-windows.ps1` (Rust with the MSVC toolchain, WiX 3.14): the script
builds the programs, removes build-machine paths, signs them if wanted and makes the MSI in
`lokal\windows\`. Needs: Rust (`rustup`), Visual Studio Build Tools with the Windows SDK
(`rc.exe`, `signtool.exe`), WiX 3.14 (`-WixDir`).
