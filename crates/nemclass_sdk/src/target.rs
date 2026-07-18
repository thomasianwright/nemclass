//! [`Target`]: a headless handle to a process's memory.
//!
//! Unifies the two memory sources the GUI supports — the OS-native
//! [`OwnedProcess`] and a user-supplied "managed" plugin library (`yc_*`
//! exports) — behind one width-aware API. Policy such as *which* plugin path to
//! use lives in the caller (e.g. the GUI reads it from its config); this crate
//! only takes an explicit path.

use crate::error::{Result, SdkError};
use nemclass_memory::external::{MemoryRegion, OwnedProcess, ProcessIterator};
use nemclass_memory::Matcher;
use std::mem::{size_of, MaybeUninit};
use std::path::Path;
use std::slice;

/// Basic information about a running process.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process id.
    pub id: u32,
    /// Process (image) name.
    pub name: String,
    /// Parent process id.
    pub parent_id: u32,
}

/// Base address, size and name of a loaded module.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Module base address in the target's address space.
    pub base: usize,
    /// Module size in bytes (true `SizeOfImage` for Wine PE modules).
    pub size: usize,
    /// Module file name (e.g. `game.exe`).
    pub name: String,
}

/// Enumerates running processes.
pub fn processes() -> Result<Vec<ProcessInfo>> {
    Ok(ProcessIterator::new()?
        .map(|e| ProcessInfo {
            id: e.id,
            name: e.name,
            parent_id: e.parent_id,
        })
        .collect())
}

/// A loaded "managed" plugin: a shared library exporting the `yc_*` memory hooks.
struct ManagedExtension {
    // Kept alive so the loaded symbols stay valid; never accessed directly.
    #[allow(dead_code)]
    lib: libloading::Library,
    pid: u32,
    read: fn(usize, *mut u8, usize) -> u32,
    write: fn(usize, *const u8, usize) -> u32,
    can_read: fn(usize) -> bool,
    detach: fn(),
}

impl Drop for ManagedExtension {
    fn drop(&mut self) {
        (self.detach)();
    }
}

enum Backend {
    Native {
        proc: OwnedProcess,
        maps: Vec<MemoryRegion>,
    },
    Managed(ManagedExtension),
}

/// A handle to a target process's memory.
pub struct Target {
    backend: Backend,
    pointer_size: usize,
    is_wine: bool,
}

fn open_native(pid: u32) -> Result<OwnedProcess> {
    #[cfg(unix)]
    {
        Ok(nemclass_memory::external::find_process_by_id(pid)?)
    }
    #[cfg(windows)]
    {
        use nemclass_memory::types::win::{
            PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
        };
        Ok(nemclass_memory::external::open_process_by_id(
            pid,
            false,
            PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_QUERY_INFORMATION,
        )?)
    }
}

impl Target {
    /// Attaches natively to a process by id, using the OS memory APIs.
    pub fn attach_pid(pid: u32) -> Result<Self> {
        Self::from_native(open_native(pid)?)
    }

