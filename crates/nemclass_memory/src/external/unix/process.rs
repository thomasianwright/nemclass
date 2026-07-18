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
    path::Path,
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

        let mut out = Vec::new();
        for RawModule { name, base, end } in parse_maps_modules(&s) {
            // The span of section mappings undercounts a Wine PE (alignment
            // gaps, header-only tail pages), so prefer the true `SizeOfImage`
            // read from the PE header mapped at the image base. Native objects
            // have no PE header there, so fall back to the measured span.
            let mut size = end.saturating_sub(base);
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".exe") || lower.ends_with(".dll") {
                if let Some(image_size) = super::wine::size_of_image(self, base) {
                    if image_size != 0 {
                        size = image_size as usize;
                    }
                }
            }

            out.push(ModuleInfoWithName {
                name,
                base: base as *const u8,
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

    /// Returns the Windows program name (e.g. `Terraria.exe`) if this process is
    /// running under Wine, or `None` for a native Linux process. See
    /// [`wine`](super::wine) for how Wine processes are recognised.
    pub fn windows_program_name(&self) -> Option<String> {
        super::wine::windows_exe_name(self.0)
    }

    /// Returns `true` if this process is running under Wine.
    pub fn is_wine(&self) -> bool {
        self.windows_program_name().is_some()
    }

    /// Returns the target's pointer width in bytes: `4` for a 32-bit / WoW64
    /// target and `8` for a 64-bit one.
    ///
    /// For Wine processes this is read from the main PE image's optional-header
    /// magic; native Linux processes are assumed to match the host width.
    pub fn pointer_size(&self) -> usize {
        if let Some(exe) = self.windows_program_name() {
            if let Ok(m) = self.find_module(&exe) {
                if let Some(size) = super::wine::pointer_size(self, m.base as usize) {
                    return size;
                }
            }
        }
        core::mem::size_of::<usize>()
    }

    /// Finds all occurrences of the pattern in `[start, start + len)`.
    ///
    /// The target's memory is pulled across in ~1 MiB chunks (one
    /// `process_vm_readv` per chunk) and matched locally. The naive alternative —
    /// one syscall per byte offset — turns a multi-MB module scan into tens of
    /// millions of syscalls (a ~70 MiB Wine module took ~30s that way). Only the
    /// readable sub-ranges are scanned, since Wine splits a module across
    /// mappings with unreadable gaps that would fail a spanning read; consecutive
    /// chunks overlap by `pat.len() - 1` so a match straddling a chunk boundary
    /// is still found.
    pub fn find_pattern<'a>(
        &'a self,
        pat: impl Matcher + 'a,
        start: usize,
        len: usize,
    ) -> impl Iterator<Item = usize> + 'a {
        const CHUNK: usize = 1 << 20; // 1 MiB per read.

        let mut out = Vec::new();
        let plen = pat.len();
        if plen == 0 || len < plen {
            return out.into_iter();
        }

        let end = start.saturating_add(len);
        let overlap = plen - 1;
        let mut buf = vec![0u8; CHUNK + overlap];

        for (region_start, region_end) in self.readable_regions(start, end) {
            let mut addr = region_start;
            while addr + plen <= region_end {
                // Read the chunk plus the overlap that lets the next window's
                // opening match be completed here.
                let want = (region_end - addr).min(CHUNK + overlap);
                let n = self.read_buf(addr, &mut buf[..want]).unwrap_or(0);

                if n >= plen {
                    for i in 0..=n - plen {
                        if pat.matches(&buf[i..i + plen]) {
                            out.push(addr + i);
                        }
                    }
                }

                // A short read (unexpected hole) or the region's tail ends it;
                // otherwise step by CHUNK, keeping `overlap` via the next read.
                if n < want || want < CHUNK + overlap {
                    break;
                }
                addr += CHUNK;
            }
        }

        out.into_iter()
    }

    /// The maximal readable sub-ranges of `[start, end)`, in ascending order.
    ///
    /// Wine maps one module across many `/proc/<pid>/maps` entries, some
    /// unreadable (guard / `PAGE_NOACCESS`); a single read spanning such a hole
    /// fails, so a scan must read each readable run separately.
    fn readable_regions(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        let mut regions: Vec<(usize, usize)> = Vec::new();
        if start >= end {
            return regions;
        }
        let Ok(maps) = fs::read_to_string(format!("/proc/{}/maps", self.0)) else {
            return regions;
        };
        for line in maps.lines() {
            let mut fields = line.split_whitespace();
            let Some(range) = fields.next() else { continue };
            if !fields.next().unwrap_or("").starts_with('r') {
                continue; // not readable
            }
            let Some((a, b)) = range.split_once('-') else {
                continue;
            };
            let (Ok(seg_start), Ok(seg_end)) = (
                usize::from_str_radix(a, 16),
                usize::from_str_radix(b, 16),
            ) else {
                continue;
            };
            let clipped_start = seg_start.max(start);
            let clipped_end = seg_end.min(end);
            if clipped_start >= clipped_end {
                continue;
            }
            // maps are address-sorted; merge runs that touch or overlap.
            match regions.last_mut() {
                Some(last) if clipped_start <= last.1 => last.1 = last.1.max(clipped_end),
                _ => regions.push((clipped_start, clipped_end)),
            }
        }
        regions
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

/// One module aggregated from `/proc/<pid>/maps`: its file name, image base and
/// the end of its last mapping.
struct RawModule {
    name: String,
    base: usize,
    end: usize,
}

/// Parses `/proc/<pid>/maps` text into one span per mapped file.
///
/// Robust to the two things that break naive whitespace splitting under Wine:
/// - **paths with spaces** (`drive_c/Program Files/…`) and the **`(deleted)`**
///   suffix — the pathname is taken as everything from the first `/` (the
///   address/perms/offset/dev/inode columns never contain one);
/// - **fragmented PE images** — Wine maps one PE as many section mappings, which
///   are merged here by full path (so 32- and 64-bit copies under WoW64 stay
///   distinct). The reported base is the mapping at file offset 0 (the PE
///   headers / true image base), falling back to the lowest mapping. Modules are
///   returned in first-seen order.
fn parse_maps_modules(maps: &str) -> Vec<RawModule> {
    struct Acc {
        name: String,
        image_base: Option<usize>,
        lowest: usize,
        end: usize,
    }

    let mut order: Vec<String> = Vec::new();
    let mut acc: HashMap<String, Acc> = HashMap::new();

    for line in maps.lines() {
        // File-backed mappings are exactly the lines with a pathname, which
        // starts at the first '/'. Anonymous / [heap] / [stack] have none.
        let Some(slash) = line.find('/') else {
            continue;
        };
        let path = line[slash..].trim_end();
        let path = path
            .strip_suffix("(deleted)")
            .map(str::trim_end)
            .unwrap_or(path);
        let Some(name) = path.rsplit('/').next().filter(|n| !n.is_empty()) else {
            continue;
        };

        // The columns before the pathname: address perms offset dev inode.
        let mut fields = line[..slash].split_whitespace();
        let Some((from, to)) = fields.next().and_then(|r| r.split_once('-')) else {
            continue;
        };
        let (Ok(start), Ok(end)) =
            (usize::from_str_radix(from, 16), usize::from_str_radix(to, 16))
        else {
            continue;
        };
        let _perms = fields.next();
        let offset = fields
            .next()
            .and_then(|s| usize::from_str_radix(s, 16).ok())
            .unwrap_or(0);

        let entry = acc.entry(path.to_owned()).or_insert_with(|| {
            order.push(path.to_owned());
            Acc {
                name: name.to_owned(),
                image_base: None,
                lowest: start,
                end,
            }
        });
        // The offset-0 mapping holds the MZ/NT headers and sits at the true image
        // base; `start - offset` is unreliable when section alignments differ.
        if offset == 0 && entry.image_base.is_none() {
            entry.image_base = Some(start);
        }
        entry.lowest = entry.lowest.min(start);
        entry.end = entry.end.max(end);
    }

    order
        .into_iter()
        .filter_map(|path| acc.remove(&path))
        .map(|a| RawModule {
            name: a.name,
            base: a.image_base.unwrap_or(a.lowest),
            end: a.end,
        })
        .collect()
}

#[cfg(test)]
mod maps_tests {
    use super::parse_maps_modules;

    #[test]
    fn wine_pe_with_spaces_in_path_is_recognised() {
        // A Wine PE under a path with a space, split across section mappings.
        let maps = "\
7f0000000000-7f0000001000 r--p 00000000 08:01 111 /home/u/.wine/drive_c/Program Files/Game/test.dll
7f0000001000-7f0000010000 r-xp 00001000 08:01 111 /home/u/.wine/drive_c/Program Files/Game/test.dll
7f0000010000-7f0000012000 rw-p 00010000 08:01 111 /home/u/.wine/drive_c/Program Files/Game/test.dll
";
        let mods = parse_maps_modules(maps);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "test.dll");
        assert_eq!(mods[0].base, 0x7f0000000000);
        assert_eq!(mods[0].end, 0x7f0000012000);
    }

    #[test]
    fn base_prefers_offset_zero_mapping_over_lowest() {
        // The offset-0 (header) mapping is higher than another section here;
        // the base must still be the offset-0 mapping, not the lowest address.
        let maps = "\
0000000000400000-0000000000401000 r-xp 00002000 08:01 222 /x/foo.dll
0000000000410000-0000000000411000 r--p 00000000 08:01 222 /x/foo.dll
";
        let mods = parse_maps_modules(maps);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].base, 0x410000);
    }

    #[test]
    fn deleted_suffix_and_non_file_lines() {
        let maps = "\
0000555500000000-0000555500001000 r--p 00000000 08:01 333 /tmp/bar.dll (deleted)
0000555500001000-0000555500002000 r-xp 00001000 08:01 333 /tmp/bar.dll (deleted)
7ffff7a00000-7ffff7a21000 rw-p 00000000 00:00 0
7ffff7ffd000-7ffff7fff000 r--p 00000000 00:00 0 [vvar]
";
        let mods = parse_maps_modules(maps);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "bar.dll");
    }

    #[test]
    fn wow64_same_basename_distinct_files_stay_separate() {
        let maps = "\
00000000f0000000-00000000f0001000 r--p 00000000 08:01 10 /wine/lib32/test.dll
00000000f0001000-00000000f0010000 r-xp 00001000 08:01 10 /wine/lib32/test.dll
7f0000000000-7f0000001000 r--p 00000000 08:01 20 /wine/lib64/test.dll
7f0000001000-7f0000010000 r-xp 00001000 08:01 20 /wine/lib64/test.dll
";
        let mods = parse_maps_modules(maps);
        assert_eq!(mods.len(), 2);
        assert!(mods.iter().all(|m| m.name == "test.dll"));
    }
}
