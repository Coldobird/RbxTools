//! Small safe wrappers over the Win32 calls shared across the app.

use std::{
    ffi::{OsStr, OsString},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        process::CommandExt,
    },
    path::PathBuf,
    process::Command,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
    System::{
        Registry::{RegGetValueW, HKEY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ},
        Threading::{QueryFullProcessImageNameW, CREATE_NO_WINDOW},
    },
};

/// A NUL-terminated UTF-16 copy of `text`, for Win32 string parameters.
pub fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// `operation` followed by the calling thread's last Win32 error.
pub fn last_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

/// Starts `command` without flashing a console window.
pub fn hide_console(command: &mut Command) -> &mut Command {
    command.creation_flags(CREATE_NO_WINDOW)
}

/// A kernel handle that is closed when dropped.
pub struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes ownership of `raw`, or returns `None` for a null or invalid handle.
    pub fn new(raw: HANDLE) -> Option<Self> {
        (!raw.is_null() && raw != INVALID_HANDLE_VALUE).then_some(Self(raw))
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }

    /// The full path of the process this handle refers to.
    pub fn image_path(&self) -> Option<PathBuf> {
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        // SAFETY: the handle is open, and `length` is the buffer's capacity.
        if unsafe { QueryFullProcessImageNameW(self.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
            return None;
        }
        Some(PathBuf::from(OsString::from_wide(
            &buffer[..length as usize],
        )))
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle is valid and owned exclusively by this value.
        unsafe { CloseHandle(self.0) };
    }
}

/// Reads single values from the Windows registry.
pub struct Registry;

impl Registry {
    /// A `REG_SZ` value; `flags` may add options such as `RRF_SUBKEY_WOW6432KEY`.
    pub fn string(root: HKEY, key: &str, value: &str, flags: u32) -> Option<String> {
        let mut buffer = [0u16; 2048];
        let mut size = std::mem::size_of_val(&buffer) as u32;
        // SAFETY: the strings are NUL-terminated and `size` is the buffer's byte length.
        let result = unsafe {
            RegGetValueW(
                root,
                wide(key).as_ptr(),
                wide(value).as_ptr(),
                RRF_RT_REG_SZ | flags,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if result != 0 {
            return None;
        }
        let end = buffer
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    /// A `REG_DWORD` value.
    pub fn dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the strings are NUL-terminated and `data` holds exactly one DWORD.
        let result = unsafe {
            RegGetValueW(
                root,
                wide(key).as_ptr(),
                wide(value).as_ptr(),
                RRF_RT_REG_DWORD,
                ptr::null_mut(),
                (&mut data as *mut u32).cast(),
                &mut size,
            )
        };
        (result == 0).then_some(data)
    }
}
