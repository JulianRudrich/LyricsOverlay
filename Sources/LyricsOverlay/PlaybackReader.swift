import AppKit
import CryptoKit

struct PlaybackSnapshot {
    let source: PlaybackSource
    let track: Track
    let position: TimeInterval
    let sampledAt: TimeInterval
    let isPlaying: Bool
}

enum PlaybackPoll {
    case snapshot(PlaybackSnapshot)
    case waiting(String)
}

/// Used exclusively on PlayerModel's serial reader queue.
final class PlaybackReader {
    private enum Reading {
        case idle
        case snapshot(PlaybackSnapshot)
        case unavailable(String, playing: Bool?)

        var activity: PlaybackActivity {
            switch self {
            case .idle: return .idle
            case .snapshot(let snapshot): return snapshot.isPlaying ? .playing : .paused
            case .unavailable(_, playing: true): return .playingWithoutTiming
            case .unavailable: return .unknown
            }
        }
    }

    private var scripts: [PlaybackSource: NSAppleScript] = [:]
    private var selection = PlaybackSelection()
    private var previousPreference: PlayerPreference?

    func poll(_ preference: PlayerPreference) -> PlaybackPoll {
        if previousPreference != preference {
            selection = PlaybackSelection()
            previousPreference = preference
        }

        var readings: [PlaybackSource: Reading] = [:]
        for source in preference.sources {
            readings[source] = read(source)
        }

        let selected = preference.source ?? selection.choose(from: readings.mapValues(\.activity))
        if let selected, let reading = readings[selected] {
            switch reading {
            case .snapshot(let snapshot): return .snapshot(snapshot)
            case .unavailable(let message, _): return .waiting(message)
            case .idle: break
            }
        }

        for source in preference.sources {
            if case .unavailable(let message, _) = readings[source] {
                return .waiting(message)
            }
        }
        return .waiting(preference.idleMessage)
    }

    private func read(_ source: PlaybackSource) -> Reading {
        let bundleID = source == .spotify ? "com.spotify.client" : "com.apple.Music"
        guard !NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).isEmpty else {
            return .idle
        }
        var error: NSDictionary?
        guard let result = script(for: source, bundleID: bundleID)?.executeAndReturnError(&error), error == nil else {
            return failure(source, code: error?[NSAppleScript.errorNumber] as? Int, playing: nil)
        }

        let sampledAt = ProcessInfo.processInfo.systemUptime
        let kind = result.atIndex(1)?.stringValue
        if kind == "stopped" { return .idle }
        let playing = result.atIndex(2)?.booleanValue
        if kind == "unavailable" {
            let code = result.atIndex(3).map { Int($0.int32Value) }
            return failure(source, code: code, playing: playing)
        }
        guard kind == "snapshot", result.numberOfItems == 8,
              let playing,
              let title = result.atIndex(4)?.stringValue, !title.isEmpty,
              let artist = result.atIndex(5)?.stringValue, !artist.isEmpty,
              let rawDuration = result.atIndex(6)?.doubleValue, rawDuration.isFinite, rawDuration > 0,
              let position = result.atIndex(7)?.doubleValue, position.isFinite, position >= 0 else {
            return failure(source, code: nil, playing: playing)
        }

        let duration = source == .spotify ? rawDuration / 1_000 : rawDuration
        let album = result.atIndex(8)?.stringValue.flatMap { $0.isEmpty ? nil : $0 }
        let identifier = result.atIndex(3)?.stringValue ?? ""
        guard let id = trackID(for: source, identifier: identifier, title: title,
                               artist: artist, album: album, duration: duration) else {
            return failure(source, code: nil, playing: playing)
        }
        let track = Track(id: id, title: title, artist: artist, duration: duration, album: album)
        return .snapshot(PlaybackSnapshot(source: source, track: track, position: position,
                                          sampledAt: sampledAt, isPlaying: playing))
    }

    private func script(for source: PlaybackSource, bundleID: String) -> NSAppleScript? {
        if let script = scripts[source] { return script }
        let identifier = source == .spotify ? "id" : "persistent ID"
        let script = NSAppleScript(source: """
        with timeout of 2 seconds
            tell application id "\(bundleID)"
                set playbackState to player state
                if playbackState is stopped then return {"stopped"}
                set playingNow to (playbackState is playing)
                try
                    set currentSong to current track
                    set trackKey to ""
                    try
                        set trackKey to (\(identifier) of currentSong) as text
                    end try
                    set albumName to ""
                    try
                        set albumValue to album of currentSong
                        if albumValue is not missing value then set albumName to albumValue as text
                    end try
                    set songTitle to name of currentSong
                    set songArtist to artist of currentSong
                    set songDuration to duration of currentSong
                    set songPosition to player position
                    if songTitle is missing value or songArtist is missing value or songDuration is missing value or songPosition is missing value then return {"unavailable", playingNow, 0}
                    return {"snapshot", playingNow, trackKey, songTitle as text, songArtist as text, songDuration as real, songPosition as real, albumName}
                on error errorMessage number errorNumber
                    return {"unavailable", playingNow, errorNumber}
                end try
            end tell
        end timeout
        """)
        scripts[source] = script
        return script
    }

    private func failure(_ source: PlaybackSource, code: Int?, playing: Bool?) -> Reading {
        if code == -600 { return .idle } // The player closed between detection and the Apple event.
        let message: String
        if code == -1743 {
            message = "Erlaube den Zugriff auf \(source.title) unter Datenschutz & Sicherheit → Automation."
        } else if source == .appleMusic, code == -1728 || code == 0 || (code == nil && playing != nil) {
            message = "Apple Music liefert keine Songdaten. Spiele den Titel direkt aus deiner Mediathek."
        } else {
            message = "\(source.title) konnte gerade nicht gelesen werden."
        }
        return .unavailable(message, playing: playing)
    }

    private func trackID(for source: PlaybackSource, identifier: String, title: String,
                         artist: String, album: String?, duration: TimeInterval) -> String? {
        if source == .spotify {
            // Existing imports and offsets already use the unmodified Spotify identifier.
            return identifier.isEmpty ? nil : identifier
        }
        if !identifier.isEmpty {
            return "apple-music:id:\(identifier)"
        }
        let fields = [title, artist, album ?? "", String((duration * 1_000).rounded())]
            .map(\.precomposedStringWithCanonicalMapping)
        let signature = fields.map { "\($0.utf8.count):\($0)" }.joined()
        let hash = SHA256.hash(data: Data(signature.utf8)).map { String(format: "%02x", $0) }.joined()
        return "apple-music:metadata:\(hash)"
    }
}
