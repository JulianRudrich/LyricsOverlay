# Abnahmeliste gegen Linory

Jede Funktion von Linory 0.3.1 (jetzt in [JulianRudrich/LyricsOverlay#1](https://github.com/JulianRudrich/LyricsOverlay/pull/1)) muss die neue App haben. Veröffentlicht wird erst, wenn jede Zeile für alle drei Systeme erledigt ist. Vor der Veröffentlichung wird die Liste mit dem dann aktuellen Linory-Stand abgeglichen.

**Zeichen:** ✅ gebaut und getestet · 🧪 gebaut, Test auf dem Gerät fehlt · ⬜ offen · — gibt es auf diesem System nicht

Spalten: **Lin** Linux · **Win** Windows · **Mac** macOS. Linux ist automatisiert getestet (virtueller Bildschirm, nachgebauter Player).

## Overlay

| Funktion | Lin | Win | Mac | Anmerkung |
|---|:-:|:-:|:-:|---|
| Schwebt über anderen Fenstern, auf allen Arbeitsflächen | ✅ | 🧪 | 🧪 | macOS über Vollbild-Apps: Messung A |
| Nimmt anderen Apps nie den Fokus | 🧪 | 🧪 | 🧪 | |
| Ziehen, vier Ecken, Position gemerkt, mehrere Bildschirme | 🧪 | 🧪 | 🧪 | |
| Klicks durchlassen, gemerkt; erneutes Öffnen hebt es auf | 🧪 | 🧪 | 🧪 | |
| Shortcut zum Ein- und Ausblenden | 🧪 | 🧪 | 🧪 | Linux nur X11; Wayland in Phase 3 |
| Drei Layouts, Schrift, Deckkraft, Größe | ✅ | 🧪 | 🧪 | Linory hat ein festes Layout |
| Wort-für-Wort-Highlight mit echten Wortzeiten | ✅ | 🧪 | 🧪 | Linory schätzt meist nur |
| Geschätztes Wort-Highlight für zeilenweise Lyrics | ⬜ | ⬜ | ⬜ | Phase 2 |
| Scrollende Zeilen, weicher Songwechsel | ⬜ | ⬜ | ⬜ | heute: Einblenden der neuen Zeile |
| Lange Zeilen teilen | ⬜ | ⬜ | ⬜ | Phase 2 |
| Cover | ⬜ | ⬜ | ⬜ | Bild-URL wird schon gelesen |
| Ausblenden, wenn nichts läuft | ⬜ | ⬜ | ⬜ | Phase 1 |

## Mini-Player

| Funktion | Lin | Win | Mac | Anmerkung |
|---|:-:|:-:|:-:|---|
| Play/Pause, Vor, Zurück, Spulen (Technik) | ✅ | 🧪 | 🧪 | Befehle sind gebaut, die Oberfläche fehlt |
| Bedienleiste, Spul-Leiste mit Zeiten | ⬜ | ⬜ | ⬜ | Phase 2 |
| Klick auf eine Zeile springt dorthin | ⬜ | ⬜ | ⬜ | Phase 2 |
| Refrain-Punkte | ⬜ | ⬜ | ⬜ | Phase 2 |
| ♥ (Spotify) | ⬜ | ⬜ | ⬜ | braucht die Spotify-Verbindung |
| In Spotify öffnen | ⬜ | ⬜ | ⬜ | |

## Sync und Lyrics

| Funktion | Lin | Win | Mac | Anmerkung |
|---|:-:|:-:|:-:|---|
| Position zwischen Abfragen fortschreiben, Ruckeln glätten | ✅ | 🧪 | 🧪 | Zeilenwechsel ~10 ms nach dem Zeitstempel |
| Sofortige Reaktion auf Songwechsel per Systemmeldung | ⬜ | ⬜ | ⬜ | heute 0,5 s (Mac 0,8 s) Abfrage |
| Korrektur pro Song und global | ✅ | 🧪 | 🧪 | Linory: nur global |
| LRCLIB exakt, dann Suche, gleiches Album bevorzugt | ✅ | ✅ | ✅ | systemunabhängig getestet |
| Lokaler Cache | ✅ | ✅ | ✅ | |
| Klartext-Lyrics, als „nicht synchron“ markiert | ✅ | ✅ | ✅ | Zeiten nach Zeilenlänge statt gleichmäßig |
| Instrumental erkannt | ✅ | ✅ | ✅ | |
| Eigene Lyrics: Speicher | ✅ | ✅ | ✅ | |
| Eigene Lyrics: LRC-Import und Editor | ⬜ | ⬜ | ⬜ | Phase 1; aus Julians 0.2 |

## Player

| Dienst | Lin | Win | Mac | Anmerkung |
|---|:-:|:-:|:-:|---|
| Spotify (Desktop-App) | ✅ | 🧪 | 🧪 | Linux mit nachgebautem Player getestet |
| Apple Music | ⬜ | 🧪 | 🧪 | Windows: „Künstler — Album“ wird getrennt; Linux nur Web (Phase 3) |
| Amazon Music | ⬜ | 🧪 | ⬜ | Windows: Messung B; sonst Browser-Erweiterung (Phase 3) |
| Auswahl Automatisch / fest | ✅ | ✅ | ✅ | |
| Andere Player (Browser, VLC …) | ✅ | 🧪 | — | macOS: Messung B |

## Weitere Funktionen

| Funktion | Lin | Win | Mac | Anmerkung |
|---|:-:|:-:|:-:|---|
| Deutsch und Englisch, Systemsprache | ✅ | 🧪 | 🧪 | |
| Bei Anmeldung starten | 🧪 | 🧪 | 🧪 | |
| Tray-Menü | 🧪 | 🧪 | 🧪 | Linory: Glas-Panel mit laufendem Titel (Phase 2) |
| Sleep-Timer | ⬜ | ⬜ | ⬜ | Phase 2 |
| „Deine Musik“ (Statistiken) | ⬜ | ⬜ | ⬜ | Phase 2; für jeden Player |
| Spotify-Verbindung, True Shuffle, Toplisten | ⬜ | ⬜ | ⬜ | Phase 2 |
| Installer | ⬜ | 🧪 | 🧪 | CI baut .exe/.msi und .dmg |
| Homebrew | — | — | ⬜ | Phase 2 |
| Automatische Updates | ⬜ | ⬜ | ⬜ | Phase 1 |
