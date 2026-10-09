//! Blocking native hotkey reception: no keyboard polling while hidden.
#[cfg(windows)]
pub struct PhantomHotkey {
    thread_id: u32,
    thread: Option<std::thread::JoinHandle<()>>,
}
#[cfg(windows)]
impl PhantomHotkey {
    pub fn register(action: impl Fn() + Send + 'static) -> Result<Self, String> {
        use windows_sys::Win32::{
            System::Threading::GetCurrentThreadId,
            UI::{
                Input::KeyboardAndMouse::{
                    RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
                },
                WindowsAndMessaging::{GetMessageW, PeekMessageW, MSG, PM_NOREMOVE, WM_HOTKEY},
            },
        };
        let (ready, response) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || unsafe {
            let mut message: MSG = std::mem::zeroed();
            // Create the message queue before exposing the thread id for shutdown.
            PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            if RegisterHotKey(
                std::ptr::null_mut(),
                1,
                MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
                0x42,
            ) == 0
            {
                let _ = ready.send(Err(std::io::Error::last_os_error().to_string()));
                return;
            }
            let _ = ready.send(Ok(GetCurrentThreadId()));
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                if message.message == WM_HOTKEY && message.wParam == 1 {
                    action();
                }
            }
            UnregisterHotKey(std::ptr::null_mut(), 1);
        });
        match response.recv().map_err(|e| e.to_string())? {
            Ok(thread_id) => Ok(Self {
                thread_id,
                thread: Some(thread),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }
}
#[cfg(windows)]
impl Drop for PhantomHotkey {
    fn drop(&mut self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(not(windows))]
pub struct PhantomHotkey;
#[cfg(not(windows))]
impl PhantomHotkey {
    pub fn register(_: impl Fn() + Send + 'static) -> Result<Self, String> {
        Err(
            "Global Phantom shortcut is currently available on Windows; use the tray to restore."
                .into(),
        )
    }
}
