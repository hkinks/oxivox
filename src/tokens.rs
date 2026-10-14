//! Whisper emits annotations such as `[BLANK_AUDIO]` or `(music)` for non-speech. They must never be pasted.

/// Annotation contents (lower-case, `_` as space) that whisper uses for non-speech.
const NON_SPEECH: &[&str] = &[
    "blank audio",
    "silence",
    "silent",
    "no audio",
    "no speech",
    "inaudible",
    "music",
    "music playing",
    "soft music",
    "upbeat music",
    "applause",
    "laughter",
    "laughs",
    "laughing",
    "cheering",
    "noise",
    "background noise",
    "static",
    "sighs",
    "coughs",
    "coughing",
    "clears throat",
    "breathing",
    "typing",
    "keyboard clicking",
    "speaking in foreign language",
    "speaking foreign language",
    "foreign language",
    "end",
    "pause",
];

fn is_annotation(inner: &str) -> bool {
    let normalized = inner.replace('_', " ").trim().to_lowercase();
    NON_SPEECH.contains(&normalized.as_str())
}

/// Removes `open ... close` groups whose content is a known non-speech annotation.
fn strip_groups(text: &str, open: char, close: char) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(open) {
        let after = &rest[start + open.len_utf8()..];
        match after.find(close) {
            Some(len) if is_annotation(&after[..len]) => {
                out.push_str(&rest[..start]);
                out.push(' ');
                rest = &after[len + close.len_utf8()..];
            }
            _ => {
                out.push_str(&rest[..start + open.len_utf8()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Cleans a raw transcript; an empty result means there was nothing to say.
pub fn clean(raw: &str) -> String {
    let mut text = raw.to_string();
    for (open, close) in [('[', ']'), ('(', ')'), ('*', '*'), ('<', '>')] {
        text = strip_groups(&text, open, close);
    }
    // Special tokens such as <|endoftext|> and note symbols that stand for music.
    while let Some(start) = text.find("<|") {
        match text[start..].find("|>") {
            Some(len) => text.replace_range(start..start + len + 2, " "),
            None => break,
        }
    }
    let text = text.replace(['♪', '♫', '♬'], " ");
    let words: Vec<&str> = text.split_whitespace().collect();
    let joined = words.join(" ");
    // Punctuation left over from removed annotations ("[MUSIC] .") is not speech either.
    if joined.chars().any(char::is_alphanumeric) { joined } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::clean;

    #[test]
    fn blank_audio_is_empty() {
        assert_eq!(clean(" [BLANK_AUDIO]"), "");
        assert_eq!(clean("[ Silence ]"), "");
        assert_eq!(clean("(music)"), "");
        assert_eq!(clean("*Music*"), "");
        assert_eq!(clean("♪ ♪"), "");
        assert_eq!(clean("<|endoftext|>"), "");
    }

    #[test]
    fn annotations_inside_speech_are_removed() {
        assert_eq!(clean("Hello [BLANK_AUDIO] world"), "Hello world");
        assert_eq!(clean("Hello world. (applause)"), "Hello world.");
    }

    #[test]
    fn real_parentheses_and_brackets_survive() {
        assert_eq!(clean("call foo(bar) and [see below]"), "call foo(bar) and [see below]");
        assert_eq!(clean("2 * 3 * 4"), "2 * 3 * 4");
    }

    #[test]
    fn whitespace_is_normalised() {
        assert_eq!(clean("  hello \n  world  "), "hello world");
    }

    #[test]
    fn leftover_punctuation_is_empty() {
        assert_eq!(clean("[MUSIC]."), "");
        assert_eq!(clean("..."), "");
    }

    #[test]
    fn unterminated_group_is_kept() {
        assert_eq!(clean("a [b c"), "a [b c");
    }
}
