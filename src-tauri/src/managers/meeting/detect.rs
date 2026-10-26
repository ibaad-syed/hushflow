//! Call detection: which other apps are using the microphone right now.
//!
//! macOS 14.2+: Core Audio's process objects say, per process, whether it is
//! taking input, with its PID and bundle ID. Windows: the privacy settings'
//! records (CapabilityAccessManager) mark an app's microphone use as ongoing
//! until it stops. Hushflow's own use never counts.

/// An app using the microphone, by the name a person knows it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    /// Stable key for "the same app is still on the call".
    pub key: String,
    pub name: String,
}

/// Bundle-ID prefixes (macOS) or executable names (Windows) to friendly names.
/// Browsers record from helper processes, so prefixes match those too.
const KNOWN: &[(&str, &str)] = &[
    ("com.google.chrome", "Google Chrome"),
    ("chrome.exe", "Google Chrome"),
    ("com.apple.webkit", "Safari"),
    ("com.apple.safari", "Safari"),
    ("com.apple.facetime", "FaceTime"),
    ("company.thebrowser", "Arc"),
    ("org.mozilla.firefox", "Firefox"),
    ("firefox.exe", "Firefox"),
    ("com.microsoft.edgemac", "Microsoft Edge"),
    ("msedge.exe", "Microsoft Edge"),
    ("com.brave.browser", "Brave"),
    ("brave.exe", "Brave"),
    ("us.zoom", "Zoom"),
    ("zoom.exe", "Zoom"),
    ("com.microsoft.teams", "Microsoft Teams"),
    ("msteams", "Microsoft Teams"),
    ("ms-teams.exe", "Microsoft Teams"),
    ("com.tinyspeck.slackmacgap", "Slack"),
    ("slack.exe", "Slack"),
    ("com.hnc.discord", "Discord"),
    ("discord.exe", "Discord"),
    ("com.cisco.webex", "Webex"),
    ("ciscocollabhost.exe", "Webex"),
    ("net.whatsapp", "WhatsApp"),
    ("whatsapp", "WhatsApp"),
];

/// macOS system services that touch the mic without being a call (Siri,
/// dictation, the menu-bar indicator). Other com.apple IDs count as calls only
/// when listed in KNOWN.
#[cfg(target_os = "macos")]
fn is_system_service(id: &str) -> bool {
    let id = id.to_lowercase();
    id.starts_with("com.apple.") && !KNOWN.iter().any(|(k, _)| id.starts_with(k))
}

/// The friendly name for an app id, or a tidied fallback.
pub fn friendly_name(id: &str) -> String {
    // "com.example.SomeApp.helper" → "SomeApp"; "C:\...\Thing.exe" → "Thing".
    let base = id.rsplit(['\\', '/', '#']).next().unwrap_or(id);
    let (lower, lower_base) = (id.to_lowercase(), base.to_lowercase());
    if let Some((_, name)) = KNOWN
        .iter()
        .find(|(k, _)| lower.starts_with(k) || lower_base.starts_with(k))
    {
        return name.to_string();
    }
    let base = base.strip_suffix(".exe").unwrap_or(base);
    let parts: Vec<&str> = base.split('.').collect();
    let pick = parts
        .iter()
        .rev()
        .find(|p| {
            !matches!(
                p.to_lowercase().as_str(),
                "helper" | "renderer" | "gpu" | "app"
            )
        })
        .copied()
        .unwrap_or(base);
    pick.to_string()
}

