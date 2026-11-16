enum PlaybackSource: String, CaseIterable, Sendable {
    case spotify
    case appleMusic

    var title: String {
        switch self {
        case .spotify: return "Spotify"
        case .appleMusic: return "Apple Music"
        }
    }
}

enum PlayerPreference: String, CaseIterable, Identifiable, Sendable {
    case automatic
    case spotify
    case appleMusic

    var id: Self { self }
    var source: PlaybackSource? { PlaybackSource(rawValue: rawValue) }
    var sources: [PlaybackSource] { source.map { [$0] } ?? PlaybackSource.allCases }
    var title: String { source?.title ?? "Automatisch" }

    var idleMessage: String {
        switch self {
        case .automatic: return "Spiele einen Song in Spotify oder Apple Music ab."
        case .spotify, .appleMusic: return "Spiele einen Song in \(title) ab."
        }
    }
}

enum PlaybackActivity: Equatable {
    case idle
    case playing
    case paused
    case playingWithoutTiming
    case unknown
}

struct PlaybackSelection {
    private var current: PlaybackSource?
    private var previouslyPlaying: Set<PlaybackSource> = []

    mutating func choose(from activity: [PlaybackSource: PlaybackActivity]) -> PlaybackSource? {
        let sources = PlaybackSource.allCases
        let playing = sources.filter { activity[$0] == .playing }
        let blocked = sources.filter { activity[$0] == .playingWithoutTiming }
        let paused = sources.filter { activity[$0] == .paused }
        let newlyPlaying = playing.first { $0 != current && !previouslyPlaying.contains($0) }
        let selected = newlyPlaying ?? preferCurrent(in: playing) ?? preferCurrent(in: blocked) ?? preferCurrent(in: paused)

        for source in sources {
            switch activity[source] {
            case .playing, .playingWithoutTiming: previouslyPlaying.insert(source)
            case .idle, .paused: previouslyPlaying.remove(source)
            case .unknown, .none: break
            }
        }
        current = selected
        return selected
    }

    private func preferCurrent(in sources: [PlaybackSource]) -> PlaybackSource? {
        if let current, sources.contains(current) { return current }
        return sources.first
    }
}
