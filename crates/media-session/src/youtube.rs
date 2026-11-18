//! Turning what YouTube reports for a video into the song it plays.
//!
//! A video reports its own title and its channel. Music videos follow loose
//! conventions, such as "Artist - Song (Official Video)" on a channel called
//! "ArtistVEVO" or "Artist - Topic", while the lyrics search needs the song
//! title and the artist alone. YouTube Music reports songs cleanly and is left
//! alone. Everything here is pure, so it is tested on every system.

use lyrics_core::matching::normalized;

/// The song a video plays, as far as its title and channel tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub title: String,
    pub artist: String,
}

/// Turns the title and channel of a YouTube video into the song's title and
/// artist. Decorations such as "(Official Video)" or "[HD]" are dropped; what
/// makes a recording different, such as "(feat. X)", "(Remix)" or "(Live)",
/// stays.
pub fn song_from_video(title: &str, channel: &str) -> Song {
    let channel = Channel::parse(channel);
    let raw = collapse(title);
    if channel.generated {
        // YouTube's own channels for an artist carry the song's exact title.
        return finish(strip_groups(&raw), channel.name, &raw);
    }
    let text = strip_trailing_decorations(&strip_groups(&without_hashtags(cut_at_pipe(&raw))));
    let (artist_part, title_part) = split_artist_title(&text, &channel.name);
    let (artist, guest) = match artist_part {
        Some(part) => artist_and_guest(&part),
        None => (channel.name.clone(), None),
    };
    let mut title = with_feature_in_brackets(&tidy(&title_part));
    if let Some(guest) = guest {
        if !title.to_lowercase().contains("feat.") {
            title = format!("{title} (feat. {guest})");
        }
    }
    let artist = if artist.is_empty() {
        channel.name
    } else {
        artist
    };
    finish(title, artist, &raw)
}

/// True when a title and artist look like a YouTube video rather than a song:
/// a channel YouTube made for an artist, such as "Artist - Topic" or
/// "ArtistVEVO", or a title with a decoration such as "(Official Video)".
/// Used where the browser does not tell which page plays.
pub fn looks_like_video(title: &str, artist: &str) -> bool {
    let channel = Channel::parse(artist);
    if channel.generated || channel.vevo {
        return true;
    }
    let title = collapse(title);
    let grouped = pieces(&title).iter().any(|piece| match piece {
        Piece::Group { inner, .. } => is_decoration(inner) && has_video_word(inner),
        Piece::Text(_) => false,
    });
    let lower = title.to_ascii_lowercase();
    let trailing = TRAILING_DECORATIONS
        .iter()
        .filter(|phrase| phrase.contains("official") || phrase.contains("video"))
        .any(|phrase| {
            lower
                .strip_suffix(phrase)
                .is_some_and(|rest| rest.ends_with([' ', '-', '|']) && !rest.trim().is_empty())
        });
    grouped || trailing || lower.contains(" | official")
}

/// What a channel name says about the artist.
struct Channel {
    /// The artist's name without "- Topic", "VEVO" or "Official".
    name: String,
    /// "Artist - Topic": made by YouTube, titled exactly like the song.
    generated: bool,
    /// "ArtistVEVO": the artist's label channel.
    vevo: bool,
}

impl Channel {
    fn parse(channel: &str) -> Self {
        let mut name = collapse(channel);
        let mut generated = false;
        let mut vevo = false;
        if let Some(rest) = strip_suffix_ignore_case(&name, " - topic") {
            name = rest.trim().to_owned();
            generated = true;
        }
        loop {
            let before = name.clone();
            for suffix in [
                " official artist channel",
                " official channel",
                " - official",
                " official",
                " (official)",
            ] {
                if let Some(rest) = strip_suffix_ignore_case(&name, suffix) {
                    name = rest.trim().to_owned();
                }
            }
            if let Some(rest) = strip_suffix_ignore_case(&name, " vevo")
                .or_else(|| name.strip_suffix("VEVO"))
                .or_else(|| name.strip_suffix("Vevo"))
            {
                name = rest.trim().to_owned();
                vevo = true;
            }
            if let Some(rest) = name
                .strip_suffix("Official")
                .or_else(|| name.strip_suffix("OFFICIAL"))
            {
                name = rest.trim().to_owned();
            }
            if name == before || name.is_empty() {
                break;
            }
        }
        if name.is_empty() {
            name = collapse(channel);
        }
        Self {
            name,
            generated,
            vevo,
        }
    }
}

