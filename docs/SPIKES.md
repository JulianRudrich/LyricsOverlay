# Messungen

Drei Fragen, deren Antworten den Bau der nächsten Phasen bestimmen. Anleitung für A und B: [TESTING.md](TESTING.md).

## A: Overlay-Verhalten je System

**Frage:** Bleibt das Overlay über allen Fenstern (auch Vollbild), auf allen Arbeitsflächen, ohne Fokus zu stehlen, mit funktionierendem Klick-Durchlass?

**Bekannt:**
- Linux X11: funktioniert im automatischen Test. Ohne Compositor (virtueller Bildschirm) sind die runden Ecken schwarz statt durchsichtig; auf echten Desktops nicht.
- Linux Wayland: „immer oben“ setzt der Compositor nicht für normale Fenster; GNOME braucht XWayland, KDE und wlroots können Layer-Shell ([tauri#3117](https://github.com/tauri-apps/tauri/issues/3117)).
- macOS: Über Vollbild-Apps braucht es das Fensterverhalten `fullScreenAuxiliary`, das Tauri nicht direkt anbietet. Die Swift-App hatte es.

**Ergebnis:** offen (Julian)

## B: Was die Player melden

**Frage:** Liefern Spotify, Apple Music und Amazon Music auf Windows und macOS Titel, Künstler, Album, Position und Steuerung?

**Bekannt aus der Recherche:**
- Windows meldet die Position nur zum Zeitpunkt der letzten Änderung; die App rechnet ab dort weiter. Spotify aktualisiert sie nur bei Play, Pause und Spulen.
- Apple Music für Windows schreibt „Künstler — Album“ ins Künstlerfeld und lässt das Album leer; die App trennt das ([Bericht](https://github.com/marcimastro98/Xenon/issues/128)).
- Amazon Music für Windows liefert laut einem Projekt nur einen Teil der Daten ([Bridge](https://github.com/GoldenPigeon92/Amazon-Music-SMTC-Bridge)).
- Amazon Music für macOS hat keine AppleScript-Schnittstelle. Andere Player sind dort nur über eine inoffizielle Systemschnittstelle erreichbar ([mediaremote-adapter](https://github.com/ungive/mediaremote-adapter)).

**Ergebnis:**

| Player | Windows | macOS |
|---|---|---|
| Spotify | offen | offen |
| Apple Music | offen | offen |
| Amazon Music | offen | offen |
| Browser (Amazon Music Web) | offen | — |

## C: Wortzeiten bei LRCLIB

**Frage:** Liefert LRCLIB Wortzeiten (`lyricsfile` mit `words`), und für wie viele Songs?

**Ergebnis (17.11.2026, zehn bekannte Songs, je 20 Suchtreffer):**

- Jeder Treffer hat ein Feld `lyricsfile` (YAML als Text) und ein Kennzeichen `hasWordSync`. Alle 200 Dokumente ließen sich lesen.
- **88 von 200** Treffern haben Wortzeiten; **8 von 10 Songs** haben mindestens eine Fassung mit Wortzeiten.
- Der Anteil schwankt stark: „As It Was“ 19/20, „Levitating“ 17/20, „Haus am See“ 17/20, „Blinding Lights“ 16/20, „Sonne“ 13/20, „Espresso“ 4/20, „BIRDS OF A FEATHER“ 1/20, „Shape of You“ 1/20, „Cruel Summer“ 0/20, „Bohemian Rhapsody“ 0/20.

Die Suche der App mit dem echten LRCLIB (`cargo run -p lyrics-providers --example lookup`):

| Song | Ergebnis |
|---|---|
| The Weeknd – Blinding Lights | Wort für Wort, 35 Zeilen |
| Harry Styles – As It Was | Wort für Wort, 37 Zeilen |
| Rammstein – Sonne | Wort für Wort, 40 Zeilen |
| Peter Fox – Haus am See (216 s) | Wort für Wort, 50 Zeilen |
| Taylor Swift – Cruel Summer | zeilenweise (keine Wortdaten vorhanden) |
| Queen – Bohemian Rhapsody | zeilenweise (keine Wortdaten vorhanden) |

**Folgen:**

- Echtes Wort-für-Wort ist für einen großen Teil bekannter Songs sofort verfügbar. Für den Rest braucht es die geschätzte Hervorhebung aus Phase 2.
- Ein exakter Treffer (Dauer ±2,5 s) gewinnt immer, auch ohne Wortzeiten: Eine Fassung mit anderer Dauer hat oft ein verschobenes Timing. Beispiel: „Haus am See“ mit 222 s trifft eine exakte Fassung ohne Wortzeiten; die Fassungen mit Wortzeiten sind 216 s lang.
- Bei Gleichstand in der Suche gewinnen jetzt Wortzeiten; das fiel beim Test mit echten Daten auf und ist behoben.
- Idee für später: Hat der exakte Treffer keine Wortzeiten, eine Fassung mit Wortzeiten und fast gleicher Dauer (±1 s) aus der Suche nehmen.
