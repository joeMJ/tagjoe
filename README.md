# tagjoe

**Check and fix tags, covers and file names of albums, compilations and audiobooks – FileBot-style, for audio.**

Drop a folder onto the window. tagjoe reads every file, identifies the release
(acoustic fingerprints via AcoustID, MusicBrainz, Cover Art Archive, optionally Discogs;
AudiobookDB and Audnexus for audiobooks), checks **every tag field** against a fixed schema,
and proposes tags, an embedded cover and file names. Nothing is written until you click
**Apply**. Every run writes a journal, so it can be undone.

- Formats: MP3 (ID3v2/ID3v1/APE), FLAC, Ogg Vorbis, Opus, MP4/M4A/M4B, AAC, WAV, AIFF, APE, WavPack, Musepack
- Foreign fields (comments, encoder junk, shop URLs, …) are listed and removed; a keep list protects what you want
- Folder clean-up: NFO, M3U, image, TXT and further files are deleted after a clean run (each one shown first, optional)
- Progress and an activity log (`log.txt`) in the window; a queue entry can drop out once written
- Manual completion of any field and of the cover (file or URL) when no source has the data
- File and folder name templates with placeholders, functions and conditions, live preview
- Command line (`tagjoe scan | propose | apply | clean | undo | identify`) and desktop app (`tagjoe-gui`)
- Linux (Ubuntu 24.04/26.04, Debian 12/13) now; Windows next; macOS later

## Install (Debian/Ubuntu)

```bash
sudo apt install ./tagjoe_<version>_<distro>_<arch>.deb
sudo apt install libchromaprint-tools   # fpcalc, for acoustic fingerprints
```

For fingerprint lookups register an application key at <https://acoustid.org/new-application>
and paste it into Settings → Sources, or into `~/.config/tagjoe/config.toml` (`acoustid_key`).

## Build from source

```bash
cargo build --release
./target/release/tagjoe --help
./target/release/tagjoe-gui
```

## Credits

Music data: [MusicBrainz](https://musicbrainz.org), [AcoustID](https://acoustid.org),
[Cover Art Archive](https://coverartarchive.org), [Discogs](https://www.discogs.com) (optional).
Audiobook data: [AudiobookDB](https://audiobookdb.org) and [Audnexus](https://github.com/laxamentumtech/audnexus).

## License

Apache License 2.0 – see [LICENSE](LICENSE).

Deutsche Fassung: [README.de.md](README.de.md)
