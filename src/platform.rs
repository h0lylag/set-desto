#[cfg(target_os = "windows")]
pub fn attach_parent_console() {
    windows::attach_parent_console();
}

#[cfg(not(target_os = "windows"))]
pub fn attach_parent_console() {}

#[cfg(target_os = "windows")]
mod windows {
    #![allow(unsafe_code)]

    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
    }

    pub fn attach_parent_console() {
        // Windows GUI-subsystem binaries do not create a companion console.
        // If launched from an existing terminal, attach to it so clap/tracing
        // output still has somewhere useful to go. Explorer launches simply
        // fail this call and remain window-only.
        unsafe {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}
