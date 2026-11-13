import Foundation
import LyricsCore

let lines = LRC.parse("[00:03.50] Second\n[00:01.00] First\n[00:07.00][00:09.10] Again\n[00:12.0] \n[00:65.00] Invalid")
precondition(lines.map(\.time) == [1, 3.5, 7, 9.1, 12])
precondition(lines.map(\.text) == ["First", "Second", "Again", "Again", ""])
precondition(LRC.currentIndex(lines, at: 0.99) == nil)
precondition(LRC.currentIndex(lines, at: 1) == 0)
precondition(LRC.currentIndex(lines, at: 8) == 2)
precondition(LRC.currentIndex(lines, at: 12) == 4)
let json = """
[
  {"trackName":"Song","artistName":"Artist","duration":200,"syncedLyrics":null},
  {"trackName":"Song","artistName":"Someone else","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song (Live)","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song","artistName":"Artist","duration":250,"syncedLyrics":"[00:01.00] Wrong"},
  {"trackName":"Song","artistName":"Artist","duration":201,"syncedLyrics":"[00:01.00] Right"}
]
"""
let results = try JSONDecoder().decode([LyricsResult].self, from: Data(json.utf8))
precondition(LyricsMatching.best(results, title: "SONG", artist: "Artist", duration: 200)?.syncedLyrics == "[00:01.00] Right")
precondition(LyricsMatching.best(results, title: "Other", artist: "Artist", duration: 200) == nil)
print("LyricsCore checks passed")
