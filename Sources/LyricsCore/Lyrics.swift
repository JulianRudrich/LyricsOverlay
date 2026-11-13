import Foundation

public struct LyricLine: Equatable {
    public let time: TimeInterval
    public let text: String
    public init(time: TimeInterval, text: String) { self.time = time; self.text = text }
}

public enum LRC {
    public static func parse(_ input: String) -> [LyricLine] {
        let pattern = #"\[(\d{1,3}):(\d{2}(?:\.\d+)?)\]"#
        let regex = try! NSRegularExpression(pattern: pattern)
        var lines: [LyricLine] = []
        for raw in input.components(separatedBy: .newlines) {
            let ns = raw as NSString
            let matches = regex.matches(in: raw, range: NSRange(location: 0, length: ns.length))
            guard let last = matches.last else { continue }
            let text = ns.substring(from: last.range.location + last.range.length).trimmingCharacters(in: .whitespacesAndNewlines)
            for match in matches {
                guard let minutes = Double(ns.substring(with: match.range(at: 1))),
                      let seconds = Double(ns.substring(with: match.range(at: 2))), seconds < 60 else { continue }
                lines.append(LyricLine(time: minutes * 60 + seconds, text: text))
            }
        }
        return lines.sorted { $0.time < $1.time }
    }

    public static func currentIndex(_ lines: [LyricLine], at position: TimeInterval) -> Int? {
        lines.lastIndex { $0.time <= position }
    }
}

public struct LyricsResult: Decodable {
    public let trackName: String
    public let artistName: String
    public let duration: Double
    public let syncedLyrics: String?
}

public enum LyricsMatching {
    private static func normalized(_ value: String) -> String {
        value.folding(options: [.diacriticInsensitive, .caseInsensitive], locale: Locale(identifier: "en_US_POSIX"))
            .components(separatedBy: CharacterSet.alphanumerics.inverted).filter { !$0.isEmpty }.joined(separator: " ")
    }
    public static func best(_ results: [LyricsResult], title: String, artist: String, duration: Double) -> LyricsResult? {
        let title = normalized(title), artist = normalized(artist)
        return results.filter { result in
            let candidateArtist = normalized(result.artistName)
            return !title.isEmpty && !artist.isEmpty && normalized(result.trackName) == title &&
                (candidateArtist == artist || candidateArtist.hasPrefix(artist + " ")) &&
                abs(result.duration - duration) <= 5 &&
                !(result.syncedLyrics ?? "").isEmpty && !LRC.parse(result.syncedLyrics ?? "").isEmpty
        }.min { abs($0.duration - duration) < abs($1.duration - duration) }
    }
}
