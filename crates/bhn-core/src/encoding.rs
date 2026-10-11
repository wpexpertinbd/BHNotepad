//! Text encodings, so a file that is not UTF-8 can be read AND written back.
//!
//! Added for <https://github.com/wpexpertinbd/BHNotepad/issues/1>, which asked
//! for a font picker so ANSI characters would show. A font was never the
//! problem: an NFO or ANSI-art file is **CP437 bytes**, which are not valid
//! UTF-8, so every box-drawing character had already become U+FFFD before any
//! font saw it. No font renders a replacement character as `═`.
//!
//! ⚠️ These are single-byte code pages, so a decode is a table lookup and an
//! encode is a reverse lookup. The tables were **generated from Python's own
//! `cp437` and `cp1252` codecs**, not transcribed by hand, and
//! `every_byte_survives_a_round_trip` checks all 256 values of each — a single
//! wrong glyph would corrupt a file on save, silently.

/// Plain UTF-8, and what a new file is.
pub const UTF8: &str = "utf-8";
/// IBM PC code page 437: NFO files, ANSI art, old DOS text.
pub const CP437: &str = "cp437";
/// Windows-1252: Western European text from Windows editors.
pub const WINDOWS_1252: &str = "windows-1252";

/// Every encoding the editor can read and write, for the UI to offer.
pub const SUPPORTED: &[&str] = &[UTF8, CP437, WINDOWS_1252];

/// A human label for the status bar.
pub fn label(encoding: &str) -> &'static str {
    match encoding {
        CP437 => "CP437",
        WINDOWS_1252 => "Windows-1252",
        _ => "UTF-8",
    }
}

/// Normalises whatever the UI or a hand-edited session sent us.
pub fn normalize(encoding: &str) -> &'static str {
    match encoding.trim().to_ascii_lowercase().as_str() {
        CP437 | "437" | "ibm437" | "oem" => CP437,
        WINDOWS_1252 | "1252" | "cp1252" | "latin1" | "iso-8859-1" => WINDOWS_1252,
        _ => UTF8,
    }
}

/// The upper half of cp437: byte 0x80 + index.
#[rustfmt::skip]
const CP437_HIGH: [char; 128] = [
    '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}', '\u{00E7}',
    '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}', '\u{00C5}',
    '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}', '\u{00F9}',
    '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}', '\u{0192}',
    '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}', '\u{00BA}',
    '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}', '\u{00BB}',
    '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
    '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}', '\u{2510}',
    '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}', '\u{255F}',
    '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}', '\u{2567}',
    '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256B}',
    '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}', '\u{2580}',
    '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}', '\u{03C4}',
    '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}', '\u{2229}',
    '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}', '\u{2248}',
    '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}', '\u{00A0}',
];

/// The upper half of windows_1252: byte 0x80 + index.
#[rustfmt::skip]
const WINDOWS_1252_HIGH: [char; 128] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
    '\u{00A0}', '\u{00A1}', '\u{00A2}', '\u{00A3}', '\u{00A4}', '\u{00A5}', '\u{00A6}', '\u{00A7}',
    '\u{00A8}', '\u{00A9}', '\u{00AA}', '\u{00AB}', '\u{00AC}', '\u{00AD}', '\u{00AE}', '\u{00AF}',
    '\u{00B0}', '\u{00B1}', '\u{00B2}', '\u{00B3}', '\u{00B4}', '\u{00B5}', '\u{00B6}', '\u{00B7}',
    '\u{00B8}', '\u{00B9}', '\u{00BA}', '\u{00BB}', '\u{00BC}', '\u{00BD}', '\u{00BE}', '\u{00BF}',
    '\u{00C0}', '\u{00C1}', '\u{00C2}', '\u{00C3}', '\u{00C4}', '\u{00C5}', '\u{00C6}', '\u{00C7}',
    '\u{00C8}', '\u{00C9}', '\u{00CA}', '\u{00CB}', '\u{00CC}', '\u{00CD}', '\u{00CE}', '\u{00CF}',
    '\u{00D0}', '\u{00D1}', '\u{00D2}', '\u{00D3}', '\u{00D4}', '\u{00D5}', '\u{00D6}', '\u{00D7}',
    '\u{00D8}', '\u{00D9}', '\u{00DA}', '\u{00DB}', '\u{00DC}', '\u{00DD}', '\u{00DE}', '\u{00DF}',
    '\u{00E0}', '\u{00E1}', '\u{00E2}', '\u{00E3}', '\u{00E4}', '\u{00E5}', '\u{00E6}', '\u{00E7}',
    '\u{00E8}', '\u{00E9}', '\u{00EA}', '\u{00EB}', '\u{00EC}', '\u{00ED}', '\u{00EE}', '\u{00EF}',
    '\u{00F0}', '\u{00F1}', '\u{00F2}', '\u{00F3}', '\u{00F4}', '\u{00F5}', '\u{00F6}', '\u{00F7}',
    '\u{00F8}', '\u{00F9}', '\u{00FA}', '\u{00FB}', '\u{00FC}', '\u{00FD}', '\u{00FE}', '\u{00FF}',
];

fn high_table(encoding: &str) -> Option<&'static [char; 128]> {
    match encoding {
        CP437 => Some(&CP437_HIGH),
        WINDOWS_1252 => Some(&WINDOWS_1252_HIGH),
        _ => None,
    }
}

