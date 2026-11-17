# Testen auf Mac und Windows

Ein Durchgang dauert pro System etwa zehn Minuten. Schick mir danach die Ausgabe von `lyrics-overlay-watch` und deine Notizen; Screenshots helfen bei allem, was komisch aussieht.

## Builds holen

1. Im GitHub-Repo: **Actions → CI → Run workflow** (Häkchen „Also build the Windows and macOS apps“ bleibt an).
2. Wenn der Lauf fertig ist, unten bei **Artifacts** herunterladen:
   - `lyrics-overlay-macOS`: `.dmg` und `lyrics-overlay-watch-macos.tar.gz`
   - `lyrics-overlay-Windows`: Installer `…-setup.exe` (oder `.msi`) und `lyrics-overlay-watch.exe`

Die Builds sind nicht signiert:

- **macOS:** Die App aus der `.dmg` in „Programme“ ziehen, dann einmal mit Rechtsklick → Öffnen starten und bestätigen. Für das Werkzeug: `tar -xzf lyrics-overlay-watch-macos.tar.gz` und `xattr -d com.apple.quarantine lyrics-overlay-watch`.
- **Windows:** Bei der SmartScreen-Warnung auf „Weitere Informationen“ → „Trotzdem ausführen“.

## 1. Was melden die Player? (Messung B)

Spiele nacheinander in **Spotify**, **Apple Music** und **Amazon Music** einen Song ab (was auf dem System verfügbar ist) und lass dabei das Werkzeug laufen:

- macOS (Terminal): `./lyrics-overlay-watch`
- Windows (PowerShell, im Download-Ordner): `.\lyrics-overlay-watch.exe`

macOS fragt beim ersten Mal, ob das Terminal Spotify bzw. Musik steuern darf: erlauben.

Lass es pro Player etwa 20 Sekunden laufen, spule einmal im Player und pausiere einmal. Kopiere die Ausgabe in eine Textdatei. Wichtig sind: Titel, Künstler und Album korrekt? Läuft die Position mit (`drift` nahe 0)? Welche `controls` stehen da?

Zusätzlich auf Windows: einen Song im **Browser** abspielen (zum Beispiel Amazon Music Web) und schauen, ob er erscheint.

Befehle testen, während ein Player läuft:

```bash
./lyrics-overlay-watch --command playpause
./lyrics-overlay-watch --command seek=60
./lyrics-overlay-watch --command next
```

## 2. Die App

Nach dem Start erscheint das Overlay oben rechts und ein Symbol im Tray (macOS: Menüleiste).

- [ ] Ein Spotify-Song: Erscheinen die Lyrics, und passt die hervorgehobene Zeile zum Gesang?
- [ ] Wenn der Song Wortzeiten hat: Leuchten die Wörter einzeln auf?
- [ ] Pause, Spulen und Songwechsel im Player: Folgt das Overlay innerhalb einer Sekunde?
- [ ] Dasselbe mit Apple Music (und Amazon Music, falls es in Teil 1 Daten geliefert hat).
- [ ] Bleibt das Overlay über anderen Fenstern, auch über einem Vollbild-Video und auf einem anderen Schreibtisch? (Messung A)
- [ ] Klick in eine andere App: Behält diese den Fokus?
- [ ] Overlay ziehen; App beenden und neu starten: Ist die Position gemerkt?
- [ ] Tray: Ausblenden/Einblenden, Klicks durchlassen, Ecken, Wiedergabequelle.
- [ ] Alt+Shift+L (macOS: ⌥⇧L) blendet das Overlay aus und ein.
- [ ] Einstellungen (Symbol im Overlay oder Tray): Layouts, Schriftgröße, Deckkraft, Breite und Höhe wirken sofort.
- [ ] Song & Timing: ±0,25 s ändert das Timing hörbar; nach einem Songwechsel und zurück ist der Wert noch da.
- [ ] Sprache Deutsch/Englisch umschalten: Overlay, Einstellungen und Tray wechseln.
- [ ] Bei Anmeldung öffnen: einschalten, abmelden und wieder anmelden.
- [ ] App ein zweites Mal öffnen, während sie läuft: Öffnen sich die Einstellungen, und ist „Klicks durchlassen“ aus?

Notiere zu jedem Punkt ✅ oder was passiert ist.

## Demo ohne Player

`--demo` spielt einen eingebauten Song mit Wortzeiten:

- macOS: `/Applications/Lyrics\ Overlay.app/Contents/MacOS/lyrics-overlay --demo`
- Windows: `"%LOCALAPPDATA%\Lyrics Overlay\lyrics-overlay.exe" --demo` (bei der `.msi`: `"C:\Program Files\Lyrics Overlay\lyrics-overlay.exe"`)
