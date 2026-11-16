import Foundation
import LyricsCore

let lines = LRC.parse("[00:03.50] Second\n[00:01.00] First\n[00:07.00][00:09.10] Again\n[00:12.0] \n[00:65.00] Invalid")
precondition(lines.map(\.time) == [1, 3.5, 7, 9.1, 12])
precondition(lines.map(\.text) == ["First", "Second", "Again", "Again", ""])
precondition(LRC.currentIndex(lines, at: 0.99) == nil)
precondition(LRC.currentIndex(lines, at: 1) == 0)
precondition(LRC.currentIndex(lines, at: 8) == 2)
precondition(LRC.currentIndex(lines, at: 12) == 4)

let imported = "\u{FEFF}[ar:Example]\r\n[00:00.25] First\r\n[00:01.00] [00:03.00] <00:01.00>Second <00:01.50>line\r\n[00:02.00]\r\n [OFFSET: +500] "
let shifted = LRC.parse(imported)
precondition(shifted.map(\.time) == [-0.25, 0.5, 1.5, 2.5], "LRC offset and repeated timestamps")
precondition(shifted.map(\.text) == ["First", "Second line", "", "Second line"], "Enhanced tags and instrumental marker")
precondition(LRC.currentIndex(shifted, at: 1.5) == 2, "Blank timestamp takes effect as soon as it is reached")
precondition(LRC.currentIndex(shifted, at: 0) == 0, "A positive LRC offset can advance a line into pre-roll")
precondition(LRC.parse("[offset:-100]\n[00:00.00] Later").first?.time == 0.1)
precondition(LRC.parse("[00:01] Mention [00:04] here").first?.text == "Mention [00:04] here")
precondition(LRC.currentIndex(LRC.parse("[00:01] First\n[00:01] Second"), at: 1) == 1)
precondition(LRC.currentIndex([], at: 0) == nil)

let draft = "[offset:500]\r\nMorgenlicht 🌙\r\nNächste Zeile"
let draftText = draft as NSString
let firstRow = draftText.range(of: "Morgenlicht 🌙")
let stamp = LRC.timestampEdit(in: draft, caret: NSMaxRange(firstRow), at: 59.999)!
let editedDraft = draftText.replacingCharacters(in: stamp.range, with: stamp.replacement)
precondition(editedDraft == "[offset:500]\r\n[01:00.50]Morgenlicht 🌙\r\nNächste Zeile")
precondition(LRC.parse(editedDraft).first?.time == 60, "Stamped time includes the LRC offset")
precondition(stamp.nextCaret == (editedDraft as NSString).range(of: "Nächste Zeile").location)
precondition(LRC.timestampEdit(in: draft, caret: 0, at: 3) == nil, "Metadata is left intact")
precondition(LRC.timestampEdit(in: draft, caret: 0, at: .nan) == nil)

let repeatDraft = "  [00:02][00:08]Wieder"
let repeatedStamp = LRC.timestampEdit(in: repeatDraft, caret: repeatDraft.utf16.count, at: 3.25)!
let correctedRepeat = (repeatDraft as NSString).replacingCharacters(in: repeatedStamp.range,
                                                                   with: repeatedStamp.replacement)
precondition(correctedRepeat == "  [00:03.25][00:08]Wieder")
precondition(repeatedStamp.nextCaret == correctedRepeat.utf16.count)
precondition(LRC.timestampEdit(in: "", caret: 0, at: 0)?.replacement == "[00:00.00]")
precondition(LRC.timestampEdit(in: "A\n", caret: 2, at: 3)?.range == NSRange(location: 2, length: 0))

let playing = PlaybackClock(position: 10, sampledAt: 100, isPlaying: true)
precondition(playing.position(at: 100.25, duration: 20) == 10.25)
precondition(playing.position(at: 99, duration: 20) == 10, "An earlier reading does not rewind the anchor")
precondition(playing.position(at: 115, duration: 20) == 20, "The estimate stops at the track duration")
precondition(playing.position(at: 105, duration: 0) == 15, "Unknown duration does not cap playback")
let paused = PlaybackClock(position: 8, sampledAt: 101, isPlaying: false)
precondition(paused.position(at: 105, duration: 20) == 8, "Pausing or seeking uses the new snapshot")
let invalid = PlaybackClock(position: .nan, sampledAt: .infinity, isPlaying: true)
precondition(invalid.position(at: 10, duration: .nan) == 0)

