//! Foreground executable identity for an opt-in sharing allowlist. No window
//! titles, control contents, clipboard or password fields are inspected.
#[cfg(target_os = "windows")]
pub fn foreground_executable_name() -> Option<String> {
    use windows::Win32::{
        Foundation::CloseHandle,
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
    };
    use windows::core::PWSTR;
    // SAFETY: only query an OS-owned foreground HWND/process. Access may be
    // denied or race process exit; every failure means no authorized identity.
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return None;
        }
        let mut id = 0;
        GetWindowThreadProcessId(foreground, Some(&mut id));
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, id).ok()?;
        let mut name = [0u16; 32768];
        let mut length = name.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(name.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        let path = String::from_utf16(&name[..length as usize]).ok()?;
        path.rsplit('\\').next().map(str::to_owned)
    }
}