/// Words that only describe the video, in lower case.
const DECORATION_WORDS: &[&str] = &[
    "official",
    "officiel",
    "officielle",
    "oficial",
    "ufficiale",
    "offizielles",
    "offizielle",
    "offizieller",
    "video",
    "vídeo",
    "vidéo",
    "videoclip",
    "clip",
    "music",
    "musik",
    "musikvideo",
    "audio",
    "lyric",
    "lyrics",
    "lyricvideo",
    "letra",
    "letras",
    "paroles",
    "songtext",
    "testo",
    "visualizer",
    "visualiser",
    "mv",
    "m/v",
    "pv",
    "hd",
    "hq",
    "4k",
    "8k",
    "uhd",
    "1080p",
    "720p",
    "60fps",
    "explicit",
    "clean",
    "the",
    "with",
    "color",
    "colour",
    "coded",
    "full",
    "only",
    "new",
    "premiere",
    "sub",
    "subs",
    "subtitles",
    "subtitulado",
    "subtitulada",
    "español",
    "english",
    "en",
    "公式",
    "歌詞",
    "歌詞付き",
    "字幕",
    "中字",
    "中文字幕",
    "官方",
    "官方mv",
    "高音質",
];

/// Words for picture quality: a group with one only describes the video.
const QUALITY_WORDS: &[&str] = &[
    "hd", "hq", "4k", "8k", "uhd", "1080p", "720p", "60fps", "upgrade",
];

/// Words that tell one recording of a song from another, in lower case.
const KEEP_WORDS: &[&str] = &[
    "remix",
    "rmx",
    "mix",
    "live",
    "acoustic",
    "unplugged",
    "version",
    "ver",
    "edit",
    "demo",
    "instrumental",
    "cover",
    "remaster",
    "remastered",
    "reprise",
    "session",
    "sessions",
    "mono",
    "stereo",
    "extended",
    "radio",
    "sped",
    "slowed",
    "reverb",
    "nightcore",
    "piano",
    "orchestral",
    "acapella",
    "cappella",
    "rework",
    "vip",
    "dub",
    "from",
    "soundtrack",
    "ost",
    "feat",
    "ft",
    "featuring",
];

/// Decorations that also appear without brackets at the end of a title,
/// longest first, in lower case.
const TRAILING_DECORATIONS: &[&str] = &[
    "official music video",
    "official lyric video",
    "official lyrics video",
    "official visualizer",
    "official visualiser",
    "official video",
    "official audio",
    "official m/v",
    "official mv",
    "lyric video",
    "lyrics video",
    "music video",
    "visualizer",
    "lyrics",
    "m/v",
    "mv",
    "hd",
    "hq",
    "4k",
    "official",
];

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| {
            c.is_whitespace() || matches!(c, '-' | '|' | ',' | '.' | ':' | '!' | '+' | '&' | '·')
        })
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

fn is_year(word: &str) -> bool {
    word.len() == 4
        && word
            .parse::<u16>()
            .is_ok_and(|year| (1900..=2099).contains(&year))
}

/// The names after "feat.", "ft." or "featuring" at the start of `text`.
fn feature_rest(text: &str) -> Option<&str> {
    let lower = text.to_ascii_lowercase();
    [
        "featuring ",
        "feat. ",
        "feat.",
        "feat ",
        "ft. ",
        "ft.",
        "ft ",
    ]
    .iter()
    .find(|marker| lower.starts_with(*marker))
    .map(|marker| text[marker.len()..].trim())
    .filter(|rest| !rest.is_empty())
}

/// True when a bracketed group only describes the video, such as "Official
/// Video", "Lyrics", "HD", "2011" or "Prod. by X".
fn is_decoration(inner: &str) -> bool {
    let words = words(inner);
    if words.is_empty() {
        return true;
    }
    let lower = inner.trim().to_lowercase();
    if words.len() == 1 && is_year(&words[0]) {
        return true;
    }
    if lower.starts_with("prod.")
        || lower.starts_with("prod by")
        || lower.starts_with("produced by")
    {
        return true;
    }
    let keeps = |word: &String| KEEP_WORDS.contains(&word.as_str());
    let quality = words
        .iter()
        .any(|word| QUALITY_WORDS.contains(&word.as_str()));
    let other_keeps = words
        .iter()
        .any(|word| keeps(word) && !word.starts_with("remaster"));
    if quality && !other_keeps {
        return true;
    }
    if words.iter().any(keeps) {
        return false;
    }
    words
        .iter()
        .all(|word| DECORATION_WORDS.contains(&word.as_str()) || is_year(word))
        || words.iter().any(|word| {
            matches!(
                word.as_str(),
                "official" | "officiel" | "oficial" | "ufficiale" | "offizielles" | "offizielle"
            )
        })
}

