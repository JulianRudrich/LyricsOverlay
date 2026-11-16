import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import LyricsCore

struct LyricsQuery: Hashable, Sendable {
    let title: String
    let artist: String
    let duration: Double
    var album: String? = nil
}

enum LyricsLookupResult: Equatable, Sendable {
    case synced([LyricLine])
    case instrumental
    case unavailable
}

protocol LyricsLookingUp: Sendable {
    func lookup(_ query: LyricsQuery, ignoringCache: Bool) async throws -> LyricsLookupResult
}

extension LyricsLookingUp {
    func lookup(_ query: LyricsQuery) async throws -> LyricsLookupResult {
        try await lookup(query, ignoringCache: false)
    }
}

enum LRCLIBLookupError: LocalizedError {
    case invalidResponse
    case httpStatus(Int)
    case retryLater

    var errorDescription: String? {
        switch self {
        case .invalidResponse: return "LRCLIB hat eine ungültige Antwort gesendet."
        case .httpStatus(let code): return "LRCLIB ist gerade nicht verfügbar (HTTP \(code))."
        case .retryLater: return "LRCLIB ist gerade ausgelastet. Versuche es später noch einmal."
        }
    }
}

actor LRCLIBLyricsLookup: LyricsLookingUp {
    private struct CacheEntry {
        let result: LyricsLookupResult
        let expires: ContinuousClock.Instant
        var lastAccess: ContinuousClock.Instant
    }

    private let session: URLSession
    private let clock = ContinuousClock()
    private let capacity = 64
    private var cache: [LyricsQuery: CacheEntry] = [:]
    private var retryAfter: ContinuousClock.Instant?

    init(session: URLSession = URLSession(configuration: .ephemeral)) {
        self.session = session
    }

    func lookup(_ query: LyricsQuery, ignoringCache: Bool) async throws -> LyricsLookupResult {
        try Task.checkCancellation()
        let title = query.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let artist = query.artist.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, !artist.isEmpty, query.duration.isFinite, query.duration > 0 else {
            return .unavailable
        }

        let now = clock.now
        if !ignoringCache, let entry = cache[query], now < entry.expires {
            cache[query]?.lastAccess = now
            return entry.result
        }

        let result = try await fetchLyrics(for: query, title: title, artist: artist)
        try Task.checkCancellation()
        let storedAt = clock.now
        cache = cache.filter { $0.value.expires > storedAt }
        if cache[query] == nil, cache.count >= capacity,
           let leastRecent = cache.min(by: { $0.value.lastAccess < $1.value.lastAccess }) {
            cache.removeValue(forKey: leastRecent.key)
        }
        let lifetime: Duration = result == .unavailable ? .seconds(120) : .seconds(3_600)
        cache[query] = CacheEntry(result: result, expires: storedAt.advanced(by: lifetime), lastAccess: storedAt)
        return result
    }

    private func fetchLyrics(for query: LyricsQuery, title: String, artist: String) async throws -> LyricsLookupResult {
        let album = query.album?.trimmingCharacters(in: .whitespacesAndNewlines)
        let signature = [URLQueryItem(name: "track_name", value: title), URLQueryItem(name: "artist_name", value: artist)]
        if (1...3_600).contains(query.duration.rounded()) {
            var exact = signature + [URLQueryItem(name: "duration", value: String(Int(query.duration.rounded())))]
            if let album, !album.isEmpty {
                exact.append(URLQueryItem(name: "album_name", value: album))
            }
            if let data = try await fetch("/api/get", queryItems: exact, allowsMiss: true) {
                let record = try JSONDecoder().decode(LyricsResult.self, from: data)
                if LyricsMatching.matches(record, title: title, artist: artist, duration: query.duration),
                   abs(record.duration - query.duration) <= 2.5 {
                    if let lines = Self.parsedLyrics(record) { return .synced(lines) }
                    if record.instrumental == true,
                       Self.metadataEquals(record.artistName, artist),
                       Self.sameAlbum(record, album: album) {
                        return .instrumental
                    }
                }
            }
            try await Task.sleep(for: .milliseconds(200))
        }

        guard let data = try await fetch("/api/search", queryItems: signature) else {
            return .unavailable
        }
        let records = try JSONDecoder().decode([LyricsResult].self, from: data)
        let sameAlbum = records.filter { Self.sameAlbum($0, album: album) }
        let best = LyricsMatching.best(sameAlbum, title: title, artist: artist, duration: query.duration)
            ?? LyricsMatching.best(records, title: title, artist: artist, duration: query.duration)
        guard let best, let lines = Self.parsedLyrics(best) else { return .unavailable }
        return .synced(lines)
    }

    private static func parsedLyrics(_ record: LyricsResult) -> [LyricLine]? {
        guard let lrc = record.syncedLyrics else { return nil }
        let lines = LRC.parse(lrc)
        return lines.contains { !$0.text.isEmpty } ? lines : nil
    }

    private static func sameAlbum(_ record: LyricsResult, album: String?) -> Bool {
        guard let album, !album.isEmpty else { return true }
        return record.albumName.map { metadataEquals($0, album) } ?? false
    }

    private static func metadataEquals(_ left: String, _ right: String) -> Bool {
        left.trimmingCharacters(in: .whitespacesAndNewlines).compare(
            right, options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "en_US_POSIX")
        ) == .orderedSame
    }

    private func fetch(_ path: String, queryItems: [URLQueryItem], allowsMiss: Bool = false) async throws -> Data? {
        try Task.checkCancellation()
        if let retryAfter, clock.now < retryAfter { throw LRCLIBLookupError.retryLater }

        var components = URLComponents()
        components.scheme = "https"
        components.host = "lrclib.net"
        components.path = path
        components.queryItems = queryItems
        // LRCLIB decodes form-style query strings, where a literal plus must be escaped.
        components.percentEncodedQuery = components.percentEncodedQuery?.replacingOccurrences(of: "+", with: "%2B")
        guard let url = components.url else { throw LRCLIBLookupError.invalidResponse }

        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 12)
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev"
        request.setValue("LyricsOverlay/\(version) (https://github.com/JulianRudrich/LyricsOverlay)", forHTTPHeaderField: "User-Agent")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await session.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw LRCLIBLookupError.invalidResponse }
        switch http.statusCode {
        case 200: return data
        case 404 where allowsMiss: return nil
        case 429, 503:
            let fallback: UInt32 = http.statusCode == 429 ? 30 : 1
            let seconds = http.value(forHTTPHeaderField: "Retry-After")
                .flatMap { UInt32($0.trimmingCharacters(in: .whitespaces)) } ?? fallback
            let deadline = clock.now.advanced(by: .seconds(Double(seconds) * Double.random(in: 1...1.25)))
            retryAfter = max(retryAfter ?? deadline, deadline)
            throw LRCLIBLookupError.retryLater
        default: throw LRCLIBLookupError.httpStatus(http.statusCode)
        }
    }
}
