# Install tagjoe on Ubuntu

For Ubuntu 24.04 and 26.04 (amd64) there is one package (`.deb`); the same package also fits
[Debian 12 and 13](install-debian.en.md). The easiest way: the script installs the package from
the newest release:

```bash
curl -fsSL https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh | bash
```

The script checks the Ubuntu version and the architecture, downloads the package and the file
`SHA256SUMS`, checks the checksum and installs with `sudo apt-get install`. The same command
updates an existing installation.

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

Under **Settings → General → Updates** you can switch on a check for new versions (off by
default, at most once a day) or run it once by hand with **Check now**. A new version shows up as
a bar under the header. **Download and verify** downloads the package and checks it against
`SHA256SUMS` of the same release; if the checksum does not match, nothing is installed. Then
tagjoe shows the command `pkexec apt-get install -y …` and installs after **Install** (the system
asks for the password); restart tagjoe afterwards. The installation command above updates as well.

## From source

Needs: `cargo` (Rust), `build-essential`, `pkg-config`, `libwayland-dev`, `libxkbcommon-dev`,
`libxkbcommon-x11-dev`, `libxcb1-dev`, `libxcb-xkb-dev`.

```bash
cargo build --release
./target/release/tagjoe --help
./target/release/tagjoe-gui
```

`tools/build-deb.sh` builds the package (`lokal/release/`). The releases are built by GitHub
Actions in a Debian 12 container (`.github/workflows/release.yml`).