do {
    let domain = "LyricsOverlay.checks.\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: domain)!
    let lyricsDirectory = FileManager.default.temporaryDirectory.appendingPathComponent(domain, isDirectory: true)
    defer {
        defaults.removePersistentDomain(forName: domain)
        try? FileManager.default.removeItem(at: lyricsDirectory)
    }
    let store = TrackLyricsStore(defaults: defaults, localLyricsDirectory: lyricsDirectory)
    let track = "spotify:track:first"
    let otherTrack = "spotify:track:second"
    precondition(store.preferences(for: track).timingOffset == 0)
    store.setTimingOffset(0.25, for: track)
    store.setTimingOffset(-0.5, for: otherTrack)
    let savedLines = try store.importLyrics(imported, for: track)
    precondition(savedLines == shifted)

    do {
        try store.importLyrics("[00:01.00] \nNo timed words", for: track)
        preconditionFailure("Import must reject LRC with no timed text")
    } catch TrackLyricsImportError.missingTimedLyrics {
        precondition(store.preferences(for: track).localLyrics.map(LRC.parse) == shifted, "Failed import keeps the previous text")
    }
    do {
        try store.importLyrics("[00:00.00]" + String(repeating: "ä", count: 262_144), for: track)
        preconditionFailure("Oversized UTF-8 drafts must be rejected")
    } catch TrackLyricsImportError.lyricsTooLarge {
        precondition(store.preferences(for: track).localLyrics.map(LRC.parse) == shifted)
    }

    let reopened = TrackLyricsStore(defaults: UserDefaults(suiteName: domain)!, localLyricsDirectory: lyricsDirectory)
    precondition(reopened.preferences(for: track).timingOffset == 0.25)
    precondition(reopened.preferences(for: track).localLyrics.map(LRC.parse) == shifted)
    precondition(reopened.preferences(for: otherTrack).timingOffset == -0.5)
    precondition(reopened.preferences(for: otherTrack).localLyrics == nil)
    try reopened.removeLocalLyrics(for: track)
    try reopened.removeLocalLyrics(for: track)
    precondition(reopened.preferences(for: track).localLyrics == nil)
    precondition(reopened.preferences(for: track).timingOffset == 0.25)
    reopened.setTimingOffset(0, for: track)
    precondition(reopened.preferences(for: track).timingOffset == 0)
    precondition(reopened.preferences(for: otherTrack).timingOffset == -0.5)

    let localTrack = "spotify:local:../a/b/\(String(repeating: "Ü", count: 150))"
    try store.importLyrics(imported, for: localTrack)
    precondition(reopened.preferences(for: localTrack).localLyrics.map(LRC.parse) == shifted)
    precondition(reopened.preferences(for: localTrack.lowercased()).localLyrics == nil)
    try reopened.removeLocalLyrics(for: localTrack)
}

let json = """
[
  {"trackName":"Song","artistName":"Artist","duration":200,"syncedLyrics":null},
  {"trackName":"Song","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00]"},
  {"trackName":"Song","artistName":"Someone else","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song (Live)","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song","artistName":"Artist","duration":250,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song","artistName":"Artist","duration":201,"syncedLyrics":"[00:01.00] Right"}
]
"""
let results = try JSONDecoder().decode([LyricsResult].self, from: Data(json.utf8))
precondition(LyricsMatching.matches(results[0], title: "Song", artist: "Artist", duration: 200))
precondition(!LyricsMatching.matches(results[2], title: "Song", artist: "Artist", duration: 200))
precondition(LyricsMatching.best(results, title: "SONG", artist: "Artist", duration: 200)?.syncedLyrics == "[00:01.00] Right")
precondition(LyricsMatching.best(results, title: "Other", artist: "Artist", duration: 200) == nil)
print("LyricsCore checks passed")
