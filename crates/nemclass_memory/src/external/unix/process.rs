use crate::{
    external::{MemoryRegion, ProcessEntry},
    types::{ModuleInfoWithName, Protection},
    Matcher, MfError,
};
use core::{
    mem::{size_of, MaybeUninit},
    slice::{from_raw_parts, from_raw_parts_mut},
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

/// Represents a single process in the system.
/// # Details
/// There is no such concept as 'owned' procses in unix. (i think).
/// The name is the same as on windows to reduce the hasle of cross-platform code.
#[derive(Debug)]
#[repr(transparent)]
pub struct OwnedProcess(pub(crate) u32);

impl OwnedProcess {
    /// Returns the id of the process.
    #[inline]
    pub fn id(&self) -> u32 {
        self.0
    }

    /// Returns full path to the process.
    pub fn path(&self) -> crate::Result<String> {
        Ok(fs::read_link(format!("/proc/{}/exe", self.0))
            .map_err(|_| MfError::ProcessDied)?
            .to_string_lossy()
            .into_owned())
    }

    /// Returns the name of the process
    pub fn name(&self) -> crate::Result<String> {
        Ok(fs::read_link(format!("/proc/{}/exe", self.0))
            .map_err(|_| MfError::ProcessDied)?
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default())
    }

    /// Reads process memory, returning amount of bytes read.
    pub fn read_buf(&self, address: usize, buf: &mut [u8]) -> crate::Result<usize> {
        unsafe {
            let read = libc::process_vm_readv(
                self.0 as _,
                &libc::iovec {
                    iov_base: buf.as_mut_ptr() as _,
                    iov_len: buf.len(),
                },
                1,
                &libc::iovec {
                    iov_base: address as _,
                    iov_len: buf.len(),
                },
                1,
                0,
            );

            if read == -1 {
                MfError::last()
            } else {
                Ok(read as usize)
            }
        }
    }

    /// Reads a value of type `T` at `address`.
    pub fn read<T>(&self, address: usize) -> crate::Result<T> {
        unsafe {
            let mut buf: MaybeUninit<T> = MaybeUninit::uninit();
            self.read_buf(
                address,
                from_raw_parts_mut(buf.as_mut_ptr().cast::<u8>() as _, size_of::<T>()),
            )?;
            Ok(buf.assume_init())
        }
    }

    /// Reads zero terminated string at `address`.
    pub fn read_str(&self, address: usize) -> crate::Result<String> {
        const STRIDE: usize = 4;

        let mut out = vec![];
        let mut offset = 0;

        loop {
            let buf = self.read::<[u8; STRIDE]>(address + offset)?;

            if let Some(i) = buf.iter().position(|b| *b == 0) {
                out.extend_from_slice(&buf[..i]);
                break;
            } else {
                out.extend_from_slice(&buf);
            }

            offset += STRIDE
        }

        String::from_utf8(out).map_err(|_| MfError::InvalidString)
    }

    /// Reads several disjoint regions of the process's memory in as few system
    /// calls as possible, returning the total number of bytes read.
    ///
    /// Each `(address, buffer)` pair names a remote address and the local buffer
    /// to fill from it. `process_vm_readv` is a scatter/gather primitive, so one
    /// syscall fetches every region at once — far cheaper than a `read_buf` per
    /// region when resolving many pointers or fields. The kernel caps a single
    /// call at `IOV_MAX` (1024) regions, so larger batches are split across
    /// calls transparently. As with [`read_buf`](Self::read_buf), the count may
    /// be short if a region is only partially readable.
    pub fn read_buf_batch(&self, regions: &mut [(usize, &mut [u8])]) -> crate::Result<usize> {
        // Linux caps one process_vm_readv at UIO_MAXIOV (IOV_MAX) iovecs.
        const MAX_IOV: usize = 1024;

        let mut total = 0;
        for chunk in regions.chunks_mut(MAX_IOV) {
            let local: Vec<libc::iovec> = chunk
                .iter_mut()
                .map(|(_, buf)| libc::iovec {
                    iov_base: buf.as_mut_ptr() as _,
                    iov_len: buf.len(),
                })
                .collect();
            let remote: Vec<libc::iovec> = chunk
                .iter()
                .map(|(address, buf)| libc::iovec {
                    iov_base: *address as _,
                    iov_len: buf.len(),
                })
                .collect();

            let read = unsafe {
                libc::process_vm_readv(
                    self.0 as _,
                    local.as_ptr(),
                    local.len() as _,
                    remote.as_ptr(),
                    remote.len() as _,
                    0,
                )
            };

            if read == -1 {
                return MfError::last();
            }
            total += read as usize;
        }

        Ok(total)
    }

    /// Reads a value of type `T` from each address in `addresses` with a single
    /// batched read, returning the values in the same order.
    ///
    /// Backed by [`read_buf_batch`](Self::read_buf_batch), so resolving N
    /// addresses costs one `process_vm_readv` instead of N. Fails if the batch
    /// could not be read in full, leaving no partially-initialized values.
    pub fn read_batch<T>(&self, addresses: &[usize]) -> crate::Result<Vec<T>> {
        let count = addresses.len();

        let mut out: Vec<MaybeUninit<T>> = Vec::with_capacity(count);
        // SAFETY: `MaybeUninit<T>` requires no initialization. Each slot is
        // filled by the read below before any value is read back out.
        unsafe { out.set_len(count) };

        let mut regions: Vec<(usize, &mut [u8])> = out
            .iter_mut()
            .zip(addresses)
            .map(|(slot, &address)| {
                // SAFETY: view the slot's storage as its raw bytes to read into.
                let bytes =
                    unsafe { from_raw_parts_mut(slot.as_mut_ptr().cast::<u8>(), size_of::<T>()) };
                (address, bytes)
            })
            .collect();

        if self.read_buf_batch(&mut regions)? != count * size_of::<T>() {
            return MfError::last();
        }

        // SAFETY: the read above populated every slot in full.
        Ok(out
            .into_iter()
            .map(|slot| unsafe { slot.assume_init() })
            .collect())
    }

    /// Writes process memory, returning amount of bytes written.
    pub fn write_buf(&self, address: usize, buf: &[u8]) -> crate::Result<usize> {
        unsafe {
            let written = libc::process_vm_writev(
                self.0 as _,
                &libc::iovec {
                    iov_base: buf.as_ptr() as _,
                    iov_len: buf.len(),
                },
                1,
                &libc::iovec {
                    iov_base: address as _,
                    iov_len: buf.len(),
                },
                1,
                0,
            );

            if written == -1 {
                MfError::last()
            } else {
                Ok(written as usize)
            }
        }
    }

    /// Writes `value` at `address` in the process's memory, returning amount of bytes written.
    pub fn write<T>(&self, address: usize, value: &T) -> crate::Result<usize> {
        unsafe {
            self.write_buf(
                address,
                from_raw_parts(value as *const T as *const u8, size_of::<T>()),
            )
        }
    }

    /// Returns an iterator over process's modules.
    pub fn modules(&self) -> crate::Result<impl Iterator<Item = ModuleInfoWithName>> {
        let s = fs::read_to_string(format!("/proc/{}/maps", self.0))
            .map_err(|_| MfError::ProcessDied)?;

        struct ModRange {
            from: usize,
            to: usize,
        }

        let mut maps: HashMap<String, ModRange> = HashMap::new();

        for l in s.lines() {
            let map = l
                .split_whitespace()
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>();
            if map.len() != 6 {
                continue;
            }

            let libname = map[5];
            let convert = |s: &str| usize::from_str_radix(s, 16).unwrap();

            let (from, to) = map[0]
                .split_once('-')
                .map(|(from, to)| (convert(from), convert(to)))
                .unwrap();

            if fs::metadata(libname).is_ok() {
                let ent = maps
                    .entry(libname.to_owned())
                    .or_insert_with(|| ModRange { from, to });

                // Wine splits one PE across many section mappings; grow the
                // range to the bounding box of every mapping of this file.
                ent.from = ent.from.min(from);
                ent.to = ent.to.max(to);
            }
        }

        let mut out = Vec::with_capacity(maps.len());
        for (k, ModRange { from, to }) in maps {
            let path = PathBuf::from(k);
            let name = match path.file_name() {
                Some(n) => n.to_string_lossy().into_owned(),
                None => continue,
            };

            // The span of section mappings undercounts a Wine PE (alignment
            // gaps, header-only tail pages), so prefer the true `SizeOfImage`
            // from the PE header mapped at the image base. Native objects have
            // no PE header there, so fall back to the measured span.
            let mut size = to - from;
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".exe") || lower.ends_with(".dll") {
                if let Some(image_size) = super::wine::size_of_image(self, from) {
                    if image_size != 0 {
                        size = image_size as usize;
                    }
                }
            }

            out.push(ModuleInfoWithName {
                name,
                base: from as *const u8,
                size,
            });
        }

        Ok(out.into_iter())
    }

    /// Searches for the specified module in the process.
    /// # Case
    /// Search is done case insensetive.
    pub fn find_module(&self, name: &str) -> crate::Result<ModuleInfoWithName> {
        self.modules()?
            .find(|m| m.name.eq_ignore_ascii_case(name))
            .ok_or(MfError::ModuleNotFound)
    }

    /// Finds all occurences of the pattern in a given range.
    // TODO: Can be optimized
    pub fn find_pattern<'a>(
        &'a self,
        pat: impl Matcher + 'a,
        start: usize,
        len: usize,
    ) -> impl Iterator<Item = usize> + 'a {
        let mut offset = 0;
        let mut buf = vec![0; pat.len()];

        std::iter::from_fn(move || {
            loop {
                if self.read_buf(start + offset, &mut buf[..]).is_err() {
                    return None;
                }

                if pat.matches(&buf[..]) {
                    break;
                }

                offset += 1;

                if offset >= len {
                    return None;
                }
            }

            offset += 1;
            Some(start + offset - 1)
        })
        .fuse()
    }

    /// Searches for a pattern in the specified module.
    pub fn find_pattern_in_module<'a>(
        &'a self,
        pat: impl Matcher + 'a,
        mod_name: &str,
    ) -> crate::Result<impl Iterator<Item = usize> + 'a> {
        let module = self.find_module(mod_name)?;

        Ok(self.find_pattern(pat, module.base as _, module.size))
    }

    /// Returns an iterator over mapped regions in the process.
    pub fn maps(&self) -> crate::Result<Vec<MemoryRegion>> {
        Ok(fs::read_to_string(format!("/proc/{}/maps", self.0))
            .map_err(|_| MfError::ProcessDied)?
            .lines()
            .map(|l| {
                let mut iter = l.split(' ');
                let (from, to) = iter.next().unwrap().split_once('-').unwrap();

                let from = usize::from_str_radix(from, 16).unwrap();
                let to = usize::from_str_radix(to, 16).unwrap();

                let prot = Protection::parse(&iter.next().unwrap()[0..3]);

                MemoryRegion { from, to, prot }
            })
            .collect())
    }

    /// Queryies protection for the specified address.
    /// `None` if no mappings were found for this address.
    pub fn query(&self, address: usize) -> crate::Result<Option<Protection>> {
        Ok(self
            .maps()?
            .iter()
            .find(|r| r.from <= address && r.to <= address)
            .map(|r| r.prot))
    }

    /// Resolves multilevel pointer
    pub fn resolve_multilevel(&self, mut base: usize, offsets: &[usize]) -> crate::Result<usize> {
        for (i, &o) in offsets.iter().enumerate() {
            if i != offsets.len() - 1 {
                base = self.read(base + o)?;
            } else {
                base += o;
            }
        }

        Ok(base)
    }
}