/// Words that only appear in decorations of videos, not of songs.
fn has_video_word(inner: &str) -> bool {
    words(inner).iter().any(|word| {
        word.starts_with("offici")
            || word.starts_with("oficial")
            || word.starts_with("offiziell")
            || word.starts_with("lyric")
            || word.starts_with("visuali")
            || matches!(
                word.as_str(),
                "video" | "audio" | "mv" | "m/v" | "clip" | "hd" | "4k" | "letra" | "paroles"
            )
    })
}

/// The decoration words of a group taken out, for groups such as "Official
/// Live Video" that also say something about the recording.
fn without_decoration_words(inner: &str) -> String {
    inner
        .split_whitespace()
        .filter(|token| {
            let word: String = token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '/')
                .flat_map(char::to_lowercase)
                .collect();
            !DECORATION_WORDS.contains(&word.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

const BRACKETS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('【', '】'), ('（', '）')];

enum Piece<'a> {
    Text(&'a str),
    Group {
        open: char,
        close: char,
        inner: &'a str,
    },
}

/// Splits a title into plain text and bracketed groups. An unclosed bracket
/// stays plain text.
fn pieces(text: &str) -> Vec<Piece<'_>> {
    let mut found = Vec::new();
    let mut rest = text;
    loop {
        let opening = rest.char_indices().find_map(|(index, c)| {
            BRACKETS
                .iter()
                .find(|(open, _)| *open == c)
                .map(|(open, close)| (index, *open, *close))
        });
        let Some((start, open, close)) = opening else {
            break;
        };
        let inner_start = start + open.len_utf8();
        let mut depth = 0usize;
        let end = rest[inner_start..].char_indices().find_map(|(index, c)| {
            if c == open {
                depth += 1;
            } else if c == close {
                if depth == 0 {
                    return Some(inner_start + index);
                }
                depth -= 1;
            }
            None
        });
        let Some(end) = end else {
            break;
        };
        found.push(Piece::Text(&rest[..start]));
        found.push(Piece::Group {
            open,
            close,
            inner: &rest[inner_start..end],
        });
        rest = &rest[end + close.len_utf8()..];
    }
    found.push(Piece::Text(rest));
    found
}

/// Drops decorating groups, writes featured artists as "(feat. X)" and keeps
/// every other group.
fn strip_groups(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for piece in pieces(text) {
        match piece {
            Piece::Text(text) => result.push_str(text),
            Piece::Group { open, close, inner } => {
                if let Some(guest) = feature_rest(inner.trim()) {
                    result.push_str(&format!(" (feat. {guest}) "));
                } else if is_decoration(inner) {
                    result.push(' ');
                } else {
                    let kept = if has_video_word(inner) {
                        without_decoration_words(inner)
                    } else {
                        inner.trim().to_owned()
                    };
                    if !kept.is_empty() {
                        result.push(open);
                        result.push_str(&kept);
                        result.push(close);
                    }
                }
            }
        }
    }
    collapse(&result)
}

/// Drops decorations at the end of a title, such as "Official Video" or
/// "- Lyrics", but never the whole title.
fn strip_trailing_decorations(text: &str) -> String {
    let mut current = tidy(text);
    loop {
        // ASCII lower case keeps every byte where it was.
        let lower = current.to_ascii_lowercase();
        let cut = TRAILING_DECORATIONS.iter().find_map(|phrase| {
            let rest = lower.strip_suffix(phrase)?;
            let boundary =
                rest.ends_with([' ', '-', '|', '–', '—', ':', '】', '」', '』', ')', ']']);
            (boundary && !rest.trim().is_empty() && current.is_char_boundary(rest.len()))
                .then(|| tidy(&current[..rest.len()]))
        });
        match cut {
            Some(shorter) if !shorter.is_empty() && shorter != current => current = shorter,
            _ => return current,
        }
    }
}

/// Everything before a " | ", which on YouTube introduces the channel, a show
/// or a playlist rather than the song.
fn cut_at_pipe(text: &str) -> &str {
    [" | ", " ｜ ", "｜", " || "]
        .iter()
        .filter_map(|pipe| text.find(pipe))
        .min()
        .map(|index| text[..index].trim())
        .filter(|head| !head.is_empty())
        .unwrap_or(text)
}

fn without_hashtags(text: &str) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    while words.len() > 1 && words.last().is_some_and(|word| word.starts_with('#')) {
        words.pop();
    }
    words.join(" ")
}

