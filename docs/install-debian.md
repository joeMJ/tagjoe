# tagjoe unter Debian installieren

Für Debian 12 und 13 (amd64) passt dasselbe Paket (`.deb`) wie für [Ubuntu](install-ubuntu.md).
Fehlt `curl`, zuerst `sudo apt install curl` (bzw. als `root` ohne `sudo`). Am einfachsten
installiert das Skript das Paket aus dem neuesten Release:

```bash
curl -fsSL https://raw.githubusercontent.com/joeMJ/tagjoe/main/install.sh | bash
```

Das Skript prüft Debian-Version und Architektur, lädt das Paket und die Datei `SHA256SUMS`,
prüft die Prüfsumme und installiert mit `apt-get install` – als normaler Benutzer über `sudo`, als
`root` direkt (Debian ohne `sudo`: in einer Root-Shell, z. B. nach `su -`, ausführen). Derselbe
Befehl aktualisiert eine vorhandene Installation.

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

Wie unter [Ubuntu](install-ubuntu.md#updates): **Einstellungen → Allgemein → Updates**, Prüfung
standardmäßig aus, Paket wird gegen `SHA256SUMS` geprüft und mit `pkexec apt-get install -y …`
installiert. Ebenso aktualisiert der Installationsbefehl oben.

## Aus dem Quelltext

Wie unter [Ubuntu](install-ubuntu.md#aus-dem-quelltext).
