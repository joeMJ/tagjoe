# Install tagjoe on Debian

For Debian 12 and 13 (amd64) the same package (`.deb`) fits as for [Ubuntu](install-ubuntu.en.md).
If `curl` is missing, first `sudo apt install curl` (or as `root` without `sudo`). The easiest
way: the script installs the package from the newest release:

```bash
curl -fsSL https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh | bash
```

The script checks the Debian version and the architecture, downloads the package and the file
`SHA256SUMS`, checks the checksum and installs with `apt-get install` – as a normal user through
`sudo`, as `root` directly (Debian without `sudo`: run it in a root shell, e.g. after `su -`). The
same command updates an existing installation.

**Read first, then run** (recommended): the one-liner above runs the script without you seeing it
first. If you do not want that, download it first, read it and then start it:

```bash
curl -fsSL -o install-tagjoe.sh https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh
less install-tagjoe.sh
bash install-tagjoe.sh
```

By hand: download the package `tagjoe_<version>_amd64.deb` from the
[release page](https://github.com/joeMJ/tagjoe/releases/latest), compare the checksum with
`SHA256SUMS` and install it:

```bash
sha256sum -c --ignore-missing SHA256SUMS
sudo apt install ./tagjoe_<version>_amd64.deb
```

tagjoe then shows up in the application menu; in a terminal the window starts with `tagjoe-gui`,
the command line with `tagjoe`. Acoustic fingerprints (AcoustID) need `fpcalc` from
`libchromaprint-tools`, the system dialogs need `xdg-desktop-portal`; apt installs both as
recommendations.

After the first start: **Settings → Sources and keys** – every online source is off until you
switch it on ([privacy](DATENSCHUTZ.en.md)).

Remove:

```bash
sudo apt remove tagjoe
```

Settings, journal and log stay in `~/.config/tagjoe/`, the cache in `~/.cache/tagjoe/`.

## Updates

As on [Ubuntu](install-ubuntu.en.md#updates): **Settings → General → Updates**, check off by
default, the package is verified against `SHA256SUMS` and installed with
`pkexec apt-get install -y …`. The installation command above updates as well.

## From source

As on [Ubuntu](install-ubuntu.en.md#from-source).