/// Separators between artist and title, as YouTube titles use them.
const SEPARATORS: [&str; 5] = [" - ", " – ", " — ", " _ ", " ‐ "];

/// Splits "Artist - Title" and its variants. Returns the artist when the
/// title names one, and the title.
fn split_artist_title(text: &str, channel: &str) -> (Option<String>, String) {
    // 米津玄師「Lemon」 and YOASOBI『アイドル』 quote the title.
    for (open, close) in [('「', '」'), ('『', '』')] {
        if let Some((before, inside, after)) = enclosed(text, open, close) {
            let artist = strip_trailing_decorations(if before.is_empty() { after } else { before });
            return ((!artist.is_empty()).then_some(artist), inside.to_owned());
        }
    }
    let separator = SEPARATORS
        .iter()
        .filter_map(|separator| text.find(separator).map(|index| (index, separator.len())))
        .min();
    if let Some((index, length)) = separator {
        let left = text[..index].trim();
        let right = text[index + length..].trim();
        if !left.is_empty() && !right.is_empty() {
            let left_is_channel = same_artist(left, channel);
            if same_artist(right, channel) && !left_is_channel {
                return (Some(right.to_owned()), left.to_owned());
            }
            if !left_is_channel && is_version_suffix(right) {
                return (None, text.to_owned());
            }
            return (Some(left.to_owned()), right.to_owned());
        }
    }
    // 周杰倫【告白氣球】 puts the title in brackets, 【初音ミク】Song the singer.
    if let Some((before, inside, after)) = enclosed(text, '【', '】') {
        return match (before.is_empty(), after.is_empty()) {
            (true, false) => (Some(inside.to_owned()), after.to_owned()),
            (true, true) => (None, inside.to_owned()),
            (false, _) => (Some(before.to_owned()), inside.to_owned()),
        };
    }
    // BTS 'Dynamite' and Artist "Title"
    if let Some((before, inside, after)) = quoted(text) {
        if !before.is_empty() {
            let title = if after.is_empty() {
                inside.to_owned()
            } else {
                format!("{inside} {after}")
            };
            return (Some(before.to_owned()), title);
        }
    }
    // 怪物 / YOASOBI, when one side is the channel
    for slash in [" / ", " ／ "] {
        if let Some((left, right)) = text.split_once(slash) {
            let (left, right) = (left.trim(), right.trim());
            if same_artist(right, channel) {
                return (Some(right.to_owned()), left.to_owned());
            }
            if same_artist(left, channel) {
                return (Some(left.to_owned()), right.to_owned());
            }
        }
    }
    (None, text.to_owned())
}

/// The text before, inside and after the first `open`…`close` pair.
fn enclosed(text: &str, open: char, close: char) -> Option<(&str, &str, &str)> {
    let start = text.find(open)?;
    let inner_start = start + open.len_utf8();
    let end = inner_start + text[inner_start..].find(close)?;
    let inside = text[inner_start..end].trim();
    (!inside.is_empty()).then(|| {
        (
            text[..start].trim(),
            inside,
            text[end + close.len_utf8()..].trim(),
        )
    })
}

/// A title in quotes after the artist: `Artist 'Title'` or `Artist "Title"`.
/// An apostrophe inside a word, as in "Don't", is not a quote.
fn quoted(text: &str) -> Option<(&str, &str, &str)> {
    for (open, close) in [('"', '"'), ('“', '”'), ('\'', '\''), ('‘', '’')] {
        let Some(start) = text
            .char_indices()
            .find(|(index, c)| *c == open && (*index == 0 || text[..*index].ends_with([' ', '('])))
            .map(|(index, _)| index)
        else {
            continue;
        };
        let inner_start = start + open.len_utf8();
        let end = text[inner_start..]
            .char_indices()
            .find(|(index, c)| {
                let after = &text[inner_start + index + c.len_utf8()..];
                *c == close && (after.is_empty() || after.starts_with([' ', ')', ',', '.']))
            })
            .map(|(index, _)| inner_start + index);
        if let Some(end) = end {
            let inside = text[inner_start..end].trim();
            if !inside.is_empty() {
                return Some((
                    text[..start].trim(),
                    inside,
                    text[end + close.len_utf8()..].trim(),
                ));
            }
        }
    }
    None
}

