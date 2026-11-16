import Foundation

public struct LyricLine: Equatable, Sendable {
    public let time: TimeInterval
    public let text: String

    public init(time: TimeInterval, text: String) {
        self.time = time
        self.text = text
    }
}

public struct LRCTimestampEdit: Equatable {
    public let range: NSRange
    public let replacement: String
    public let nextCaret: Int
}

public enum LRC {
    private static let timestamp = try! NSRegularExpression(pattern: #"\[([0-9]{1,3}):([0-9]{2}(?:\.[0-9]+)?)\]"#)
    private static let offsetTag = try! NSRegularExpression(
        pattern: #"^\[offset:\s*([+-]?[0-9]+)\s*\]$"#,
        options: .caseInsensitive
    )
    private static let wordTimestamp = try! NSRegularExpression(pattern: #"<[0-9]{1,3}:[0-9]{2}(?:\.[0-9]+)?>"#)
    private static let metadataTag = try! NSRegularExpression(pattern: #"^\[[A-Za-z][A-Za-z0-9_-]*:[^\]]*\]$"#)
    private static let linePadding = CharacterSet.whitespaces.union(CharacterSet(charactersIn: "\u{FEFF}"))

    public static func parse(_ input: String) -> [LyricLine] {
        var lines: [LyricLine] = []
        var offset: TimeInterval = 0
        for raw in input.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: linePadding)
            let ns = line as NSString
            let range = NSRange(location: 0, length: ns.length)
            if let value = parseOffset(in: line, range: range) {
                offset = value
                continue
            }

            var end = 0
            var times: [TimeInterval] = []
            for match in timestamp.matches(in: line, range: range) {
                // A timestamp inside the text belongs to the lyric, not the timing prefix.
                let gap = NSRange(location: end, length: match.range.location - end)
                guard ns.substring(with: gap).trimmingCharacters(in: .whitespaces).isEmpty else { break }
                end = NSMaxRange(match.range)
                guard let minutes = Double(ns.substring(with: match.range(at: 1))),
                      let seconds = Double(ns.substring(with: match.range(at: 2))), seconds < 60 else { continue }
                times.append(minutes * 60 + seconds)
            }
            guard !times.isEmpty else { continue }

            let lyric = ns.substring(from: end)
            let text = wordTimestamp.stringByReplacingMatches(
                in: lyric, range: NSRange(location: 0, length: lyric.utf16.count), withTemplate: ""
            ).trimmingCharacters(in: .whitespaces)
            lines.append(contentsOf: times.map { LyricLine(time: $0, text: text) })
        }
        // Positive LRC offsets advance the text. Keep pre-roll times so a user offset can delay them again.
        return lines.sorted { $0.time < $1.time }.map { LyricLine(time: $0.time - offset, text: $0.text) }
    }

    private static func parseOffset(in line: String, range: NSRange) -> TimeInterval? {
        guard let tag = offsetTag.firstMatch(in: line, range: range),
              let milliseconds = Int64((line as NSString).substring(with: tag.range(at: 1))) else { return nil }
        return Double(milliseconds) / 1_000
    }

    /// Stamps the line containing an editor caret at the effective lyric position.
    /// The caller supplies the playback position with the overlay corrections applied.
    public static func timestampEdit(in draft: String, caret: Int, at position: TimeInterval) -> LRCTimestampEdit? {
        let text = draft as NSString
        guard position.isFinite, (0...text.length).contains(caret) else { return nil }

        var start = 0
        var end = 0
        var contentsEnd = 0
        text.getLineStart(&start, end: &end, contentsEnd: &contentsEnd,
                          for: NSRange(location: caret, length: 0))
        let rawLine = text.substring(with: NSRange(location: start, length: contentsEnd - start))
        let line = rawLine.trimmingCharacters(in: linePadding)
        let lineRange = NSRange(location: 0, length: line.utf16.count)
        guard metadataTag.firstMatch(in: line, range: lineRange) == nil else { return nil }

        let rawText = rawLine as NSString
        let leading = rawText.rangeOfCharacter(from: linePadding.inverted)
        let paddingLength = leading.location == NSNotFound ? rawText.length : leading.location
        let token = timestamp.firstMatch(in: line, options: .anchored, range: lineRange)
        let range = NSRange(location: start + paddingLength, length: token?.range.length ?? 0)
        var fileOffset: TimeInterval = 0
        for raw in draft.components(separatedBy: .newlines) {
            let line = raw.trimmingCharacters(in: linePadding)
            if let value = parseOffset(in: line, range: NSRange(location: 0, length: line.utf16.count)) {
                fileOffset = value
            }
        }

        let limit = 999 * 60 * 100 + 59 * 100 + 99
        let centiseconds = Int((min(Double(limit) / 100, max(0, position + fileOffset)) * 100).rounded())
        let replacement = String(format: "[%02d:%02d.%02d]", centiseconds / 6_000,
                                 (centiseconds / 100) % 60, centiseconds % 100)
        return LRCTimestampEdit(range: range, replacement: replacement,
                                 nextCaret: end + replacement.utf16.count - range.length)
    }

    /// Finds the active line or instrumental marker in ascending timestamp order.
    public static func currentIndex(_ lines: [LyricLine], at position: TimeInterval) -> Int? {
        var lower = lines.startIndex
        var upper = lines.endIndex
        while lower < upper {
            let middle = lower + (upper - lower) / 2
            if lines[middle].time <= position {
                lower = middle + 1
            } else {
                upper = middle
            }
        }
        return lower == lines.startIndex ? nil : lower - 1
    }
}

public struct LyricsResult: Decodable {
    public let trackName: String
    public let artistName: String
    public let duration: Double
    public let syncedLyrics: String?
    public let instrumental: Bool?
    public let albumName: String?
}

public enum LyricsMatching {
    private static func normalized(_ value: String) -> String {
        value.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
            .components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }.joined(separator: " ")
    }

    public static func matches(_ result: LyricsResult, title: String, artist: String, duration: Double) -> Bool {
        let title = normalized(title)
        let artist = normalized(artist)
        let candidateArtist = normalized(result.artistName)
        return !title.isEmpty && !artist.isEmpty && normalized(result.trackName) == title &&
            (candidateArtist == artist || candidateArtist.hasPrefix(artist + " ")) &&
            abs(result.duration - duration) <= 5
    }

    public static func best(_ results: [LyricsResult], title: String, artist: String, duration: Double) -> LyricsResult? {
        results.filter { result in
            guard matches(result, title: title, artist: artist, duration: duration),
                  let lyrics = result.syncedLyrics else { return false }
            return LRC.parse(lyrics).contains { !$0.text.isEmpty }
        }.min { abs($0.duration - duration) < abs($1.duration - duration) }
    }
}
