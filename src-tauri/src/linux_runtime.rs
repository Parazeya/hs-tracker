//! Optional Linux runtime dependencies must not abort the dashboard at startup.
//!
//! libappindicator-sys panics from its lazy loader if none of these sonames can
//! be opened. Its tray constructor therefore cannot report that as a Result.
//! Probe the same ordered list before invoking it.

const INDICATOR_LIBRARIES: [&str; 4] = [
    "libayatana-appindicator3.so.1",
    "libappindicator3.so.1",
    "libayatana-appindicator3.so",
    "libappindicator3.so",
];

enum Probe {
    Missing(String),
    Compatible,
    Incompatible(String),
}

fn check_with(mut probe: impl FnMut(&str) -> Probe) -> Result<(), String> {
    let mut failures = Vec::new();
    for library in INDICATOR_LIBRARIES {
        match probe(library) {
            Probe::Compatible => return Ok(()),
            // The dependency would select the first successfully loaded
            // library, too. Trying a later library here would hide its failure.
            Probe::Incompatible(reason) => {
                return Err(format!("tray unavailable: {library}: {reason}"))
            }
            Probe::Missing(reason) => failures.push(format!("{library}: {reason}")),
        }
    }
    Err(format!(
        "tray unavailable; install libayatana-appindicator or libappindicator: {}",
        failures.join("; ")
    ))
}

#[cfg(target_os = "linux")]
pub fn check_indicator() -> Result<(), String> {
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int, c_void};
    #[link(name = "dl")]
    extern "C" {
        fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    let error = || unsafe {
        let message = dlerror();
        if message.is_null() {
            "dynamic loader refused this library".into()
        } else {
            CStr::from_ptr(message).to_string_lossy().into_owned()
        }
    };
    check_with(|name| {
        let name = CString::new(name).expect("fixed library name");
        let handle = unsafe { dlopen(name.as_ptr(), 1) }; // RTLD_LAZY | RTLD_LOCAL
        if handle.is_null() {
            return Probe::Missing(error());
        }
        let missing = [
            c"app_indicator_new",
            c"app_indicator_set_status",
            c"app_indicator_set_icon_theme_path",
            c"app_indicator_set_icon_full",
            c"app_indicator_set_menu",
            c"app_indicator_set_label",
        ]
        .into_iter()
        .find(|symbol| unsafe { dlsym(handle, symbol.as_ptr()).is_null() });
        unsafe {
            dlclose(handle);
        }
        match missing {
            Some(symbol) => Probe::Incompatible(format!(
                "missing required symbol {}",
                symbol.to_string_lossy()
            )),
            None => Probe::Compatible,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_indicator_is_an_error_before_any_tray_constructor_runs() {
        let mut attempts = Vec::new();
        let result = check_with(|name| {
            attempts.push(name.to_string());
            Probe::Missing("not installed".into())
        });
        assert_eq!(attempts, INDICATOR_LIBRARIES);
        assert!(result.unwrap_err().contains("tray unavailable; install"));
    }
    #[test]
    fn a_working_legacy_library_is_accepted_after_absent_ayatana() {
        let mut attempts = 0;
        assert!(check_with(|name| {
            attempts += 1;
            if name == "libappindicator3.so.1" {
                Probe::Compatible
            } else {
                Probe::Missing("not installed".into())
            }
        })
        .is_ok());
        assert_eq!(attempts, 2);
    }
    #[test]
    fn an_incompatible_first_library_cannot_be_hidden_by_a_later_candidate() {
        let mut attempts = 0;
        let result = check_with(|_| {
            attempts += 1;
            Probe::Incompatible("missing app_indicator_new".into())
        });
        assert_eq!(attempts, 1);
        assert!(result.unwrap_err().contains("missing app_indicator_new"));
    }
}
