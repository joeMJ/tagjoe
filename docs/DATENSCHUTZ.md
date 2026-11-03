# Datenschutz und Netzzugriffe

tagjoe sendet keine Nutzungsdaten und hat keine Telemetrie und keine Absturzberichte. Nach der
Installation geht **nichts** ins Netz: Jede Online-Quelle ist ausgeschaltet, bis du sie unter
**Einstellungen → Quellen und Schlüssel** einschaltest. Ohne Quellen arbeitet tagjoe nur mit dem,
was in den Dateien und Ordnernamen steht; für Erkennung, Cover und Hörbuchdaten braucht es
mindestens eine Quelle. Bei jedem Zugriff auf einen fremden Server wird, wie bei jedem
Netzzugriff, die eigene IP-Adresse übermittelt, dazu die Programmkennung `tagjoe/<Version>`
(bei MusicBrainz mit der Projektadresse, wie MusicBrainz es verlangt). Antworten werden lokal
zwischengespeichert, damit dieselbe Frage nicht zweimal gestellt wird.

## Nie

- **Audible** wird nie direkt angefragt.
- Audiodateien werden nie hochgeladen. Für AcoustID berechnet das Programm `fpcalc`
  (Chromaprint) auf deinem Rechner einen akustischen Fingerabdruck; nur dieser kurze
  Zahlenwert und die Dauer der Datei gehen zu AcoustID, nicht die Musik.

## Nur wenn eingeschaltet, je Quelle

| Quelle | Was gesendet wird | Wofür | Datenschutz des Anbieters |
|---|---|---|---|
| **AcoustID** (`api.acoustid.org`) | Fingerabdruck und Dauer jeder Datei, dein Anwendungsschlüssel | Erkennung der Aufnahmen und Veröffentlichungen | keine eigene Datenschutzseite; [acoustid.org](https://acoustid.org), Daten unter [CC BY-SA 3.0](https://acoustid.org/license) |
| **MusicBrainz** (`musicbrainz.org`) | Album, Interpret bzw. Titel und Autor als Suchbegriffe; Kennungen von Veröffentlichungen | Titel, Nummern, Jahr, Label, Katalognummer, Kapitel | [MetaBrainz Privacy Policy](https://metabrainz.org/privacy) |
| **Cover Art Archive** (`coverartarchive.org`, Bilder über `archive.org`) | Kennung der Veröffentlichung | Cover | [MetaBrainz Privacy Policy](https://metabrainz.org/privacy), [Internet Archive](https://archive.org/about/terms) |
| **Discogs** (`api.discogs.com`) | Album und Interpret als Suchbegriffe, Kennungen, dein persönlicher Zugangsschlüssel | zweite Musikquelle, Cover | [Discogs Privacy Policy](https://www.discogs.com/privacy) |
| **AudiobookDB** (`audiobookdb.org`) | Titel als Suchbegriff, Buchkennungen, falls eingetragen dein API-Schlüssel | Hörbuchdaten, Kapitel, Cover | [AudiobookDB Privacy Policy](https://audiobookdb.org/privacy) |
| **Audnexus** (`api.audnex.us`) | ASIN und Region | Audible-Metadaten über die freie Audnexus-Schnittstelle | keine veröffentlichte Datenschutzerklärung; [Projektseite](https://github.com/laxamentumtech/audnexus) |
| **Deutsche Nationalbibliothek** (`services.dnb.de`) | Titel und Autor als Suchbegriffe (SRU) | deutsche Hörbücher: Titel, Autor, Sprecher, Verlag, Jahr, ISBN | [Datenschutzerklärung der DNB](https://www.dnb.de/DE/Service/Datenschutz/datenschutz_node.html) |
| **Open Library** (`covers.openlibrary.org`) | ISBN | Cover zu DNB-Treffern | [Internet Archive](https://archive.org/about/terms) |
| **GitHub** (`github.com`), nur auf Klick | nichts außer IP-Adresse und Programmkennung | **fpcalc herunterladen** (Einstellungen → Quellen → AcoustID): lädt das Chromaprint-Release `chromaprint-fpcalc-1.6.2-…` von `github.com/acoustid/chromaprint`, prüft die SHA-256-Summe gegen den im Programm hinterlegten Wert und legt `fpcalc` in den eigenen Werkzeugordner (Windows `%LOCALAPPDATA%\tagjoe\bin`, Linux `~/.local/share/tagjoe/bin`) | [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) |
| **GitHub** (`api.github.com`, Downloads von `github.com`) | nichts außer IP-Adresse und Programmkennung | Updateprüfung (**Einstellungen → Allgemein**, standardmäßig aus): höchstens einmal täglich die Frage nach dem neuesten Release; ein Paket wird nur nach deiner Bestätigung geladen und gegen `SHA256SUMS` geprüft | [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) |

Cover, die eine Quelle als Adresse nennt (Discogs-Bilder, Open Library), lädt tagjoe von dort.
Verweise in den Einstellungen öffnen erst beim Klick den Browser.

## Was lokal gespeichert wird

Alles liegt in deinem Benutzerprofil (Linux: `~/.config/tagjoe/` und `~/.cache/tagjoe/`;
Windows: `%APPDATA%\tagjoe\` und `%LOCALAPPDATA%\tagjoe\`):

- **Konfiguration** `config.toml`: Einstellungen und deine Schlüssel (AcoustID, Discogs,
  AudiobookDB) im Klartext. Die Datei ist so geschützt wie dein Benutzerordner; gib sie nicht
  weiter.
- **Journal** `journal/`: je Schreibvorgang die alten Werte (Tags, entfernte Einträge, vorheriges
  Cover als Datei, alte Dateinamen), damit „Rückgängig“ funktioniert. Enthält Pfade und
  Dateinamen deiner Musik.
- **Protokoll** `log.txt`: die letzten 2000 Vorgänge mit Zeit, Ordner, Datei und Ergebnis;
  „Protokoll leeren“ löscht die Datei.
- **Zwischenspeicher** (Linux `~/.cache/tagjoe/`, Windows `%LOCALAPPDATA%\tagjoe\`): `http/` mit
  Antworten der Quellen (Standard 30 Tage, einstellbar), `updates/` mit heruntergeladenen Paketen,
  unter Windows `bin\fpcalc.exe`. Kann jederzeit gelöscht werden.

Die Audiodateien selbst werden nur dort verändert, wo du sie abgelegt hast, und nur nach
„Übernehmen“ oder „Nur bereinigen“. Die Ordnerbereinigung löscht Dateien **endgültig** (kein
Papierkorb); welche, steht vorher in der Dateiliste und in den Einstellungen.

## Löschen

- Quellen abschalten: **Einstellungen → Quellen und Schlüssel**.
- Schlüssel entfernen: dort die Felder leeren oder `config.toml` bearbeiten.
- Alles von tagjoe entfernen: Paket deinstallieren und die beiden Ordner oben löschen.

## Fehler melden

Der Eintrag **Einstellungen → Allgemein → Updates** verweist nur auf die Issue-Seite bei GitHub;
tagjoe selbst überträgt dabei nichts. Bitte keine Schlüssel und keine privaten Pfade in
Meldungen.
