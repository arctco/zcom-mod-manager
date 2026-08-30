//! Turning a file name into something a person recognizes.
//!
//! Mods arrive as archives whose names carry publishing metadata, and as
//! folders whose names are written for a file system rather than for a reader.
//! Everything here is presentation only: nothing that is derived affects where
//! a file is deployed.

/// Splits `CamelCase` and `snake_case` into words without breaking initialisms,
/// so `UniqueTalentsForAll` reads as "Unique Talents For All" and `ZCUnlocked`
/// as "ZC Unlocked" rather than "Z C Unlocked".
pub fn display_name(raw: &str) -> String {
    let trimmed = raw
        .trim()
        .trim_end_matches("_P")
        .trim_end_matches("_p")
        .trim_start_matches("pakchunk99-");
    let characters: Vec<char> = trimmed.chars().collect();
    let mut result = String::new();
    for (index, &character) in characters.iter().enumerate() {
        let previous = index.checked_sub(1).map(|i| characters[i]);
        let next = characters.get(index + 1).copied();
        let boundary = character.is_uppercase()
            && match previous {
                Some(previous) if previous.is_lowercase() => true,
                Some(previous) if previous.is_uppercase() => next.is_some_and(char::is_lowercase),
                _ => false,
            };
        if boundary && !result.ends_with(' ') {
            result.push(' ');
        }
        result.push(match character {
            '_' | '-' | '.' => ' ',
            other => other,
        });
    }
    let collapsed = result.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        "Unnamed Mod".into()
    } else {
        collapsed
    }
}

fn is_version(token: &str) -> bool {
    let token = token.trim_start_matches(['v', 'V']);
    !token.is_empty()
        && token.starts_with(|c: char| c.is_ascii_digit())
        && token.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn is_timestamp(token: &str) -> bool {
    // The Nexus download name ends with an upload stamp such as
    // `2026-08-30T09-27Z`, which is the reliable marker for where the
    // publishing metadata starts.
    let bytes = token.as_bytes();
    token.len() == 17
        && token.ends_with('Z')
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b'-'
        && bytes
            .iter()
            .enumerate()
            .filter(|(index, _)| ![4, 7, 10, 13, 16].contains(index))
            .all(|(_, byte)| byte.is_ascii_digit())
}

/// Recovers the published title and version from a download file name.
///
/// Nexus Mods hands out two shapes, and both bury the title under identifiers
/// that mean nothing to the person installing the mod:
///
/// * `UniqueTalentsForAll V08 Beta 38 0.8 2026-08-30T09-27Z uk40p8I6P`
/// * `ZCUnlocked-34-1-3-1756542720`
pub fn from_source_name(stem: &str) -> (Option<String>, Option<String>) {
    // A browser that downloaded the same file twice appends "(1)".
    let mut cleaned = stem.trim();
    while cleaned.ends_with(')') {
        match cleaned.rfind('(') {
            Some(open)
                if cleaned[open + 1..cleaned.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
            {
                cleaned = cleaned[..open].trim_end()
            }
            _ => break,
        }
    }
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if let Some(stamp) = tokens.iter().position(|token| is_timestamp(token)) {
        let mut cut = stamp;
        let mut version = None;
        if cut > 0 && is_version(tokens[cut - 1]) {
            version = Some(tokens[cut - 1].trim_start_matches(['v', 'V']).to_string());
            cut -= 1;
        }
        // The remaining number is the Nexus mod id.
        if cut > 0 && tokens[cut - 1].chars().all(|c| c.is_ascii_digit()) {
            cut -= 1;
        }
        let name = tokens[..cut].join(" ");
        return (
            (!name.trim().is_empty()).then(|| display_name(&name)),
            version,
        );
    }
    if tokens.len() == 1 {
        let parts: Vec<&str> = cleaned.split('-').collect();
        let numeric = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
        // `Name-<mod id>-<version parts>-<unix upload time>`
        if parts.len() >= 4
            && parts
                .last()
                .is_some_and(|last| numeric(last) && last.len() >= 9)
        {
            let keep = parts
                .iter()
                .position(|part| numeric(part))
                .filter(|index| *index > 0);
            if let Some(keep) = keep {
                let name = parts[..keep].join("-");
                let version = parts[keep + 1..parts.len() - 1].join(".");
                return (
                    (!name.trim().is_empty()).then(|| display_name(&name)),
                    (!version.is_empty()).then_some(version),
                );
            }
        }
    }
    (
        (!cleaned.trim().is_empty()).then(|| display_name(cleaned)),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words_without_breaking_initialisms() {
        assert_eq!(
            display_name("UniqueTalentsForAll"),
            "Unique Talents For All"
        );
        assert_eq!(display_name("ZCUnlocked"), "ZC Unlocked");
        assert_eq!(display_name("BetterActions_P"), "Better Actions");
        assert_eq!(display_name("no_intro"), "no intro");
        assert_eq!(display_name("  "), "Unnamed Mod");
    }

    #[test]
    fn recovers_the_title_from_a_nexus_download() {
        assert_eq!(
            from_source_name("UniqueTalentsForAll V08 Beta 38 0.8 2026-08-30T09-27Z uk40p8I6P"),
            (
                Some("Unique Talents For All V08 Beta".into()),
                Some("0.8".into())
            )
        );
        assert_eq!(
            from_source_name("ZCUnlocked 34 1.3 2026-08-30T07-32Z i9WZfkaQ7"),
            (Some("ZC Unlocked".into()), Some("1.3".into()))
        );
        assert_eq!(
            from_source_name(
                "UE4SS For Star Wars Zero Company 9 1.0 2026-08-27T16-44Z 3SLfClRiS(1)"
            ),
            (
                Some("UE4SS For Star Wars Zero Company".into()),
                Some("1.0".into())
            )
        );
    }

    #[test]
    fn recovers_the_title_from_the_classic_nexus_shape() {
        assert_eq!(
            from_source_name("ZCUnlocked-34-1-3-1756542720"),
            (Some("ZC Unlocked".into()), Some("1.3".into()))
        );
    }

    #[test]
    fn leaves_an_ordinary_name_alone() {
        assert_eq!(
            from_source_name("TrueLight Shadows"),
            (Some("True Light Shadows".into()), None)
        );
        assert_eq!(
            from_source_name("no-intro-mod"),
            (Some("no intro mod".into()), None)
        );
    }
}
