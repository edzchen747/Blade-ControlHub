use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::null_mut;

use serde::Serialize;
use serde::de::DeserializeOwned;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};

const MAX_FRAME_SIZE: usize = 64 * 1024;

pub struct PipeHandle(HANDLE);

impl PipeHandle {
    pub fn new(handle: HANDLE) -> io::Result<Self> {
        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    pub fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for PipeHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

pub fn write_json_frame<T: Serialize>(pipe: &PipeHandle, value: &T) -> io::Result<()> {
    let payload = serde_json::to_vec(value).map_err(invalid_data)?;
    if payload.len() > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame exceeded maximum size",
        ));
    }

    let len = (payload.len() as u32).to_le_bytes();
    write_all(pipe.raw(), &len)?;
    write_all(pipe.raw(), &payload)
}

pub fn read_json_frame<T: DeserializeOwned>(pipe: &PipeHandle) -> io::Result<T> {
    let mut len = [0u8; 4];
    read_exact(pipe.raw(), &mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame exceeded maximum size",
        ));
    }

    let mut payload = vec![0u8; len];
    read_exact(pipe.raw(), &mut payload)?;
    serde_json::from_slice(&payload).map_err(invalid_data)
}

fn write_all(handle: HANDLE, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let mut written = 0;
        let ok = unsafe {
            WriteFile(
                handle,
                bytes.as_ptr().cast(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if written == 0 {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "named pipe write returned zero bytes",
            ));
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

fn read_exact(handle: HANDLE, mut bytes: &mut [u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let mut read = 0;
        let ok = unsafe {
            ReadFile(
                handle,
                bytes.as_mut_ptr().cast(),
                bytes.len() as u32,
                &mut read,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "named pipe closed before frame completed",
            ));
        }
        let remaining = bytes;
        bytes = &mut remaining[read as usize..];
    }
    Ok(())
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
