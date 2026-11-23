//! Identify the game executable without mistaking a launcher or an argument
//! mentioning the game for the process that owns its network sockets.

fn basename(value: &str) -> &str {
    value
        .trim()
        .trim_matches('"')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
}

fn is_game_executable(value: &str) -> bool {
    let name: String = basename(value)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect();
    matches!(
        name.as_str(),
        "herosiege" | "herosiegeexe" | "herosiegex86" | "herosiegex8664"
    )
}

/// `exe` is the native process executable. Under Wine that can be a loader,
/// while argv[0] or its first positional argument names the Windows executable.
pub fn is_game_process(name: &str, exe: Option<&str>, command: &[&str]) -> bool {
    if is_game_executable(name) || exe.is_some_and(is_game_executable) {
        return true;
    }
    let Some(first) = command.first() else {
        return false;
    };
    if is_game_executable(first) {
        return true;
    }
    let loader = basename(first).to_ascii_lowercase();
    if !matches!(
        loader.as_str(),
        "wine" | "wine64" | "wine-preloader" | "wine64-preloader"
    ) {
        return false;
    }
    // Do not scan arbitrary later arguments: a browser, shell or Steam launch
    // wrapper can mention the game without owning any of its connections.
    command
        .iter()
        .skip(1)
        .find(|arg| !arg.starts_with('-'))
        .is_some_and(|arg| is_game_executable(arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_windows_game_names_are_recognized() {
        assert!(is_game_process("Hero_Siege.exe", None, &[]));
        assert!(is_game_process("HeroSiege", None, &[]));
        assert!(is_game_process(
            "Hero_Siege.x86_",
            Some("/games/HeroSiege/Hero_Siege.x86_64"),
            &[]
        ));
        assert!(is_game_process(
            "wrapper",
            Some(r"E:\Steam\HeroSiege\Hero_Siege.exe"),
            &[]
        ));
    }

    #[test]
    fn proton_can_identify_a_windows_path_in_argv_zero_or_a_wine_loader() {
        assert!(is_game_process(
            "wine64-preloader",
            Some("/usr/bin/wine64-preloader"),
            &[r"Z:\games\HeroSiege\Hero_Siege.exe"]
        ));
        assert!(is_game_process(
            "wine64-preloader",
            Some("/usr/bin/wine64-preloader"),
            &["wine64-preloader", r"Z:\games\HeroSiege\Hero_Siege.exe"]
        ));
        assert!(is_game_process(
            "wine64-preloader",
            None,
            &[
                "/proton/files/bin/wine64",
                "/steam/steamapps/common/HeroSiege/Hero_Siege.exe",
                "--some-game-option"
            ]
        ));
    }

    #[test]
    fn command_mentions_and_similar_tools_are_not_game_processes() {
        for name in [
            "HeroSiegeUpdater.exe",
            "HeroSiegeLauncher",
            "HeroSiege Extractor",
            "HeroSiegeBrowser",
        ] {
            assert!(!is_game_process(name, None, &[]), "{name}");
        }
        assert!(!is_game_process(
            "chrome",
            Some("/usr/bin/chrome"),
            &["chrome", r"Z:\games\HeroSiege\Hero_Siege.exe"]
        ));
        assert!(!is_game_process(
            "sh",
            None,
            &["sh", "-c", "Hero_Siege.exe"]
        ));
        assert!(!is_game_process(
            "python3",
            None,
            &[
                "python3",
                "proton",
                "waitforexitandrun",
                "/games/Hero_Siege.exe"
            ]
        ));
        assert!(!is_game_process(
            "wine64",
            None,
            &["wine64", "notepad.exe", "Hero_Siege.exe"]
        ));
    }
}
