//! Project links are parsed before opening, and never passed to a shell.

pub fn validate(input: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(input).map_err(|_| "invalid project URL".to_string())?;
    let path = url.path();
    let tracker = url.host_str() == Some("github.com")
        && (path.eq_ignore_ascii_case("/Parazeya/hs-tracker")
            || path.to_ascii_lowercase().starts_with("/parazeya/hs-tracker/"));
    let map = url.host_str() == Some("parazeya.github.io")
        && matches!(path, "/hs-map/" | "/hs-map/index.html" | "/hs-map/codex.html");
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !(tracker || map)
    {
        return Err("that is not one of this project's pages".into());
    }
    Ok(url)
}

pub fn packaged_map_url(origin: &str, requested: &reqwest::Url) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(origin).map_err(|error| error.to_string())?;
    let path = if requested.path() == "/hs-map/" { "/hs-map/index.html" } else { requested.path() };
    url.set_path(path);
    url.set_query(requested.query());
    url.set_fragment(requested.fragment());
    Ok(url)
}

pub fn open(url: &reqwest::Url) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use std::{ffi::OsStr, ptr};
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                window: *mut std::ffi::c_void,
                verb: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show: i32,
            ) -> isize;
        }
        let wide = |s: &str| {
            OsStr::new(s)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>()
        };
        let verb = wide("open");
        let target = wide(url.as_str());
        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                verb.as_ptr(),
                target.as_ptr(),
                ptr::null(),
                ptr::null(),
                1,
            )
        };
        if result <= 32 {
            return Err(format!("could not open project URL ({result})"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        let program = "open";
        #[cfg(not(target_os = "macos"))]
        let program = "xdg-open";
        crate::reap(
            std::process::Command::new(program)
                .arg(url.as_str())
                .spawn(),
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_exact_project_paths_are_opened() {
        for good in [
            "https://github.com/Parazeya/hs-tracker",
            "https://github.com/Parazeya/hs-tracker/releases/tag/v1.1.15",
            "https://github.com/Parazeya/hs-tracker/issues?q=a&b=c",
            "https://parazeya.github.io/hs-map/?find=Crown+of+Doom",
            "https://parazeya.github.io/hs-map/codex.html?item=helmet_crown_of_doom",
        ] {
            assert!(validate(good).is_ok(), "{good}");
        }
        for bad in [
            "http://github.com/Parazeya/hs-tracker",
            "https://github.com/Parazeya/hs-tracker-evil",
            "https://github.com.evil/Parazeya/hs-tracker",
            "https://github.com@evil/Parazeya/hs-tracker",
            "https://user@github.com/Parazeya/hs-tracker",
            "https://github.com:444/Parazeya/hs-tracker",
            "https://github.com/Parazeya/hs-tracker/../../other/project",
            "https://parazeya.github.io/hs-map-other/codex.html",
            "https://parazeya.github.io/hs-map/../other/index.html",
        ] {
            assert!(validate(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn shell_characters_remain_url_data() {
        let parsed =
            validate("https://github.com/Parazeya/hs-tracker/issues?q=test&echo.INJECTED").unwrap();
        assert_eq!(parsed.query(), Some("q=test&echo.INJECTED"));
    }

    #[test]
    fn packaged_map_resolves_index_and_preserves_exact_target() {
        let origin = "http://127.0.0.1:41234";
        for (requested, expected) in [
            ("https://parazeya.github.io/hs-map/?source=Crystal+Chest", "/hs-map/index.html?source=Crystal+Chest"),
            ("https://parazeya.github.io/hs-map/codex.html?item=helmet_crown_of_doom", "/hs-map/codex.html?item=helmet_crown_of_doom"),
        ] {
            let url = packaged_map_url(origin, &validate(requested).unwrap()).unwrap();
            assert_eq!(url.as_str(), format!("{origin}{expected}"));
        }
    }
}
