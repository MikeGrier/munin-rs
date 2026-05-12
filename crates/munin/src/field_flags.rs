// Copyright (c) Michael Grier

//! Bitflags indicating which fields are present on a serialized `BuildEventArgs`.
//!
//! These flag values are part of the on-disk format. Changing any value is a
//! breaking change.

/// Bitmask specifying which fields are present on a serialized build event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BuildEventArgsFieldFlags(u32);

/// Individual flag constants. Changing any value is a breaking change.
#[allow(non_upper_case_globals)]
impl BuildEventArgsFieldFlags {
    pub const NONE: Self = Self(0x0000);
    pub const BUILD_EVENT_CONTEXT: Self = Self(0x0001);
    pub const HELP_KEYWORD: Self = Self(0x0002);
    pub const MESSAGE: Self = Self(0x0004);
    pub const SENDER_NAME: Self = Self(0x0008);
    pub const THREAD_ID: Self = Self(0x0010);
    pub const TIMESTAMP: Self = Self(0x0020);
    pub const SUBCATEGORY: Self = Self(0x0040);
    pub const CODE: Self = Self(0x0080);
    pub const FILE: Self = Self(0x0100);
    pub const PROJECT_FILE: Self = Self(0x0200);
    pub const LINE_NUMBER: Self = Self(0x0400);
    pub const COLUMN_NUMBER: Self = Self(0x0800);
    pub const END_LINE_NUMBER: Self = Self(0x1000);
    pub const END_COLUMN_NUMBER: Self = Self(0x2000);
    pub const ARGUMENTS: Self = Self(0x4000);
    pub const IMPORTANCE: Self = Self(0x8000);
    pub const EXTENDED: Self = Self(0x1_0000);
}

impl BuildEventArgsFieldFlags {
    /// Create from a raw `u32` read from the stream.
    pub fn from_raw(bits: u32) -> Self {
        Self(bits)
    }

    /// Return the raw `u32` value.
    pub fn bits(self) -> u32 {
        self.0
    }

    /// Check whether a specific flag is set.
    pub fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

impl std::ops::BitOr for BuildEventArgsFieldFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitAnd for BuildEventArgsFieldFlags {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

#[cfg(test)]
mod tests;
