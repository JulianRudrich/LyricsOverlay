import Foundation
import SwiftUI

struct SettingsView: View {
    @ObservedObject var model: PlayerModel
    var resize: () -> Void

    @AppStorage("fontSize") private var fontSize = 25.0
    @AppStorage("opacity") private var opacity = 0.78
    @AppStorage("overlayOpacity") private var overlayOpacity = 1.0
    @AppStorage("offset") private var offset = 0.0
    @AppStorage("overlayWidth") private var width = 400.0
    @AppStorage("overlayHeight") private var height = 240.0
    @AppStorage("overlayLayout") private var layout = OverlayLayout.context
    @AppStorage("lookupOnline") private var lookupOnline = true
    @AppStorage("playerPreference") private var playerPreference = PlayerPreference.automatic

    @State private var importError: String?
    @State private var editingTrack: EditingTrack?

    private struct EditingTrack: Identifiable {
        let track: Track
        let text: String
        var id: String { track.id }
    }

    var body: some View {
        TabView {
            ScrollView {
                appearance
                    .padding(22)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .tabItem { Label("Ansicht", systemImage: "rectangle.on.rectangle") }

            ScrollView {
                song
                    .padding(22)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .tabItem { Label("Song & Timing", systemImage: "music.note") }
        }
        .padding(14)
        .onChange(of: width) { _ in resize() }
        .onChange(of: height) { _ in resize() }
        .onChange(of: layout) { _ in resize() }
        .onChange(of: lookupOnline) { _ in model.reloadLyrics() }
        .onChange(of: playerPreference) { model.setPlayerPreference($0) }
        .onChange(of: model.track?.id) { _ in importError = nil }
        .sheet(item: $editingTrack) { selection in
            LRCEditorView(initialText: selection.text, position: {
                guard model.track?.id == selection.id else { return nil }
                return model.position(at: ProcessInfo.processInfo.systemUptime) + offset + model.songOffset
            }, save: { text in
                try model.saveLocalLyrics(text, for: selection.track)
            })
        }
    }

    private var appearance: some View {
        VStack(alignment: .leading, spacing: 14) {
            Picker("Layout", selection: $layout) {
                ForEach(OverlayLayout.allCases) { layout in
                    Text(layout.title).tag(layout)
                }
            }
            .pickerStyle(.segmented)
            control("Schriftgröße", value: $fontSize, range: 14...44, suffix: " pt")
            control("Deckkraft des Overlays", value: $overlayOpacity, range: 0.15...1, multiplier: 100, suffix: " %")
            control("Deckkraft des Hintergrunds", value: $opacity, range: 0...1, multiplier: 100, suffix: " %")
            control("Breite", value: $width, range: 260...800, suffix: " pt")
            if layout == .context {
                control("Höhe", value: $height, range: 160...500, suffix: " pt")
            }
            Text("⌥ ⇧ L blendet das Overlay ein oder aus.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var song: some View {
        VStack(alignment: .leading, spacing: 14) {
            Picker("Wiedergabequelle", selection: $playerPreference) {
                ForEach(PlayerPreference.allCases) { source in
                    Text(source.title).tag(source)
                }
            }
            .disabled(model.isDemo)

            if model.isDemo {
                Text("Vorschau mit Beispieltext")
                    .font(.headline)
            } else if let track = model.track {
                VStack(alignment: .leading, spacing: 4) {
                    Text(track.title).font(.headline)
                    Text([track.artist, model.activeSource?.title].compactMap { $0 }.joined(separator: " · "))
                        .foregroundStyle(.secondary)
                }
                .lineLimit(1)

                HStack(spacing: 10) {
                    Text("Korrektur für diesen Song")
                    Spacer()
                    Button("−0,25") { model.adjustSongOffset(by: -0.25) }
                        .accessibilityLabel("Lyrics eine Viertelsekunde später anzeigen")
                        .disabled(model.songOffset <= -10)
                    Text(String(format: "%+.2f s", model.songOffset))
                        .monospacedDigit()
                        .frame(minWidth: 60)
                    Button("+0,25") { model.adjustSongOffset(by: 0.25) }
                        .accessibilityLabel("Lyrics eine Viertelsekunde früher anzeigen")
                        .disabled(model.songOffset >= 10)
                    Button("Zurücksetzen") { model.setSongOffset(0) }
                        .disabled(model.songOffset == 0)
                }

                HStack {
                    Button("Lyrics bearbeiten …") {
                        editingTrack = EditingTrack(track: track, text: model.localLyricsText(for: track) ?? "")
                    }
                    Button("LRC importieren …") {
                        importError = model.importLyrics(for: track)
                    }
                    if model.state == .synced(local: true) {
                        Button("LRC entfernen") {
                            importError = model.removeLocalLyrics()
                        }
                    }
                }
                if let importError {
                    Text(importError)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .fixedSize(horizontal: false, vertical: true)
                }
            } else {
                Text("Spiele einen Song in Spotify oder Apple Music ab, um seine Lyrics oder das Timing zu bearbeiten.")
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Divider()
            control("Globale Timing-Korrektur", value: $offset, range: -5...5, suffix: " s", decimals: 1)
            Text("Positive Werte zeigen Lyrics früher. Die globale Korrektur und die Korrektur für den Song werden addiert.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)

            Toggle("Lyrics online suchen", isOn: $lookupOnline)
                .disabled(model.isDemo)
            Text("Die Onlinesuche sendet Titel, Interpret, Album und Dauer an LRCLIB. Eigene Lyrics bleiben lokal.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func control(_ title: String, value: Binding<Double>, range: ClosedRange<Double>,
                         multiplier: Double = 1, suffix: String, decimals: Int = 0) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(title)
                Spacer()
                Text(String(format: "%.*f", decimals, value.wrappedValue * multiplier) + suffix)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
            }
            Slider(value: value, in: range)
                .accessibilityLabel(title)
        }
    }
}