fn dedupe(users: Vec<MicUser>) -> Vec<MicUser> {
    let mut out: Vec<MicUser> = Vec::new();
    for u in users {
        if !out.iter().any(|o| o.name == u.name) {
            out.push(u);
        }
    }
    out
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::ffi::c_void;

    type AudioObjectId = u32;
    #[repr(C)]
    struct PropertyAddress {
        selector: u32,
        scope: u32,
        element: u32,
    }
    const fn fourcc(s: &[u8; 4]) -> u32 {
        ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
    }
    const SYSTEM_OBJECT: AudioObjectId = 1;
    const SCOPE_GLOBAL: u32 = fourcc(b"glob");
    const ELEMENT_MAIN: u32 = 0;
    const PROCESS_LIST: u32 = fourcc(b"prs#");
    const PROCESS_PID: u32 = fourcc(b"ppid");
    const PROCESS_BUNDLE_ID: u32 = fourcc(b"pbid");
    const PROCESS_RUNNING_INPUT: u32 = fourcc(b"piri");

    #[link(name = "CoreAudio", kind = "framework")]
    extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: AudioObjectId,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            id: AudioObjectId,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringGetCString(s: *const c_void, buf: *mut u8, size: isize, encoding: u32) -> u8;
        fn CFRelease(cf: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;
    extern "C" {
        /// libproc: a process's executable name.
        fn proc_name(pid: i32, buffer: *mut c_void, size: u32) -> i32;
    }

    /// For processes without a bundle (command-line tools).
    fn process_name(pid: i32) -> Option<String> {
        let mut buf = [0u8; 256];
        let n = unsafe { proc_name(pid, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        (n > 0).then(|| String::from_utf8_lossy(&buf[..n as usize]).to_string())
    }

    fn address(selector: u32) -> PropertyAddress {
        PropertyAddress {
            selector,
            scope: SCOPE_GLOBAL,
            element: ELEMENT_MAIN,
        }
    }

    fn get<T: Default>(id: AudioObjectId, selector: u32) -> Option<T> {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                id,
                &address(selector),
                0,
                std::ptr::null(),
                &mut size,
                &mut value as *mut T as *mut c_void,
            )
        };
        (status == 0).then_some(value)
    }

    fn bundle_id(id: AudioObjectId) -> Option<String> {
        let cf: usize = get(id, PROCESS_BUNDLE_ID)?;
        if cf == 0 {
            return None;
        }
        let mut buf = [0u8; 512];
        let ok = unsafe {
            let ok = CFStringGetCString(cf as *const c_void, buf.as_mut_ptr(), 512, UTF8);
            CFRelease(cf as *const c_void);
            ok
        };
        if ok == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0).unwrap_or(0);
        let s = String::from_utf8_lossy(&buf[..end]).to_string();
        (!s.is_empty()).then_some(s)
    }

    #[cfg(test)]
    pub fn debug_dump() {
        for id in process_ids() {
            println!(
                "obj {id}: pid={:?} input={:?} bundle={:?}",
                get::<i32>(id, PROCESS_PID),
                get::<u32>(id, PROCESS_RUNNING_INPUT),
                bundle_id(id)
            );
        }
    }

    fn process_ids() -> Vec<AudioObjectId> {
        let mut size = 0u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                SYSTEM_OBJECT,
                &address(PROCESS_LIST),
                0,
                std::ptr::null(),
                &mut size,
            )
        };
        // Before macOS 14.2 there are no process objects: nothing to report.
        if status != 0 || size == 0 {
            return Vec::new();
        }
        let mut ids =
            vec![0 as AudioObjectId; size as usize / std::mem::size_of::<AudioObjectId>()];
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &address(PROCESS_LIST),
                0,
                std::ptr::null(),
                &mut size,
                ids.as_mut_ptr() as *mut c_void,
            )
        };
        if status != 0 {
            return Vec::new();
        }
        ids
    }

    pub fn mic_users() -> Vec<MicUser> {
        let ours = std::process::id() as i32;
        let users = process_ids()
            .into_iter()
            .filter(|&id| get::<u32>(id, PROCESS_RUNNING_INPUT).unwrap_or(0) != 0)
            .filter_map(|id| {
                let pid = get::<i32>(id, PROCESS_PID)?;
                if pid == ours {
                    return None;
                }
                bundle_id(id).or_else(|| process_name(pid))
            })
            .filter(|b| !b.starts_with("com.hushflow") && !is_system_service(b))
            .map(|b| MicUser {
                name: friendly_name(&b),
                key: b,
            })
            .collect();
        dedupe(users)
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    /// Apps whose last microphone use hasn't ended. Packaged apps are
    /// subkeys of the store; desktop apps sit under NonPackaged, named by
    /// their path with '#' for '\'.
    pub fn mic_users() -> Vec<MicUser> {
        let Ok(store) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(STORE) else {
            return Vec::new();
        };
        let mut users = Vec::new();
        let mut scan = |key: &RegKey| {
            for name in key.enum_keys().flatten() {
                if name == "NonPackaged" {
                    continue;
                }
                let Ok(app) = key.open_subkey(&name) else {
                    continue;
                };
                let start: u64 = app.get_value("LastUsedTimeStart").unwrap_or(0);
                let stop: u64 = app.get_value("LastUsedTimeStop").unwrap_or(1);
                let lower = name.to_lowercase();
                if start > 0
                    && stop == 0
                    && !lower.ends_with("handy.exe")
                    && !lower.contains("hushflow")
                {
                    users.push(MicUser {
                        name: friendly_name(&name),
                        key: name,
                    });
                }
            }
        };
        scan(&store);
        if let Ok(desktop) = store.open_subkey("NonPackaged") {
            scan(&desktop);
        }
        dedupe(users)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::*;
    pub fn mic_users() -> Vec<MicUser> {
        Vec::new()
    }
}

pub use platform::mic_users;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers_and_paths_get_the_app_name() {
        assert_eq!(friendly_name("com.google.Chrome.helper"), "Google Chrome");
        assert_eq!(friendly_name("com.apple.WebKit.GPU"), "Safari");
        assert_eq!(friendly_name("us.zoom.xos"), "Zoom");
        assert_eq!(
            friendly_name("C:#Program Files#Google#Chrome#Application#chrome.exe"),
            "Google Chrome"
        );
        assert_eq!(friendly_name("com.example.Whereby.helper"), "Whereby");
        assert_eq!(friendly_name("C:#Tools#Mumble.exe"), "Mumble");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn system_services_are_not_calls() {
        assert!(is_system_service("com.apple.siri"));
        assert!(!is_system_service("com.apple.FaceTime"));
        assert!(!is_system_service("com.google.Chrome.helper"));
    }

    /// Live check on this machine: every process taking input, unfiltered.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn print_raw_input_processes() {
        platform::debug_dump();
    }

    /// Live check on this machine: prints who is using the mic.
    #[test]
    #[ignore]
    fn print_mic_users() {
        println!("{:?}", mic_users());
    }
}
