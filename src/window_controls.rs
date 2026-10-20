use gpui::Window;

#[cfg(target_os = "windows")]
fn window_handle(window: &Window) -> Result<windows::Win32::Foundation::HWND, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("当前窗口不是 Windows 窗口。".into());
    };
    Ok(windows::Win32::Foundation::HWND(handle.hwnd.get() as *mut _))
}

pub fn remove_border(window: &Window) {
    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = window_handle(window) {
        use windows::Win32::Graphics::Dwm::{DWMWA_BORDER_COLOR, DwmSetWindowAttribute};
        let no_border = 0xfffffffe_u32;
        // SAFETY: this is the live window on its UI thread; the initialized color
        // remains valid for the whole synchronous call. Older Windows may reject it.
        unsafe {
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                std::ptr::from_ref(&no_border).cast(),
                std::mem::size_of_val(&no_border) as u32,
            );
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = window;
}

/// Keep the window visible while changing its taskbar entry on the UI thread.
pub fn set_taskbar_visible(window: &Window, visible: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::{
            Foundation::{ERROR_SUCCESS, GetLastError, SetLastError},
            UI::WindowsAndMessaging::{
                GWL_EXSTYLE, GetWindowLongPtrW, IsWindowVisible, SW_HIDE, SW_SHOW,
                SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
            },
        };

        let hwnd = window_handle(window)?;

        // SAFETY: the HWND is borrowed from this live GPUI window on its UI thread.
        // Only style flags are changed; no handle or pointer is retained after this call.
        unsafe {
            SetLastError(ERROR_SUCCESS);
            let previous = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if previous == 0 && GetLastError() != ERROR_SUCCESS {
                return Err(windows::core::Error::from_win32().to_string());
            }
            let next = if visible {
                (previous & !(WS_EX_TOOLWINDOW.0 as isize)) | WS_EX_APPWINDOW.0 as isize
            } else {
                (previous & !(WS_EX_APPWINDOW.0 as isize)) | WS_EX_TOOLWINDOW.0 as isize
            };
            let was_visible = IsWindowVisible(hwnd).as_bool();
            if was_visible {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
            SetLastError(ERROR_SUCCESS);
            let result = if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next) == 0
                && GetLastError() != ERROR_SUCCESS
            {
                Err(windows::core::Error::from_win32())
            } else {
                SetWindowPos(hwnd, None, 0, 0, 0, 0, flags)
            };
            if result.is_err() {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, previous);
                let _ = SetWindowPos(hwnd, None, 0, 0, 0, 0, flags);
            }
            if was_visible {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            result.map_err(|error| error.to_string())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, visible);
        Err("任务栏切换仅支持 Windows。".into())
    }
}
