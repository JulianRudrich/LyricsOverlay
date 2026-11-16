import Foundation
import SwiftUI
import LyricsCore

enum OverlayLayout: String, CaseIterable, Identifiable {
    case caption
    case focus
    case context

    var id: Self { self }

    var title: String {
        switch self {
        case .caption: return "Einzeiler"
        case .focus: return "Fokus"
        case .context: return "Kontext"
        }
    }

    func height(for preferredHeight: Double) -> Double {
        switch self {
        case .caption: return min(preferredHeight, 150)
        case .focus: return min(preferredHeight, 205)
        case .context: return preferredHeight
        }
    }
}

struct OverlayView: View {
    @ObservedObject var model: PlayerModel
    var showSettings: () -> Void

    @AppStorage("fontSize") private var fontSize = 25.0
    @AppStorage("opacity") private var opacity = 0.78
    @AppStorage("overlayOpacity") private var overlayOpacity = 1.0
    @AppStorage("offset") private var offset = 0.0
    @AppStorage("overlayLayout") private var layout = OverlayLayout.context

    private let accent = Color(red: 0.48, green: 0.95, blue: 0.83)
    private let base = Color(red: 0.055, green: 0.075, blue: 0.094)

    var body: some View {
        TimelineView(.animation(minimumInterval: 0.08, paused: !model.isPlaying)) { _ in
            let position = model.position(at: ProcessInfo.processInfo.systemUptime)
            let active = LRC.currentIndex(model.lines, at: position + offset + model.songOffset)
            VStack(alignment: .leading, spacing: 8) {
                header
                Group {
                    if model.lines.isEmpty {
                        status
                    } else {
                        lyricLines(active: active)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                if let track = model.track, track.duration.isFinite, track.duration > 0 {
                    progress(position / track.duration)
                }
            }
            .padding(.horizontal, 17)
            .padding(.vertical, 12)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(base.opacity(opacity), in: RoundedRectangle(cornerRadius: 18))
        .overlay {
            RoundedRectangle(cornerRadius: 18)
                .strokeBorder(.white.opacity(opacity * 0.15), lineWidth: 1)
                .allowsHitTesting(false)
        }
        .preferredColorScheme(.dark)
        .opacity(overlayOpacity)
    }

    private var header: some View {
        HStack(spacing: 9) {
            Button(action: showSettings) {
                Image(systemName: "waveform")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(accent)
                    .frame(width: 28, height: 28)
                    .background(accent.opacity(0.13), in: RoundedRectangle(cornerRadius: 7))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Einstellungen")
            .help("Einstellungen und Timing öffnen")

            if let track = model.track {
                VStack(alignment: .leading, spacing: 2) {
                    Text(track.title)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(.white)
                    Text([track.artist, model.activeSource?.title].compactMap { $0 }.joined(separator: " · "))
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.white.opacity(0.69))
                }
                .lineLimit(1)
            } else {
                Text("Lyrics Overlay")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white)
            }

            Spacer(minLength: 4)
            if model.state == .synced(local: true) {
                Label("Eigene LRC", systemImage: "doc.text")
                    .labelStyle(.iconOnly)
                    .foregroundStyle(accent)
                    .help("Eigene LRC-Datei")
            }
            if model.track != nil && !model.isPlaying {
                Image(systemName: "pause.fill")
                    .foregroundStyle(.white.opacity(0.75))
                    .accessibilityLabel("Pausiert")
            }
        }
        .font(.system(size: 11, weight: .semibold))
    }

    private var status: some View {
        HStack(spacing: 9) {
            if model.state == .loading {
                ProgressView().controlSize(.small)
            }
            Text(model.state.message ?? "")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(.white.opacity(0.84))
                .lineLimit(3)
            if model.state.canRetry {
                Button {
                    model.reloadLyrics(ignoringCache: true)
                } label: {
                    Image(systemName: "arrow.clockwise")
                        .frame(width: 26, height: 26)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(accent)
                .help("Lyrics erneut laden")
                .accessibilityLabel("Lyrics erneut laden")
            }
        }
    }

    private func lyricLines(active: Int?) -> some View {
        let previous = active.map { $0 - 1 }
        let next = (active ?? -1) + 1
        let current = active.map { model.lines[$0].text }
        let currentLabel = current.map { $0.isEmpty ? "Instrumentalpassage" : $0 } ?? "Intro"
        let showContext = layout == .context
        let showNext = layout != .caption
        return VStack(alignment: .leading, spacing: 5) {
            if showContext, let previous, model.lines.indices.contains(previous) {
                Text(text(at: previous))
                    .font(.system(size: max(11, fontSize * 0.55), weight: .medium))
                    .foregroundStyle(.white.opacity(0.50))
                    .lineLimit(1)
            }

            Text(active.map { text(at: $0) } ?? "♪")
                .font(.system(size: fontSize, weight: .semibold))
                .foregroundStyle(.white)
                .lineLimit(2)
                .minimumScaleFactor(0.55)
                .layoutPriority(1)
                .accessibilityLabel(currentLabel)

            if showNext, model.lines.indices.contains(next) {
                Text(text(at: next))
                    .font(.system(size: max(12, fontSize * 0.63), weight: .medium))
                    .foregroundStyle(.white.opacity(0.70))
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .shadow(color: .black.opacity(0.75), radius: 3, x: 0, y: 1)
    }

    private func text(at index: Int) -> String {
        let text = model.lines[index].text
        return text.isEmpty ? "♪" : text
    }

    private func progress(_ fraction: Double) -> some View {
        GeometryReader { geometry in
            ZStack(alignment: .leading) {
                Capsule().fill(.white.opacity(0.16))
                Capsule().fill(accent)
                    .frame(width: geometry.size.width * CGFloat(min(1, max(0, fraction))))
            }
        }
        .frame(height: 2)
        .accessibilityHidden(true)
    }
}
