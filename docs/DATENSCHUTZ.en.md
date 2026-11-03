# Privacy and network access

tagjoe sends no usage data and has no telemetry and no crash reports. After installation
**nothing** goes to the network: every online source is off until you switch it on under
**Settings → Sources and keys**. Without sources tagjoe works only with what the files and folder
names contain; identification, covers and audiobook data need at least one source. As with any
network access, every request to another server carries your IP address, plus the program id
`tagjoe/<version>` (for MusicBrainz with the project address, as MusicBrainz requires). Answers
are cached locally so the same question is not asked twice.

## Never

- **Audible** is never contacted directly.
- Audio files are never uploaded. For AcoustID the program `fpcalc` (Chromaprint) computes an
  acoustic fingerprint on your computer; only this short number sequence and the duration of the
  file go to AcoustID, not the music.

## Only when switched on, per source

| Source | What is sent | Used for | Provider's privacy page |
|---|---|---|---|
| **AcoustID** (`api.acoustid.org`) | fingerprint and duration of every file, your application key | recognising recordings and releases | no privacy page of its own; [acoustid.org](https://acoustid.org), data under [CC BY-SA 3.0](https://acoustid.org/license) |
| **MusicBrainz** (`musicbrainz.org`) | album, artist or title and author as search terms; release ids | titles, numbers, year, label, catalogue number, chapters | [MetaBrainz privacy policy](https://metabrainz.org/privacy) |
| **Cover Art Archive** (`coverartarchive.org`, images via `archive.org`) | release id | covers | [MetaBrainz privacy policy](https://metabrainz.org/privacy), [Internet Archive](https://archive.org/about/terms) |
| **Discogs** (`api.discogs.com`) | album and artist as search terms, ids, your personal access token | second music source, covers | [Discogs privacy policy](https://www.discogs.com/privacy) |
| **AudiobookDB** (`audiobookdb.org`) | title as search term, book ids, your API key if set | audiobook data, chapters, covers | [AudiobookDB privacy policy](https://audiobookdb.org/privacy) |
| **Audnexus** (`api.audnex.us`) | ASIN and region | Audible metadata through the free Audnexus API | no published privacy policy; [project page](https://github.com/laxamentumtech/audnexus) |
| **Deutsche Nationalbibliothek** (`services.dnb.de`) | title and author as search terms (SRU) | German audiobooks: title, author, narrator, publisher, year, ISBN | [DNB privacy statement](https://www.dnb.de/DE/Service/Datenschutz/datenschutz_node.html) (German) |
| **Open Library** (`covers.openlibrary.org`) | ISBN | covers for DNB results | [Internet Archive](https://archive.org/about/terms) |
| **GitHub** (`github.com`), only on click | nothing but IP address and program id | **Download fpcalc** (Settings → Sources → AcoustID): downloads the Chromaprint release `chromaprint-fpcalc-1.6.2-…` from `github.com/acoustid/chromaprint`, checks its SHA-256 sum against the value compiled into the program and puts `fpcalc` into tagjoe's own tools folder (Windows `%LOCALAPPDATA%\tagjoe\bin`, Linux `~/.local/share/tagjoe/bin`) | [GitHub privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) |
| **GitHub** (`api.github.com`, downloads from `github.com`) | nothing but IP address and program id | update check (**Settings → General**, off by default): at most once a day the question for the latest release; a package is downloaded only after your confirmation and verified against `SHA256SUMS` | [GitHub privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement) |

Covers that a source names by address (Discogs images, Open Library) are fetched from there.
Links in the settings open the browser only when clicked.

## What is stored locally

Everything lives in your user profile (Linux: `~/.config/tagjoe/` and `~/.cache/tagjoe/`;
Windows: `%APPDATA%\tagjoe\` and `%LOCALAPPDATA%\tagjoe\`):

- **Configuration** `config.toml`: settings and your keys (AcoustID, Discogs, AudiobookDB) in
  plain text. The file is protected like your home folder; do not pass it on.
- **Journal** `journal/`: per write run the old values (tags, removed entries, the previous cover
  as a file, old file names) so that "Undo" works. Contains paths and file names of your music.
- **Log** `log.txt`: the last 2000 actions with time, folder, file and result; "Clear log"
  deletes the file.
- **Cache** (Linux `~/.cache/tagjoe/`, Windows `%LOCALAPPDATA%\tagjoe\`): `http/` with answers of
  the sources (30 days by default, adjustable), `updates/` with downloaded packages, on Windows
  `bin\fpcalc.exe`. Can be deleted at any time.

The audio files themselves are changed only where you keep them, and only after "Apply" or
"Clean only". The folder clean-up deletes files **for good** (no trash); which ones is shown in
the files table and in the settings beforehand.

## Deleting

- Switch sources off: **Settings → Sources and keys**.
- Remove keys: clear the fields there or edit `config.toml`.
- Remove everything of tagjoe: uninstall the package and delete the two folders above.

## Reporting bugs

The entry **Settings → General → Updates** only links to the issue page on GitHub; tagjoe itself
transmits nothing. Please include no keys and no private paths in reports.
