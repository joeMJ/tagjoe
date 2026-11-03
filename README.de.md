# tagjoe

**Tags, Cover und Dateinamen von Alben, Samplern und Hörbüchern prüfen und korrigieren – nach dem FileBot-Prinzip, für Audio.**

Ordner auf das Fenster ziehen. tagjoe liest jede Datei, erkennt das Release (Audio-Fingerabdruck
über AcoustID, MusicBrainz, Cover Art Archive, wahlweise Discogs; AudiobookDB und Audnexus für
Hörbücher), prüft **jedes Tag-Feld** gegen ein festes Schema und schlägt Tags, eingebettetes Cover
und Dateinamen vor. Geschrieben wird erst nach **Übernehmen**. Jeder Lauf schreibt ein Journal und
lässt sich rückgängig machen.

- Formate: MP3 (ID3v2/ID3v1/APE), FLAC, Ogg Vorbis, Opus, MP4/M4A/M4B, AAC, WAV, AIFF, APE, WavPack, Musepack
- Fremde Felder (Kommentare, Encoder-Reste, Shop-URLs, …) werden aufgelistet und entfernt; eine Behalte-Liste schützt Gewünschtes
- Ordnerbereinigung: NFO-, M3U-, Bild-, TXT- und weitere Dateien werden nach fehlerfreiem Lauf gelöscht (vorher je Datei sichtbar, abwählbar)
- Fortschritt und Protokoll (`log.txt`) im Fenster; Einträge können nach dem Schreiben aus der Warteschlange fallen
- Manuelle Vervollständigung jedes Felds und des Covers (Datei oder URL), wenn keine Quelle Daten hat
- Vorlagen für Datei- und Ordnernamen mit Platzhaltern, Funktionen und Bedingungen, Live-Vorschau
- Kommandozeile (`tagjoe scan | propose | apply | clean | undo | identify`) und Desktop-App (`tagjoe-gui`)
- Linux (Ubuntu 24.04/26.04, Debian 12/13) jetzt; Windows als Nächstes; macOS später

## Installation (Debian/Ubuntu)

```bash
sudo apt install ./tagjoe_<version>_<distro>_<arch>.deb
sudo apt install libchromaprint-tools   # fpcalc, für Audio-Fingerabdrücke
```

Für Fingerabdruck-Abfragen einen Anwendungsschlüssel unter <https://acoustid.org/new-application>
anlegen und in Einstellungen → Quellen eintragen (oder in `~/.config/tagjoe/config.toml`, `acoustid_key`).

## Aus dem Quelltext bauen

```bash
cargo build --release
./target/release/tagjoe --help
./target/release/tagjoe-gui
```

## Danksagung

Musikdaten: [MusicBrainz](https://musicbrainz.org), [AcoustID](https://acoustid.org),
[Cover Art Archive](https://coverartarchive.org), [Discogs](https://www.discogs.com) (optional).
Hörbuchdaten: [AudiobookDB](https://audiobookdb.org) und [Audnexus](https://github.com/laxamentumtech/audnexus).

## Lizenz

Apache License 2.0 – siehe [LICENSE](LICENSE).

English version: [README.md](README.md)
