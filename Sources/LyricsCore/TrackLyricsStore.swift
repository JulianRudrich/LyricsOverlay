import Foundation

public struct TrackLyricsPreferences {
    public let timingOffset: TimeInterval
    public let localLyrics: String?
}

public enum TrackLyricsImportError: Error, Equatable {
    case missingTimedLyrics
    case lyricsTooLarge
    case storageUnavailable
}

public struct TrackLyricsStore {
    public static let maximumLyricsBytes = 512 * 1_024

    private let defaults: UserDefaults
    private let localLyricsDirectory: URL?

    public init(defaults: UserDefaults = .standard, localLyricsDirectory: URL? = nil) {
        self.defaults = defaults
        self.localLyricsDirectory = localLyricsDirectory ?? FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first?
            .appendingPathComponent("Lyrics Overlay", isDirectory: true)
            .appendingPathComponent("LRC", isDirectory: true)
    }

    public func preferences(for trackID: String) -> TrackLyricsPreferences {
        let offset = defaults.double(forKey: timingKey(for: trackID))
        let localLyrics = lyricsFile(for: trackID).flatMap { try? String(contentsOf: $0, encoding: .utf8) }
        return TrackLyricsPreferences(
            timingOffset: offset.isFinite ? offset : 0,
            localLyrics: localLyrics
        )
    }

    public func setTimingOffset(_ offset: TimeInterval, for trackID: String) {
        guard offset.isFinite else { return }
        let key = timingKey(for: trackID)
        if offset == 0 {
            defaults.removeObject(forKey: key)
        } else {
            defaults.set(offset, forKey: key)
        }
    }

    /// A failed import leaves the track's existing lyrics intact.
    @discardableResult
    public func importLyrics(_ input: String, for trackID: String) throws -> [LyricLine] {
        guard input.utf8.count <= Self.maximumLyricsBytes else { throw TrackLyricsImportError.lyricsTooLarge }
        let lines = LRC.parse(input)
        guard lines.contains(where: { !$0.text.isEmpty }) else {
            throw TrackLyricsImportError.missingTimedLyrics
        }
        guard let file = lyricsFile(for: trackID) else { throw TrackLyricsImportError.storageUnavailable }
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(input.utf8).write(to: file, options: .atomic)
        return lines
    }

    public func removeLocalLyrics(for trackID: String) throws {
        guard let file = lyricsFile(for: trackID) else { throw TrackLyricsImportError.storageUnavailable }
        do {
            try FileManager.default.removeItem(at: file)
        } catch let error as CocoaError where error.code == .fileNoSuchFile {
            return
        }
    }

    private func timingKey(for trackID: String) -> String {
        "trackTimingOffset.\(trackID)"
    }

    private func lyricsFile(for trackID: String) -> URL? {
        guard let directory = localLyricsDirectory, !trackID.isEmpty else { return nil }
        let bytes = Array(trackID.utf8)
        let chunkSize = 64
        // Hex keeps IDs distinct on case-insensitive volumes. Chunks keep long local Spotify IDs below filename limits.
        return stride(from: 0, to: bytes.count, by: chunkSize).reduce(into: directory) { path, start in
            let end = min(start + chunkSize, bytes.count)
            let part = bytes[start..<end].map { String(format: "%02x", $0) }.joined()
            let last = end == bytes.count
            path.appendPathComponent(last ? part + ".lrc" : part, isDirectory: !last)
        }
    }
}
