# Roadmap

Lyrics Overlay wird plattformübergreifend neu gebaut: eine Codebasis für Windows, macOS und Linux, mit Spotify, Apple Music und Amazon Music. Die Swift-App bleibt bis zur Funktionsgleichheit die macOS-Version.

## Entscheidungen

| Thema | Entscheidung |
|---|---|
| Technik | Tauri 2, Rust-Kern, Oberfläche in Svelte 5 + TypeScript |
| Lizenz | GPL-3.0-or-later für den neuen Code; veröffentlichte Swift-Versionen bleiben MIT |
| Entwicklung | öffentlich im Original-Repo auf dem Branch `next`; `main` bleibt die Swift-App, bis `next` sie ablöst |
| Tests | Mac und Windows-PC durch Julian; Linux automatisiert in CI |
| Umfang | jede Linory-Funktion ist Pflicht, dazu mehr ([PARITY.md](PARITY.md)) |
| Branding | eigener Name, eigenes Logo, eigene Bilder; weder Linory noch Mikes Icon |
| 0.2-Funktionen | LRC-Editor, Korrektur pro Song und drei Layouts kommen nur in den Neubau (Phase 1) |

## Neu: Mike gibt Linory zurück (17.11.2026)

Mike (MikeLeonFox) hat [JulianRudrich/LyricsOverlay#1](https://github.com/JulianRudrich/LyricsOverlay/pull/1) geöffnet: alle Linory-Funktionen in einem Commit, unter Julians Namen, Bundle-ID und Links, weiter unter MIT. Folgen:

- Linorys Swift-Code ist damit die offizielle Vorlage für Phase 2. Wir portieren seine Funktionen statt sie nachzubauen und nennen Mike als Mitautor.
- Gegen `main` ist der PR konfliktfrei. Gegen `preview/lyrics-overlay-0.2` gibt es Konflikte in 11 Dateien; Mike bietet an, beim Zusammenführen zu helfen.
- Entschieden: Der PR wird gemergt, damit das Original-Repo wieder alles hat. Julians 0.2-Funktionen kommen nicht in die Swift-App, sondern in Phase 1 des Neubaus. Name, Logo und Bilder werden neu gestaltet.
- Aus Mikes Beitrag portierter Code bleibt unter MIT nutzbar; im Neubau steht er unter GPL-3.0-or-later, mit Mike als Mitautor.

## Phasen

**Phase 0: Fundament und Messungen.** Fast fertig.
- [x] Kern in Rust mit allen Prüfungen der Swift-App
- [x] Player-Erkennung: MPRIS, SMTC, AppleScript; Diagnose-Werkzeug `lyrics-overlay-watch`
- [x] LRCLIB-Suche, Cache, eigene Lyrics, Korrektur pro Song (SQLite)
- [x] App: Overlay, Einstellungen, Tray, Shortcut, Demo-Modus
- [x] Linux-Gesamttest in CI mit Screenshot
- [x] Messung C: LRCLIB liefert für 8 von 10 bekannten Songs Wortzeiten ([SPIKES.md](SPIKES.md))
- [ ] Messungen A und B: brauchen Julians Mac und Windows-PC
- [x] Im Original-Repo auf `next`; CI baut bei jedem Push Testversionen für Mac und Windows

**Phase 1: Dein 0.2 auf drei Systemen (erste Alpha).** LRC-Import und Editor mit Zeitstempel-Taste, automatisches Ausblenden, frei belegbarer Shortcut, automatische Updates, Installer, Feinschliff aus den Messungen (zum Beispiel Overlay über Vollbild-Apps auf macOS).

**Phase 2: Alles, was Linory kann.** Aus Mikes Swift-Code portiert: Mini-Player mit Spul-Leiste, Klick auf eine Zeile springt dorthin, Refrain-Punkte, Cover, geschätztes Wort-Highlight für zeilenweise Lyrics, lange Zeilen teilen, Glas-Menü, Sleep-Timer, „Deine Musik“, Spotify-Verbindung mit True Shuffle, ♥ und Toplisten, Homebrew.

**Phase 3: Jeder Player.** Browser-Erweiterung für Amazon Music, Apple Music Web, Spotify Web, YouTube Music, Tidal und Deezer; Amazon-Music-Desktop-Apps, soweit Messung B es hergibt; beliebige Player mit Erlauben- und Ignorieren-Liste; Linux-Feinschliff (Wayland, Paketformate).

**Erstes Release des Neubaus** nach Phase 3; dann löst `next` die Swift-App auf `main` ab.

**Phase 4: Noch mehr.** Weitere Lyrics-Quellen und Versionswahl, Editor mit Wort-Timing, Veröffentlichen bei LRCLIB, Verzögerung je Audioausgang, Übersetzung, Themes, Barrierefreiheit, OBS-Modus.

**Phase 5: Version 1.0.** Name und Marke, Website, Signierung, winget, Flathub, AUR.
