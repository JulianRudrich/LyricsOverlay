import AppKit
import Combine
import LyricsCore
import UniformTypeIdentifiers

struct Track: Equatable {
    let id: String
    let title: String
    let artist: String
    let duration: Double
    var album: String? = nil
}

enum LyricsState: Equatable {
    case idle(String)
    case loading
    case synced(local: Bool)
    case instrumental
    case unavailable
    case offline
    case lookupFailed
    case rateLimited
    case localOnly

    var message: String? {
        switch self {
        case .idle(let message): return message
        case .loading: return "Suche synchronisierte Lyrics …"
        case .synced: return nil
        case .instrumental: return "Instrumentalstück"
        case .unavailable: return "Keine synchronisierten Lyrics gefunden."
        case .offline: return "Offline. Eigene LRC-Dateien bleiben verfügbar."
        case .lookupFailed: return "Lyrics konnten gerade nicht geladen werden."
        case .rateLimited: return "LRCLIB ist ausgelastet. Versuch es gleich erneut."
        case .localOnly: return "Onlinesuche ist aus. Du kannst eine LRC-Datei importieren."
        }
    }

    var canRetry: Bool {
        self == .unavailable || self == .offline || self == .lookupFailed || self == .rateLimited
    }
}

private enum LocalLyricsError: LocalizedError {
    case demo
    case tooLarge

    var errorDescription: String? {
        switch self {
        case .demo: return "Die Beispiel-Lyrics lassen sich nicht speichern."
        case .tooLarge: return "Die LRC-Datei darf höchstens 512 KB groß sein."
        }
    }
}

final class PlayerModel: ObservableObject {
    @Published private(set) var track: Track?
    @Published private(set) var lines: [LyricLine] = []
    @Published private(set) var state: LyricsState = .idle(PlayerPreference.automatic.idleMessage)
    @Published private(set) var clock = PlaybackClock(position: 0, sampledAt: 0, isPlaying: false)
    @Published private(set) var isPlaying = false
    @Published private(set) var songOffset: Double = 0
    @Published private(set) var activeSource: PlaybackSource?

    let isDemo: Bool
    private let lookup: any LyricsLookingUp
    private let trackStore = TrackLyricsStore()
    private static let localLyricsByteLimit = 512 * 1024
    private let reader = PlaybackReader()
    private var preference = PlayerPreference.automatic
    private var preferenceRevision = 0
    private var request: Task<Void, Never>?
    private var pending = false
    private let queue = DispatchQueue(label: "playback-reader")
    private var timer: Timer?

    init(isDemo: Bool, lookup: any LyricsLookingUp = LRCLIBLyricsLookup()) {
        self.isDemo = isDemo
        self.lookup = lookup
    }