/// Iterator over all processes in the system.
pub struct ProcessIterator(Box<dyn Iterator<Item = ProcessEntry>>);

impl ProcessIterator {
    /// Creates new iterator over all processes in the system.
    /// # Unix
    /// Always returns Ok(I).
    pub fn new() -> crate::Result<Self> {
        fn get_parent_id(proc: &Path) -> u32 {
            let status = fs::read_to_string(proc.join("status")).unwrap();

            let parent_id = status
                .lines()
                .find_map(|l: &str| {
                    if l.starts_with("PPid:") {
                        l.split_once(':').map(|(_, tail)| tail.trim().to_owned())
                    } else {
                        None
                    }
                })
                .and_then(|p| p.parse::<u32>().ok())
                .unwrap();

            parent_id
        }

        let iter = fs::read_dir("/proc")
            .unwrap()
            .flatten()
            .filter_map(|de| Some((de.file_name().to_str()?.parse::<u32>().ok()?, de)))
            .filter_map(|(id, de)| {
                let entry = de.path();

                let path = fs::read_link(entry.join("exe")).ok()?;
                let mut name = path.file_name()?.to_str()?.to_owned();
                let parent_id = get_parent_id(&entry);

                // A Wine process's ELF image is just the loader (wine-preloader,
                // wine64-preloader, ...), so every Wine game lists under the same
                // useless name. Surface the actual Windows program it runs. Gate
                // on the loader name so we only scan maps for likely candidates.
                if name.to_ascii_lowercase().starts_with("wine") {
                    if let Some(exe) = super::wine::windows_exe_name(id) {
                        name = format!("{name} ({exe})");
                    }
                }

                Some(ProcessEntry {
                    id,
                    name,
                    parent_id,
                })
            });

        Ok(Self(Box::new(iter)))
    }
}

impl Iterator for ProcessIterator {
    type Item = ProcessEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Searches for the specified process by its name.
pub fn find_process_by_name(name: &str) -> crate::Result<OwnedProcess> {
    ProcessIterator::new()?
        .find_map(|pe| {
            if pe.name.eq_ignore_ascii_case(name) {
                Some(pe.open())
            } else {
                None
            }
        })
        .ok_or(MfError::ProcessNotFound)?
}

/// Searches for the specified process by its id.
pub fn find_process_by_id(id: u32) -> crate::Result<OwnedProcess> {
    if fs::metadata(format!("/proc/{id}")).is_err() {
        return Err(MfError::ProcessNotFound);
    }

    Ok(OwnedProcess(id))
}