/// The name without spaces, case, accents and punctuation.
fn compact(name: &str) -> String {
    normalized(name).replace(' ', "")
}

/// True when two names are the same artist, allowing for a channel such as
/// "Ayase / YOASOBI" or "LinkinParkTV".
fn same_artist(name: &str, channel: &str) -> bool {
    let (name, channel) = (compact(name), compact(channel));
    let shorter = name.chars().count().min(channel.chars().count());
    if name.is_empty() || channel.is_empty() {
        return false;
    }
    name == channel
        || (shorter >= 3 && (name.starts_with(&channel) || channel.starts_with(&name)))
        || (shorter >= 4 && (name.contains(&channel) || channel.contains(&name)))
}

/// True for the part after a dash that names a version of the song rather
/// than the song, as in "Song - Remastered 2011" or "Song - Live at Wembley".
fn is_version_suffix(part: &str) -> bool {
    let outside: String = pieces(part)
        .iter()
        .filter_map(|piece| match piece {
            Piece::Text(text) => Some(*text),
            Piece::Group { .. } => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    if outside.trim().is_empty() {
        return true;
    }
    let words: Vec<String> = outside
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect();
    let (Some(first), Some(last)) = (words.first(), words.last()) else {
        return false;
    };
    if words.len() > 6 {
        return false;
    }
    let second = words.get(1).map(String::as_str);
    let live = first == "live"
        && (words.len() == 1
            || matches!(second, Some("at" | "in" | "from" | "on" | "version"))
            || second.is_some_and(is_year));
    let leading = matches!(first.as_str(), "remaster" | "remastered")
        || (words.len() >= 2 && matches!(first.as_str(), "from" | "feat" | "ft" | "featuring"));
    let trailing = matches!(
        last.as_str(),
        "remaster"
            | "remastered"
            | "remix"
            | "mix"
            | "edit"
            | "version"
            | "session"
            | "sessions"
            | "take"
            | "demo"
            | "mono"
            | "stereo"
            | "instrumental"
            | "acoustic"
            | "unplugged"
            | "reprise"
            | "live"
    );
    live || leading || trailing
}

/// The artist without a featured guest, and the guest: "Artist ft. Guest".
/// A name in another script after the artist, as in "BTS (방탄소년단)", is
/// dropped.
fn artist_and_guest(part: &str) -> (String, Option<String>) {
    let part = tidy(part);
    let lower = part.to_ascii_lowercase();
    let marker = [
        " (feat. ",
        " (ft. ",
        " (featuring ",
        " feat. ",
        " feat ",
        " ft. ",
        " ft ",
        " featuring ",
    ]
    .iter()
    .filter_map(|marker| lower.find(marker).map(|index| (index, marker.len())))
    .min();
    let (artist, guest) = match marker {
        Some((index, length)) => {
            let guest = part[index + length..]
                .trim()
                .trim_end_matches(')')
                .trim()
                .to_owned();
            (
                part[..index].trim().to_owned(),
                (!guest.is_empty()).then_some(guest),
            )
        }
        None => (part.clone(), None),
    };
    let artist = match (artist.ends_with(')'), artist.rfind('(')) {
        (true, Some(index)) if index > 0 => artist[..index].trim().to_owned(),
        _ => artist,
    };
    (tidy(&artist), guest)
}

/// "Song ft. Guest" becomes "Song (feat. Guest)", the way music services
/// write it.
fn with_feature_in_brackets(title: &str) -> String {
    let lower = title.to_ascii_lowercase();
    let marker = [" featuring ", " feat. ", " feat ", " ft. ", " ft "]
        .iter()
        .filter_map(|marker| lower.find(marker).map(|index| (index, marker.len())))
        .min();
    let Some((index, length)) = marker else {
        return title.to_owned();
    };
    let rest = &title[index + length..];
    let (guest, tail) = match rest.find(['(', '[']) {
        Some(bracket) => (rest[..bracket].trim(), rest[bracket..].trim()),
        None => (rest.trim(), ""),
    };
    if guest.is_empty() {
        return title.to_owned();
    }
    let base = title[..index].trim();
    collapse(&format!("{base} {tail} (feat. {guest})"))
}

/// Single spaces, no separators or quotes around the text.
fn tidy(text: &str) -> String {
    let collapsed = collapse(text);
    let mut current = collapsed.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '-' | '–' | '—' | '|' | ':' | '_' | ',' | '/' | '~')
    });
    for (open, close) in [('"', '"'), ('“', '”'), ('\'', '\''), ('‘', '’')] {
        if current.chars().count() > 2 {
            if let Some(inner) = current
                .strip_prefix(open)
                .and_then(|rest| rest.strip_suffix(close))
            {
                if !inner.contains(close) || open != close {
                    current = inner.trim();
                }
            }
        }
    }
    current.to_owned()
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `text` without an ASCII `suffix` of any case.
fn strip_suffix_ignore_case<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let start = text.len().checked_sub(suffix.len())?;
    (text.is_char_boundary(start) && text[start..].eq_ignore_ascii_case(suffix))
        .then(|| &text[..start])
}

