# tagjoe unter Ubuntu installieren

Für Ubuntu 24.04 und 26.04 (amd64) gibt es ein Paket (`.deb`); dasselbe Paket passt auch für
[Debian 12 und 13](install-debian.md). Am einfachsten installiert das Skript das Paket aus dem
neuesten Release:

```bash
curl -fsSL https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh | bash
```

Das Skript prüft Ubuntu-Version und Architektur, lädt das Paket und die Datei `SHA256SUMS`,
prüft die Prüfsumme und installiert mit `sudo apt-get install`. Derselbe Befehl aktualisiert eine
vorhandene Installation.

**Erst lesen, dann ausführen** (empfohlen): Der Einzeiler oben führt das Skript aus, ohne dass du
es vorher siehst. Wer das nicht will, lädt es zuerst, liest es und startet es dann:

```bash
curl -fsSL -o install-tagjoe.sh https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh
less install-tagjoe.sh
bash install-tagjoe.sh
```

Von Hand: Auf der [Release-Seite](https://github.com/joeMJ/tagjoe/releases/latest) das Paket
`tagjoe_<Version>_amd64.deb` laden, die Prüfsumme mit `SHA256SUMS` vergleichen und installieren:

```bash
sha256sum -c --ignore-missing SHA256SUMS
sudo apt install ./tagjoe_<Version>_amd64.deb
```

tagjoe erscheint danach im Anwendungsmenü; im Terminal startet die Oberfläche mit `tagjoe-gui`,
die Kommandozeile mit `tagjoe`. Für akustische Fingerabdrücke (AcoustID) braucht es `fpcalc` aus
`libchromaprint-tools`, für die Systemdialoge `xdg-desktop-portal`; apt installiert beides als
Empfehlung mit. Fehlt `fpcalc` trotzdem, bietet **Einstellungen → Quellen → AcoustID** den Knopf
**fpcalc herunterladen** (Chromaprint-Release von GitHub, SHA-256 geprüft, nach
`~/.local/share/tagjoe/bin/`).

Nach dem ersten Start: **Einstellungen → Quellen und Schlüssel** – jede Online-Quelle ist aus,
bis du sie einschaltest ([Datenschutz](DATENSCHUTZ.md)).

Entfernen:

```bash
sudo apt remove tagjoe
```

Einstellungen, Journal und Protokoll bleiben dabei in `~/.config/tagjoe/`, der Zwischenspeicher
in `~/.cache/tagjoe/`.

## Updates

Unter **Einstellungen → Allgemein → Updates** lässt sich eine Prüfung auf neue Versionen
einschalten (standardmäßig aus, höchstens einmal täglich) oder mit **Jetzt prüfen** einmal von
Hand auslösen. Eine neue Version erscheint als Leiste unter der Kopfzeile. **Herunterladen und
prüfen** lädt das Paket und prüft es gegen `SHA256SUMS` desselben Releases; stimmt die Prüfsumme
nicht, wird nichts installiert. Danach zeigt tagjoe den Befehl `pkexec apt-get install -y …` und
installiert nach **Installieren** (das System fragt nach dem Kennwort); anschließend tagjoe neu
starten. Ebenso aktualisiert der Installationsbefehl oben.

## Aus dem Quelltext

Benötigt: `cargo` (Rust), `build-essential`, `pkg-config`, `libwayland-dev`, `libxkbcommon-dev`,
`libxkbcommon-x11-dev`, `libxcb1-dev`, `libxcb-xkb-dev`.

```bash
cargo build --release
./target/release/tagjoe --help
./target/release/tagjoe-gui
```

`tools/build-deb.sh` erzeugt das Paket (`lokal/release/`). Die Releases baut GitHub Actions in
einem Debian-12-Container (`.github/workflows/release.yml`).