/// Decodes bytes in a known encoding.
///
/// Returns the text and whether anything had to be replaced. A single-byte
/// code page can represent every possible byte, so only UTF-8 can be lossy.
pub fn decode(bytes: &[u8], encoding: &str) -> (String, bool) {
    let encoding = normalize(encoding);
    let Some(table) = high_table(encoding) else {
        return match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_string(), false),
            Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
        };
    };
    let text = bytes
        .iter()
        .map(|&byte| {
            if byte < 0x80 {
                byte as char
            } else {
                table[(byte - 0x80) as usize]
            }
        })
        .collect();
    (text, false)
}

/// Encodes text back to a known encoding.
///
/// ⚠️ Returns the first character that the target cannot represent rather than
/// substituting it. Typing an emoji into an NFO and silently saving a `?` in
/// its place is the kind of quiet corruption this editor exists to avoid; the
/// shell turns this into "save as UTF-8 instead?".
pub fn encode(text: &str, encoding: &str) -> Result<Vec<u8>, char> {
    let encoding = normalize(encoding);
    let Some(table) = high_table(encoding) else {
        return Ok(text.as_bytes().to_vec());
    };
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        if (ch as u32) < 0x80 {
            out.push(ch as u8);
            continue;
        }
        match table.iter().position(|&candidate| candidate == ch) {
            Some(index) => out.push(0x80 + index as u8),
            None => return Err(ch),
        }
    }
    Ok(out)
}

/// Guesses an encoding for bytes that are not valid UTF-8.
///
/// ⚠️ Only ever consulted when UTF-8 has already failed, so this is not a
/// general charset detector and must not become one. The question it answers
/// is narrow: DOS-era art, or Western European text from Windows?
pub fn sniff(bytes: &[u8]) -> &'static str {
    if std::str::from_utf8(bytes).is_ok() {
        return UTF8;
    }
    // CP437's block and box-drawing characters live at 0xB0-0xDF, and a file
    // made of them is art. Windows-1252 text uses high bytes too, but spread
    // across the accented letters and punctuation instead of clustered here.
    let high = bytes.iter().filter(|&&b| b >= 0x80).count();
    if high == 0 {
        return WINDOWS_1252;
    }
    let drawing = bytes
        .iter()
        .filter(|&&b| (0xB0..=0xDF).contains(&b))
        .count();
    if drawing * 2 >= high {
        CP437
    } else {
        WINDOWS_1252
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole feature rests on this: read a byte, write it back unchanged.
    /// A single wrong entry in a table would corrupt somebody's file on save,
    /// and nothing else in the app would notice.
    #[test]
    fn every_byte_survives_a_round_trip() {
        for encoding in [CP437, WINDOWS_1252] {
            let all: Vec<u8> = (0u8..=255).collect();
            let (text, lossy) = decode(&all, encoding);
            assert!(!lossy, "{encoding} can represent every byte");
            assert_eq!(text.chars().count(), 256);
            let back = encode(&text, encoding).expect("round trip");
            assert_eq!(back, all, "{encoding} did not survive a round trip");
        }
    }

    #[test]
    fn a_line_of_ansi_art_decodes_to_box_drawing() {
        // Exactly the bytes an NFO file holds.
        let bytes = [0xC9, 0xCD, 0xCD, 0xCD, 0xBB];
        let (text, lossy) = decode(&bytes, CP437);
        assert_eq!(text, "╔═══╗");
        assert!(!lossy);
        // And the same bytes as UTF-8 are what the editor used to show.
        let (broken, lossy) = decode(&bytes, UTF8);
        assert!(lossy);
        assert!(broken.contains('\u{FFFD}'), "this is the bug being fixed");
    }

    #[test]
    fn windows_1252_punctuation_is_not_latin1() {
        // 0x92 is a right single quote in Windows-1252 and undefined in
        // ISO-8859-1 — getting this wrong turns every apostrophe in a Windows
        // text file into a control character.
        let (text, _) = decode(&[b'I', 0x92, b'm'], WINDOWS_1252);
        assert_eq!(text, "I\u{2019}m");
    }

    #[test]
    fn a_character_the_code_page_cannot_hold_is_refused_not_mangled() {
        let problem = encode("hello 😀", CP437).unwrap_err();
        assert_eq!(problem, '😀');
        // ASCII is always fine.
        assert_eq!(encode("hello", CP437).unwrap(), b"hello");
    }

    #[test]
    fn ansi_art_is_sniffed_as_cp437_and_accented_text_as_windows_1252() {
        let art = [0xC9, 0xCD, 0xCD, 0xBB, 0xDB, 0xB1, 0xB0];
        assert_eq!(sniff(&art), CP437);

        // "Café résumé" in Windows-1252: high bytes, but none of them drawing.
        let text = [b'C', b'a', b'f', 0xE9, b' ', b'r', 0xE9, b's', 0xFC, b'm'];
        assert_eq!(sniff(&text), WINDOWS_1252);

        assert_eq!(sniff("plain ascii".as_bytes()), UTF8);
        assert_eq!(sniff("héllo".as_bytes()), UTF8, "valid UTF-8 stays UTF-8");
    }

    #[test]
    fn an_unknown_name_falls_back_to_utf8_rather_than_failing() {
        assert_eq!(normalize("klingon"), UTF8);
        assert_eq!(normalize("IBM437"), CP437);
        assert_eq!(normalize("  CP1252 "), WINDOWS_1252);
    }
}
