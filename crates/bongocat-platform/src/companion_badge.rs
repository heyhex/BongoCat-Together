//! A bounded positioning worker avoids synchronously re-entering GPUI from
//! SetWindowPos while its App/Window is borrowed. Drop before closing the HWND.
use crate::NativeWindowError;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    sync::mpsc::{self, SyncSender},
    thread::JoinHandle,
};

pub struct CompanionBadgePositioner {
    sender: Option<SyncSender<(i32, i32, u32, u32)>>,
    worker: Option<JoinHandle<()>>,
}

impl CompanionBadgePositioner {
    /// The UI owner keeps its window alive until this RAII worker is dropped.
    pub fn start(window: &impl HasWindowHandle) -> Result<Self, NativeWindowError> {
        let raw = window
            .window_handle()
            .map_err(|_| NativeWindowError::HandleUnavailable)?;
        let RawWindowHandle::Win32(handle) = raw.as_raw() else {
            return Err(NativeWindowError::UnsupportedHandle);
        };
        let identity = handle.hwnd.get();
        let (sender, receiver) = mpsc::sync_channel::<(i32, i32, u32, u32)>(1);
        let worker = std::thread::Builder::new()
            .name("bongocat-badge-position".into())
            .spawn(move || {
                use windows::Win32::{
                    Foundation::{HWND, RECT},
                    Graphics::Gdi::{
                        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect,
                    },
                    UI::WindowsAndMessaging::{
                        HWND_TOPMOST, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SetWindowPos,
                    },
                };
                while let Ok(mut bounds) = receiver.recv() {
                    while let Ok(newer) = receiver.try_recv() {
                        bounds = newer;
                    }
                    let (mut x, mut y, width, height) = bounds;
                    let rect = RECT {
                        left: x,
                        top: y,
                        right: x.saturating_add_unsigned(width),
                        bottom: y.saturating_add_unsigned(height),
                    };
                    let mut info = MONITORINFO {
                        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                        ..Default::default()
                    };
                    // SAFETY: the UI owner keeps the HWND alive until this worker
                    // joins. ASYNCWINDOWPOS queues work on its different owner
                    // thread, so this call cannot dispatch GPUI callbacks here.
                    unsafe {
                        if GetMonitorInfoW(
                            MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST),
                            &mut info,
                        )
                        .as_bool()
                        {
                            x = x.clamp(
                                info.rcWork.left,
                                info.rcWork
                                    .right
                                    .saturating_sub(width as i32)
                                    .max(info.rcWork.left),
                            );
                            y = y.clamp(
                                info.rcWork.top,
                                info.rcWork
                                    .bottom
                                    .saturating_sub(height as i32)
                                    .max(info.rcWork.top),
                            );
                        }
                        let _ = SetWindowPos(
                            HWND(identity as *mut _),
                            Some(HWND_TOPMOST),
                            x,
                            y,
                            width as i32,
                            height as i32,
                            SWP_ASYNCWINDOWPOS | SWP_NOACTIVATE,
                        );
                    }
                }
            })
            .map_err(|_| NativeWindowError::HandleUnavailable)?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub fn update(&self, bounds: (i32, i32, u32, u32)) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(bounds);
        }
    }
}

impl Drop for CompanionBadgePositioner {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
