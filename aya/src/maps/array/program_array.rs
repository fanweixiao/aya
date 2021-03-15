//! An array of eBPF program file descriptors used as a jump table.

use std::{
    convert::TryFrom,
    mem,
    ops::{Deref, DerefMut},
    os::unix::prelude::RawFd,
};

use crate::{
    generated::bpf_map_type::BPF_MAP_TYPE_PROG_ARRAY,
    maps::{Map, MapError, MapKeys, MapRef, MapRefMut},
    programs::ProgramFd,
    sys::{bpf_map_delete_elem, bpf_map_update_elem},
};

/// An array of eBPF program file descriptors used as a jump table.
///
/// eBPF programs can jump to other programs calling `bpf_tail_call(prog_array, index)`. User space
/// programs can use [`ProgramArray`] to configure which programs correspond to which jump indexes.
pub struct ProgramArray<T: Deref<Target = Map>> {
    inner: T,
}

impl<T: Deref<Target = Map>> ProgramArray<T> {
    fn new(map: T) -> Result<ProgramArray<T>, MapError> {
        let map_type = map.obj.def.map_type;
        if map_type != BPF_MAP_TYPE_PROG_ARRAY as u32 {
            return Err(MapError::InvalidMapType {
                map_type: map_type as u32,
            })?;
        }
        let expected = mem::size_of::<RawFd>();
        let size = map.obj.def.key_size as usize;
        if size != expected {
            return Err(MapError::InvalidKeySize { size, expected });
        }

        let expected = mem::size_of::<RawFd>();
        let size = map.obj.def.value_size as usize;
        if size != expected {
            return Err(MapError::InvalidValueSize { size, expected });
        }
        let _fd = map.fd_or_err()?;

        Ok(ProgramArray { inner: map })
    }

    pub unsafe fn keys<'coll>(&'coll self) -> MapKeys<'coll, u32> {
        MapKeys::new(&self.inner)
    }

    fn check_bounds(&self, index: u32) -> Result<(), MapError> {
        let max_entries = self.inner.obj.def.max_entries;
        if index >= self.inner.obj.def.max_entries {
            Err(MapError::OutOfBounds { index, max_entries })
        } else {
            Ok(())
        }
    }
}

impl<T: Deref<Target = Map> + DerefMut<Target = Map>> ProgramArray<T> {
    /// Sets the target program file descriptor for the given index in the jump table.
    ///
    /// When an eBPF program calls `bpf_tail_call(prog_array, index)`, control
    /// flow will jump to `program`.
    ///
    /// # Example
    /// ```no_run
    /// # let bpf = aya::Bpf::load(&[], None)?;
    /// use aya::maps::ProgramArray;
    /// use aya::programs::KProbe;
    /// use std::convert::{TryFrom, TryInto};
    ///
    /// let mut prog_array = ProgramArray::try_from(bpf.map_mut("JUMP_TABLE")?)?;
    /// let prog: &KProbe = bpf.program("example_prog")?.try_into()?;
    /// prog_array.set(0, prog, 0 /* flags */);
    /// # Ok::<(), aya::BpfError>(())
    /// ```
    pub fn set(&mut self, index: u32, program: &dyn ProgramFd, flags: u64) -> Result<(), MapError> {
        let fd = self.inner.fd_or_err()?;
        self.check_bounds(index)?;
        let prog_fd = program.fd().ok_or(MapError::ProgramNotLoaded)?;

        bpf_map_update_elem(fd, &index, &prog_fd, flags).map_err(|(code, io_error)| {
            MapError::SyscallError {
                call: "bpf_map_update_elem".to_owned(),
                code,
                io_error,
            }
        })?;
        Ok(())
    }

    /// Clears the value at index in the jump table.
    ///
    /// Calling `bpf_tail_call(prog_array, index)` on an index that has been results in a failure.
    pub fn unset(&mut self, index: &u32) -> Result<(), MapError> {
        let fd = self.inner.fd_or_err()?;
        self.check_bounds(*index)?;
        bpf_map_delete_elem(fd, index)
            .map(|_| ())
            .map_err(|(code, io_error)| MapError::SyscallError {
                call: "bpf_map_delete_elem".to_owned(),
                code,
                io_error,
            })
    }
}

impl TryFrom<MapRef> for ProgramArray<MapRef> {
    type Error = MapError;

    fn try_from(a: MapRef) -> Result<ProgramArray<MapRef>, MapError> {
        ProgramArray::new(a)
    }
}

impl TryFrom<MapRefMut> for ProgramArray<MapRefMut> {
    type Error = MapError;

    fn try_from(a: MapRefMut) -> Result<ProgramArray<MapRefMut>, MapError> {
        ProgramArray::new(a)
    }
}
