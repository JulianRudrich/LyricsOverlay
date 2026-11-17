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

**Bekannt:** Andere Projekte berichten von einem Feld `lyricsfile` im Lyricsfile-Format ([Spezifikation](https://github.com/tranxuanthang/lyricsfile)). Die App liest es als Text oder als Objekt und bevorzugt Wortzeiten; ein unlesbares Feld verhindert keine Lyrics.

**Ergebnis:** offen. In dieser Entwicklungsumgebung ist `lrclib.net` gesperrt. Auf deinem Rechner prüft dieser Befehl einen Song:

```bash
curl -s "https://lrclib.net/api/search?track_name=Blinding%20Lights&artist_name=The%20Weeknd" | python3 -c "import json,sys; r=json.load(sys.stdin); print(len(r), 'Treffer;', sum(1 for x in r if x.get('lyricsfile')), 'mit lyricsfile')"
```