    /// Attaches natively to the first process whose image name matches `name`.
    pub fn attach_name(name: &str) -> Result<Self> {
        let info = processes()?
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| SdkError::Memory(nemclass_memory::MfError::ProcessNotFound))?;
        Self::attach_pid(info.id)
    }

    fn from_native(proc: OwnedProcess) -> Result<Self> {
        let maps = proc.maps()?;
        let pointer_size = proc.pointer_size();
        let is_wine = proc.is_wine();
        Ok(Self {
            backend: Backend::Native { proc, maps },
            pointer_size,
            is_wine,
        })
    }

    /// Attaches via a managed plugin library that provides the `yc_*` exports.
    ///
    /// # Safety
    /// Loads and calls arbitrary native code from `plugin`. The caller must
    /// trust the library.
    pub fn attach_managed(pid: u32, plugin: &Path) -> Result<Self> {
        let err = |e: libloading::Error| SdkError::Plugin(e.to_string());
        let sym_err = |name: &str| SdkError::Plugin(format!("missing export `{name}`"));

        unsafe {
            let lib = libloading::Library::new(plugin).map_err(err)?;
            let attach = *lib
                .get::<fn(u32) -> u32>(b"yc_attach")
                .map_err(|_| sym_err("yc_attach"))?;
            let read = *lib
                .get::<fn(usize, *mut u8, usize) -> u32>(b"yc_read")
                .map_err(|_| sym_err("yc_read"))?;
            let write = *lib
                .get::<fn(usize, *const u8, usize) -> u32>(b"yc_write")
                .map_err(|_| sym_err("yc_write"))?;
            let can_read = *lib
                .get::<fn(usize) -> bool>(b"yc_can_read")
                .map_err(|_| sym_err("yc_can_read"))?;
            let detach = *lib
                .get::<fn()>(b"yc_detach")
                .map_err(|_| sym_err("yc_detach"))?;

            let ext = ManagedExtension {
                lib,
                pid,
                read,
                write,
                can_read,
                detach,
            };
            (attach)(pid);

            Ok(Self {
                backend: Backend::Managed(ext),
                // A managed plugin abstracts the target's arch; assume 64-bit.
                pointer_size: size_of::<usize>(),
                is_wine: false,
            })
        }
    }

    /// The target's pointer width in bytes (4 for 32-bit / WoW64, 8 for 64-bit).
    pub fn pointer_size(&self) -> usize {
        self.pointer_size
    }

    /// Whether the target is a Wine process (always `false` for managed plugins).
    pub fn is_wine(&self) -> bool {
        self.is_wine
    }

    /// The target's process id.
    pub fn id(&self) -> u32 {
        match &self.backend {
            Backend::Native { proc, .. } => proc.id(),
            Backend::Managed(ext) => ext.pid,
        }
    }

    /// The target's process name (`"[managed]"` for plugin targets).
    pub fn name(&self) -> String {
        match &self.backend {
            Backend::Native { proc, .. } => proc.name().unwrap_or_default(),
            Backend::Managed(_) => "[managed]".into(),
        }
    }

    /// Fills `buf` with bytes read at `address`; returns `false` on failure (in
    /// which case `buf` may hold stale/partial data — do not trust it).
    #[must_use]
    pub fn read(&self, address: usize, buf: &mut [u8]) -> bool {
        match &self.backend {
            Backend::Native { proc, .. } => proc.read_buf(address, buf).is_ok(),
            Backend::Managed(ext) => (ext.read)(address, buf.as_mut_ptr(), buf.len()) == 0,
        }
    }

    /// Writes `buf` at `address`; returns `false` on failure.
    pub fn write(&self, address: usize, buf: &[u8]) -> bool {
        match &self.backend {
            Backend::Native { proc, .. } => proc.write_buf(address, buf).is_ok(),
            Backend::Managed(ext) => (ext.write)(address, buf.as_ptr(), buf.len()) == 0,
        }
    }

    /// Whether `address` is readable.
    pub fn can_read(&self, address: usize) -> bool {
        match &self.backend {
            Backend::Native { maps, .. } => maps
                .iter()
                .any(|m| m.from <= address && address < m.to && m.prot.read()),
            Backend::Managed(ext) => (ext.can_read)(address),
        }
    }

    /// Reads `len` bytes at `address`.
    pub fn read_bytes(&self, address: usize, len: usize) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; len];
        self.read(address, &mut buf).then_some(buf)
    }

    /// Reads a `Copy` value of type `T` at `address`.
    pub fn read_pod<T: Copy>(&self, address: usize) -> Option<T> {
        let mut v = MaybeUninit::<T>::uninit();
        // SAFETY: view the uninit storage as raw bytes to fill; only read back on success.
        let buf = unsafe { slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, size_of::<T>()) };
        self.read(address, buf)
            .then(|| unsafe { v.assume_init() })
    }

    /// Reads a pointer at `address`, honoring the target's pointer width
    /// (a 4-byte read is zero-extended for 32-bit / WoW64 targets).
    pub fn read_ptr(&self, address: usize) -> Option<usize> {
        let mut buf = [0u8; 8];
        let n = self.pointer_size.min(8);
        self.read(address, &mut buf[..n])
            .then(|| usize::from_ne_bytes(buf))
    }

    /// Reads a NUL-terminated UTF-8 string at `address` (lossy, capped at 4 KiB).
    pub fn read_string(&self, address: usize) -> Option<String> {
        let mut out = Vec::new();
        let mut addr = address;
        loop {
            let mut chunk = [0u8; 32];
            if !self.read(addr, &mut chunk) {
                break;
            }
            if let Some(pos) = chunk.iter().position(|&b| b == 0) {
                out.extend_from_slice(&chunk[..pos]);
                return Some(String::from_utf8_lossy(&out).into_owned());
            }
            out.extend_from_slice(&chunk);
            addr += 32;
            if out.len() >= 4096 {
                break;
            }
        }
        (!out.is_empty()).then(|| String::from_utf8_lossy(&out).into_owned())
    }

    /// Lists the target's loaded modules (native targets only).
    pub fn modules(&self) -> Result<Vec<ModuleInfo>> {
        match &self.backend {
            Backend::Native { proc, .. } => Ok(proc
                .modules()?
                .map(|m| ModuleInfo {
                    base: m.base as usize,
                    size: m.size,
                    name: m.name,
                })
                .collect()),
            Backend::Managed(_) => Err(SdkError::Plugin(
                "module enumeration is not available for managed targets".into(),
            )),
        }
    }

    /// Finds a module by name (case-insensitive).
    pub fn module(&self, name: &str) -> Result<ModuleInfo> {
        self.modules()?
            .into_iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| SdkError::ModuleNotFound(name.to_owned()))
    }

    /// Scans `[start, start+len)` for `pat`, returning every match address.
    pub fn scan_range<M: Matcher>(&self, pat: M, start: usize, len: usize) -> Vec<usize> {
        match &self.backend {
            Backend::Native { proc, .. } => proc.find_pattern(pat, start, len).collect(),
            Backend::Managed(_) => self.scan_managed(pat, start, len),
        }
    }

    fn scan_managed<M: Matcher>(&self, pat: M, start: usize, len: usize) -> Vec<usize> {
        let plen = pat.len();
        if plen == 0 || len < plen {
            return vec![];
        }
        let Some(buf) = self.read_bytes(start, len) else {
            return vec![];
        };
        (0..=len - plen)
            .filter(|&i| pat.matches(&buf[i..i + plen]))
            .map(|i| start + i)
            .collect()
    }

    /// Scans a named module for `pat`.
    pub fn scan_module<M: Matcher>(&self, pat: M, module: &str) -> Result<Vec<usize>> {
        let m = self.module(module)?;
        Ok(self.scan_range(pat, m.base, m.size))
    }
}