/// The cleaned song, falling back to the raw title when nothing is left.
fn finish(title: String, artist: String, raw: &str) -> Song {
    let title = tidy(&title);
    Song {
        title: if title.is_empty() { tidy(raw) } else { title },
        artist: tidy(&artist),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, channel: &str) -> (String, String) {
        let song = song_from_video(title, channel);
        (song.title, song.artist)
    }

    fn check(cases: &[(&str, &str, &str, &str)]) {
        for (title, channel, expected_title, expected_artist) in cases {
            assert_eq!(
                song(title, channel),
                ((*expected_title).to_owned(), (*expected_artist).to_owned()),
                "{title:?} on {channel:?}"
            );
        }
    }

    #[test]
    fn channels_name_the_artist() {
        check(&[
            (
                "Never Gonna Give You Up",
                "Rick Astley - Topic",
                "Never Gonna Give You Up",
                "Rick Astley",
            ),
            ("Hello", "AdeleVEVO", "Hello", "Adele"),
            ("Hello", "Adele VEVO", "Hello", "Adele"),
            (
                "thank u, next",
                "ArianaGrandeVevo",
                "thank u, next",
                "ArianaGrande",
            ),
            ("Du Hast", "Rammstein Official", "Du Hast", "Rammstein"),
            ("Beggin'", "MåneskinOfficial", "Beggin'", "Måneskin"),
            ("Song", "Unofficial", "Song", "Unofficial"),
            ("Song", "VEVO", "Song", "VEVO"),
            ("Song", "", "Song", ""),
        ]);
    }

    #[test]
    fn topic_channels_keep_the_title_as_it_is() {
        check(&[
            (
                "Bohemian Rhapsody - Remastered 2011",
                "Queen - Topic",
                "Bohemian Rhapsody - Remastered 2011",
                "Queen",
            ),
            (
                "Don't Stop Me Now",
                "Queen - Topic",
                "Don't Stop Me Now",
                "Queen",
            ),
            (
                "Rock And Roll - Live",
                "Led Zeppelin - Topic",
                "Rock And Roll - Live",
                "Led Zeppelin",
            ),
            (
                "Artist - Song",
                "Someone - Topic",
                "Artist - Song",
                "Someone",
            ),
        ]);
    }

    #[test]
    fn artist_and_title_are_split_apart() {
        check(&[
            (
                "Rick Astley - Never Gonna Give You Up (Official Music Video)",
                "Rick Astley",
                "Never Gonna Give You Up",
                "Rick Astley",
            ),
            (
                "Taylor Swift - Shake It Off",
                "TaylorSwiftVEVO",
                "Shake It Off",
                "Taylor Swift",
            ),
            (
                "Imagine Dragons - Believer (Lyrics)",
                "7clouds",
                "Believer",
                "Imagine Dragons",
            ),
            (
                "AC/DC - Back In Black (Official Video)",
                "acdcVEVO",
                "Back In Black",
                "AC/DC",
            ),
            (
                "Queen – Don't Stop Me Now (Official Video)",
                "Queen Official",
                "Don't Stop Me Now",
                "Queen",
            ),
            (
                "Måneskin - Beggin' (Lyrics)",
                "MåneskinOfficial",
                "Beggin'",
                "Måneskin",
            ),
            (
                "Despacito - Luis Fonsi",
                "LuisFonsiVEVO",
                "Despacito",
                "Luis Fonsi",
            ),
            (
                "Artist & Guest - Song",
                "Label Records",
                "Song",
                "Artist & Guest",
            ),
            ("Artist x Guest - Song", "Label", "Song", "Artist x Guest"),
            ("Artist - \"Song\"", "Artist", "Song", "Artist"),
            (
                "Artist \"Song\" (Official Video)",
                "Label",
                "Song",
                "Artist",
            ),
            (
                "Oasis - Live Forever",
                "Someone's uploads",
                "Live Forever",
                "Oasis",
            ),
        ]);
    }

    #[test]
    fn decorations_are_dropped() {
        check(&[
            (
                "Coldplay - Yellow (Official Video)",
                "Coldplay",
                "Yellow",
                "Coldplay",
            ),
            (
                "Linkin Park - Numb [Official Music Video] [4K Upgrade]",
                "Linkin Park",
                "Numb",
                "Linkin Park",
            ),
            (
                "Billie Eilish - bad guy (Audio)",
                "BillieEilishVEVO",
                "bad guy",
                "Billie Eilish",
            ),
            ("Artist - Song (Visualizer)", "Artist", "Song", "Artist"),
            (
                "Artist - Song (Official Visualiser)",
                "Artist",
                "Song",
                "Artist",
            ),
            (
                "Ed Sheeran - Shape of You | Official Music Video",
                "Ed Sheeran",
                "Shape of You",
                "Ed Sheeran",
            ),
            ("Artist - Song | Lyrics", "Artist", "Song", "Artist"),
            (
                "Eminem - Lose Yourself [HD]",
                "EminemVEVO",
                "Lose Yourself",
                "Eminem",
            ),
            ("Artist - Song 4K", "Artist", "Song", "Artist"),
            (
                "Oasis - Live Forever (Official HD Remastered Video)",
                "Oasis",
                "Live Forever",
                "Oasis",
            ),
            (
                "Rammstein - Du Hast (Official 4K Video)",
                "Rammstein Official",
                "Du Hast",
                "Rammstein",
            ),
            (
                "The Weeknd - Blinding Lights (Official Video) #shorts #music",
                "TheWeekndVEVO",
                "Blinding Lights",
                "The Weeknd",
            ),
            ("Song Official Video", "Artist", "Song", "Artist"),
            ("Artist - Song - Lyrics", "Artist", "Song", "Artist"),
            ("Song (1985)", "Artist", "Song", "Artist"),
            (
                "Artist - Song (Prod. by Someone)",
                "Artist",
                "Song",
                "Artist",
            ),
            ("Artist - Song (Explicit)", "Artist", "Song", "Artist"),
            ("Artist - Song (with lyrics)", "Artist", "Song", "Artist"),
            (
                "Artista - Canción (Video Oficial)",
                "Artista",
                "Canción",
                "Artista",
            ),
            (
                "Artiste - Chanson (Clip Officiel)",
                "Artiste",
                "Chanson",
                "Artiste",
            ),
            (
                "Künstler - Lied (Offizielles Musikvideo)",
                "Künstler",
                "Lied",
                "Künstler",
            ),
            ("Lyrics", "Artist", "Lyrics", "Artist"),
            ("Official Video", "Artist", "Official Video", "Artist"),
        ]);
    }

    #[test]
    fn meaningful_parts_stay() {
        check(&[
            ("Song (Remix)", "Artist", "Song (Remix)", "Artist"),
            ("Artist - Song (Live)", "Artist", "Song (Live)", "Artist"),
            (
                "Artist - Song (Live at Wembley) [HD]",
                "Artist",
                "Song (Live at Wembley)",
                "Artist",
            ),
            (
                "Artist - Song (Official Live Video)",
                "Artist",
                "Song (Live)",
                "Artist",
            ),
            (
                "Artist - Song (Official Audio) (Remix)",
                "Artist",
                "Song (Remix)",
                "Artist",
            ),
            (
                "Artist - Song (Acoustic Version)",
                "Artist",
                "Song (Acoustic Version)",
                "Artist",
            ),
            (
                "Artist - Song (Slowed + Reverb)",
                "Uploader",
                "Song (Slowed + Reverb)",
                "Artist",
            ),
            (
                "Artist - Song (2011 Remaster)",
                "Artist",
                "Song (2011 Remaster)",
                "Artist",
            ),
            (
                "Artist - Song (with Guest)",
                "Artist",
                "Song (with Guest)",
                "Artist",
            ),
        ]);
    }

    #[test]
    fn featured_artists_are_written_like_music_services_do() {
        check(&[
            (
                "Artist - Song (feat. Guest) [Official Video]",
                "ArtistVEVO",
                "Song (feat. Guest)",
                "Artist",
            ),
            (
                "Mark Ronson - Uptown Funk (Official Video) ft. Bruno Mars",
                "Mark Ronson",
                "Uptown Funk (feat. Bruno Mars)",
                "Mark Ronson",
            ),
            (
                "Artist ft. Guest - Song",
                "Artist",
                "Song (feat. Guest)",
                "Artist",
            ),
            (
                "Artist feat. Guest - Song (Official Video)",
                "Label",
                "Song (feat. Guest)",
                "Artist",
            ),
            (
                "Artist - Song ft. Guest (Official Video)",
                "Artist",
                "Song (feat. Guest)",
                "Artist",
            ),
            (
                "Artist - Song [ft. Guest]",
                "Uploader",
                "Song (feat. Guest)",
                "Artist",
            ),
            (
                "Artist - Song (Remix) ft. Guest",
                "Artist",
                "Song (Remix) (feat. Guest)",
                "Artist",
            ),
        ]);
    }

    #[test]
    fn titles_with_several_dashes() {
        check(&[
            (
                "Artist - Song - Live at Wembley",
                "Artist",
                "Song - Live at Wembley",
                "Artist",
            ),
            (
                "Artist - Song - Remastered 2011",
                "Label",
                "Song - Remastered 2011",
                "Artist",
            ),
            (
                "Song - Remastered 2011",
                "Artist",
                "Song - Remastered 2011",
                "Artist",
            ),
            (
                "Song - Live at the Royal Albert Hall",
                "Artist",
                "Song - Live at the Royal Albert Hall",
                "Artist",
            ),
            (
                "Song - Radio Edit",
                "Uploader",
                "Song - Radio Edit",
                "Uploader",
            ),
            (
                "Jay-Z - Empire State of Mind",
                "JayZVEVO",
                "Empire State of Mind",
                "Jay-Z",
            ),
            (
                "a-ha - Take On Me (Official Video) [Remastered in 4K]",
                "a-ha",
                "Take On Me",
                "a-ha",
            ),
        ]);
    }

    #[test]
    fn titles_in_other_scripts() {
        check(&[
            ("米津玄師 MV「Lemon」", "米津玄師", "Lemon", "米津玄師"),
            (
                "YOASOBI「アイドル」 Official Music Video",
                "Ayase / YOASOBI",
                "アイドル",
                "YOASOBI",
            ),
            ("【MV】怪物 / YOASOBI", "Ayase / YOASOBI", "怪物", "YOASOBI"),
            (
                "BTS (방탄소년단) 'Dynamite' Official MV",
                "HYBE LABELS",
                "Dynamite",
                "BTS",
            ),
            (
                "[MV] IU(아이유) _ Love wins all",
                "1theK (원더케이)",
                "Love wins all",
                "IU",
            ),
            ("Кино - Группа крови", "Кино", "Группа крови", "Кино"),
            (
                "周杰倫 Jay Chou【告白氣球 Love Confession】Official MV",
                "周杰倫 Jay Chou",
                "告白氣球 Love Confession",
                "周杰倫 Jay Chou",
            ),
            ("【初音ミク】メルト", "ryo", "メルト", "初音ミク"),
            ("「Lemon」米津玄師", "米津玄師", "Lemon", "米津玄師"),
        ]);
    }

    #[test]
    fn videos_are_told_from_songs() {
        for (title, artist) in [
            ("Song", "Artist - Topic"),
            ("Song", "ArtistVEVO"),
            ("Artist - Song (Official Video)", "Artist"),
            ("Artist - Song [Official Music Video]", "Label"),
            ("Artist - Song (Lyrics)", "Uploader"),
            ("Artist - Song (Visualizer)", "Artist"),
            ("Artist - Song | Official Video", "Artist"),
            ("Artist - Song Official Video", "Artist"),
            ("Artist - Song (HD)", "Artist"),
        ] {
            assert!(looks_like_video(title, artist), "{title:?} by {artist:?}");
        }
        for (title, artist) in [
            ("Song - Remastered 2011", "Artist"),
            ("Song (feat. Guest)", "Artist"),
            ("Song (Live)", "Artist"),
            ("Song (2011)", "Artist"),
            ("Song (Explicit)", "Artist"),
            ("Lyrics", "Artist"),
            ("Official Video", "Artist"),
            ("Evolution", "Unofficial"),
        ] {
            assert!(!looks_like_video(title, artist), "{title:?} by {artist:?}");
        }
    }
}
