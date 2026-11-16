import Foundation

public struct PlaybackClock {
    private let position: TimeInterval
    private let sampledAt: TimeInterval
    private let isPlaying: Bool

    public init(position: TimeInterval, sampledAt: TimeInterval, isPlaying: Bool) {
        self.position = position.isFinite ? max(0, position) : 0
        self.sampledAt = sampledAt
        self.isPlaying = isPlaying
    }

    /// Use the same monotonic clock for `sampledAt` and `time`.
    public func position(at time: TimeInterval, duration: TimeInterval) -> TimeInterval {
        let elapsed = time - sampledAt
        let advance = isPlaying && elapsed.isFinite ? max(0, elapsed) : 0
        let estimated = position + advance
        if duration.isFinite && duration > 0 {
            return min(estimated, duration)
        }
        return estimated.isFinite ? estimated : position
    }
}
