//! Looks a song up on LRCLIB the way the app does and prints what was found,
//! without the lyrics themselves:
//! `cargo run -p lyrics-providers --example lookup -- "Title" "Artist" SECONDS ["Album"]`

use lyrics_providers::{LookupResult, LrclibClient, Query};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(title), Some(artist), Some(duration)) = (args.next(), args.next(), args.next())
    else {
        eprintln!("usage: lookup TITLE ARTIST SECONDS [ALBUM]");
        std::process::exit(2);
    };
    let query = Query {
        title,
        artist,
        album: args.next(),
        duration: duration.parse().unwrap_or(0.0),
    };
    let client = LrclibClient::new("Singline/dev (https://github.com/JulianRudrich/Singline)");
    match client.find(&query).await {
        Ok(LookupResult::Found { lyrics }) => {
            let worded = lyrics
                .lines
                .iter()
                .filter(|line| !line.words.is_empty())
                .count();
            let words: usize = lyrics.lines.iter().map(|line| line.words.len()).sum();
            println!(
                "{} — {}: {:?}, {} lines, {} with word timing ({} words), first line at {:.2} s",
                query.artist,
                query.title,
                lyrics.sync,
                lyrics.lines.len(),
                worded,
                words,
                lyrics.lines.first().map_or(0.0, |line| line.time)
            );
        }
        Ok(other) => println!("{} — {}: {other:?}", query.artist, query.title),
        Err(error) => println!("{} — {}: error {error}", query.artist, query.title),
    }
}