    func start() {
        if isDemo {
            track = Track(id: "demo", title: "Nachthimmel", artist: "Lyrics Overlay · Demo", duration: 24)
            lines = LRC.parse("""
            [00:00.00]Leise zieht die Nacht vorbei
            [00:04.00]Über Dächern, sacht und frei
            [00:08.00]In den Fenstern brennt noch Licht
            [00:12.00]Und der Himmel ändert sich
            [00:16.00]
            [00:20.00]Bis der erste Morgen spricht
            """)
            clock = PlaybackClock(position: 0, sampledAt: ProcessInfo.processInfo.systemUptime, isPlaying: true)
            isPlaying = true
            return
        }

        if let value = UserDefaults.standard.string(forKey: "playerPreference"),
           let stored = PlayerPreference(rawValue: value) {
            preference = stored
            state = .idle(stored.idleMessage)
        }
        poll()
        let timer = Timer(timeInterval: 0.6, repeats: true) { [weak self] _ in self?.poll() }
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    func position(at uptime: TimeInterval) -> TimeInterval {
        guard let track else { return 0 }
        if isDemo {
            return clock.position(at: uptime, duration: 0).truncatingRemainder(dividingBy: track.duration)
        }
        return clock.position(at: uptime, duration: track.duration)
    }

    func setPlayerPreference(_ value: PlayerPreference) {
        guard !isDemo, value != preference else { return }
        preference = value
        preferenceRevision &+= 1
        if activeSource.map({ !value.sources.contains($0) }) ?? true {
            clear(value.idleMessage)
        }
        poll()
    }

    private func poll() {
        guard !pending else { return }
        pending = true
        let preference = self.preference
        let revision = preferenceRevision
        queue.async { [weak self, reader] in
            let result = reader.poll(preference)
            DispatchQueue.main.async {
                guard let self else { return }
                self.pending = false
                guard self.preferenceRevision == revision else {
                    self.poll()
                    return
                }
                switch result {
                case .waiting(let message):
                    self.clear(message)
                case .snapshot(let snapshot):
                    self.update(snapshot)
                }
            }
        }
    }

    private func clear(_ message: String) {
        let idle = LyricsState.idle(message)
        guard track != nil || state != idle else { return }
        request?.cancel()
        request = nil
        track = nil
        activeSource = nil
        lines = []
        state = idle
        isPlaying = false
        songOffset = 0
    }

    private func update(_ snapshot: PlaybackSnapshot) {
        clock = PlaybackClock(position: snapshot.position, sampledAt: snapshot.sampledAt, isPlaying: snapshot.isPlaying)
        if activeSource != snapshot.source { activeSource = snapshot.source }
        if isPlaying != snapshot.isPlaying { isPlaying = snapshot.isPlaying }
        guard track != snapshot.track else { return }
        track = snapshot.track
        songOffset = trackStore.preferences(for: snapshot.track.id).timingOffset
        reloadLyrics()
    }

    func reloadLyrics(ignoringCache: Bool = false) {
        guard !isDemo else { return }
        request?.cancel()
        request = nil
        lines = []
        guard let track else { return }

        if let local = trackStore.preferences(for: track.id).localLyrics {
            let parsed = LRC.parse(local)
            if parsed.contains(where: { !$0.text.isEmpty }) {
                lines = parsed
                state = .synced(local: true)
                return
            }
        }
        guard UserDefaults.standard.bool(forKey: "lookupOnline") else {
            state = .localOnly
            return
        }

        state = .loading
        let query = LyricsQuery(title: track.title, artist: track.artist, duration: track.duration, album: track.album)
        request = Task { @MainActor [weak self, lookup] in
            do {
                let result = try await lookup.lookup(query, ignoringCache: ignoringCache)
                guard let self, !Task.isCancelled, self.track == track else { return }
                switch result {
                case .synced(let lines):
                    self.lines = lines
                    self.state = .synced(local: false)
                case .instrumental:
                    self.state = .instrumental
                case .unavailable:
                    self.state = .unavailable
                }
                self.request = nil
            } catch {
                guard let self, !Task.isCancelled, self.track == track else { return }
                let code = (error as? URLError)?.code
                if code == .notConnectedToInternet || code == .networkConnectionLost {
                    self.state = .offline
                } else if case .retryLater? = error as? LRCLIBLookupError {
                    self.state = .rateLimited
                } else {
                    self.state = .lookupFailed
                }
                self.request = nil
            }
        }
    }

    func adjustSongOffset(by delta: Double) {
        setSongOffset(songOffset + delta)
    }

    func setSongOffset(_ value: Double) {
        guard let track, !isDemo else { return }
        let offset = min(10, max(-10, value))
        trackStore.setTimingOffset(offset, for: track.id)
        songOffset = offset
    }

    func localLyricsText(for selectedTrack: Track) -> String? {
        guard !isDemo else { return nil }
        return trackStore.preferences(for: selectedTrack.id).localLyrics
    }

    func saveLocalLyrics(_ text: String, for selectedTrack: Track) throws {
        guard !isDemo else { throw LocalLyricsError.demo }
        guard text.utf8.count <= Self.localLyricsByteLimit else { throw LocalLyricsError.tooLarge }
        _ = try trackStore.importLyrics(text, for: selectedTrack.id)
        if track?.id == selectedTrack.id { reloadLyrics() }
    }

    func importLyrics(for selectedTrack: Track) -> String? {
        let panel = NSOpenPanel()
        panel.title = "LRC für \(selectedTrack.title) importieren"
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [UTType(filenameExtension: "lrc") ?? .plainText]
        guard panel.runModal() == .OK, let url = panel.url else { return nil }

        do {
            let file = try FileHandle(forReadingFrom: url)
            defer { try? file.close() }
            let limit = Self.localLyricsByteLimit
            guard let data = try file.read(upToCount: limit + 1), data.count <= limit,
                  let text = String(data: data, encoding: .utf8) else {
                return "Wähle eine UTF-8-kodierte LRC-Datei mit höchstens 512 KB."
            }
            try saveLocalLyrics(text, for: selectedTrack)
            return nil
        } catch TrackLyricsImportError.missingTimedLyrics {
            return "Die Datei konnte nicht importiert werden. Prüfe, ob sie mindestens eine Textzeile mit LRC-Zeitstempel enthält."
        } catch {
            return "Die LRC-Datei konnte nicht gelesen oder gespeichert werden."
        }
    }

    func removeLocalLyrics() -> String? {
        guard let track, !isDemo else { return nil }
        do {
            try trackStore.removeLocalLyrics(for: track.id)
            reloadLyrics()
            return nil
        } catch {
            return "Die eigene LRC-Datei konnte nicht entfernt werden."
        }
    }
}
