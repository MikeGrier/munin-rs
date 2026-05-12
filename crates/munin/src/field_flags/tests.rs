// Copyright (c) Michael Grier

use super::BuildEventArgsFieldFlags;

#[test]
fn none_is_zero() {
    assert_eq!(BuildEventArgsFieldFlags::NONE.bits(), 0);
}

#[test]
fn individual_flag_values() {
    assert_eq!(BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT.bits(), 0x0001);
    assert_eq!(BuildEventArgsFieldFlags::HELP_KEYWORD.bits(), 0x0002);
    assert_eq!(BuildEventArgsFieldFlags::MESSAGE.bits(), 0x0004);
    assert_eq!(BuildEventArgsFieldFlags::SENDER_NAME.bits(), 0x0008);
    assert_eq!(BuildEventArgsFieldFlags::THREAD_ID.bits(), 0x0010);
    assert_eq!(BuildEventArgsFieldFlags::TIMESTAMP.bits(), 0x0020);
    assert_eq!(BuildEventArgsFieldFlags::SUBCATEGORY.bits(), 0x0040);
    assert_eq!(BuildEventArgsFieldFlags::CODE.bits(), 0x0080);
    assert_eq!(BuildEventArgsFieldFlags::FILE.bits(), 0x0100);
    assert_eq!(BuildEventArgsFieldFlags::PROJECT_FILE.bits(), 0x0200);
    assert_eq!(BuildEventArgsFieldFlags::LINE_NUMBER.bits(), 0x0400);
    assert_eq!(BuildEventArgsFieldFlags::COLUMN_NUMBER.bits(), 0x0800);
    assert_eq!(BuildEventArgsFieldFlags::END_LINE_NUMBER.bits(), 0x1000);
    assert_eq!(BuildEventArgsFieldFlags::END_COLUMN_NUMBER.bits(), 0x2000);
    assert_eq!(BuildEventArgsFieldFlags::ARGUMENTS.bits(), 0x4000);
    assert_eq!(BuildEventArgsFieldFlags::IMPORTANCE.bits(), 0x8000);
    assert_eq!(BuildEventArgsFieldFlags::EXTENDED.bits(), 0x1_0000);
}

#[test]
fn no_flag_overlaps() {
    let all_flags = [
        BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT,
        BuildEventArgsFieldFlags::HELP_KEYWORD,
        BuildEventArgsFieldFlags::MESSAGE,
        BuildEventArgsFieldFlags::SENDER_NAME,
        BuildEventArgsFieldFlags::THREAD_ID,
        BuildEventArgsFieldFlags::TIMESTAMP,
        BuildEventArgsFieldFlags::SUBCATEGORY,
        BuildEventArgsFieldFlags::CODE,
        BuildEventArgsFieldFlags::FILE,
        BuildEventArgsFieldFlags::PROJECT_FILE,
        BuildEventArgsFieldFlags::LINE_NUMBER,
        BuildEventArgsFieldFlags::COLUMN_NUMBER,
        BuildEventArgsFieldFlags::END_LINE_NUMBER,
        BuildEventArgsFieldFlags::END_COLUMN_NUMBER,
        BuildEventArgsFieldFlags::ARGUMENTS,
        BuildEventArgsFieldFlags::IMPORTANCE,
        BuildEventArgsFieldFlags::EXTENDED,
    ];
    for (i, a) in all_flags.iter().enumerate() {
        for (j, b) in all_flags.iter().enumerate() {
            if i != j {
                assert_eq!((*a & *b).bits(), 0, "flags at index {i} and {j} overlap");
            }
        }
    }
}

#[test]
fn contains_check() {
    let combined = BuildEventArgsFieldFlags::MESSAGE | BuildEventArgsFieldFlags::TIMESTAMP;
    assert!(combined.contains(BuildEventArgsFieldFlags::MESSAGE));
    assert!(combined.contains(BuildEventArgsFieldFlags::TIMESTAMP));
    assert!(!combined.contains(BuildEventArgsFieldFlags::CODE));
    // NONE (0) is vacuously contained in every value.
    assert!(combined.contains(BuildEventArgsFieldFlags::NONE));
}

#[test]
fn from_raw_preserves_bits() {
    let flags = BuildEventArgsFieldFlags::from_raw(0x0205);
    assert!(flags.contains(BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT));
    assert!(flags.contains(BuildEventArgsFieldFlags::MESSAGE));
    assert!(flags.contains(BuildEventArgsFieldFlags::PROJECT_FILE));
    assert!(!flags.contains(BuildEventArgsFieldFlags::CODE));
}

#[test]
fn bitor_combines_flags() {
    let a = BuildEventArgsFieldFlags::FILE;
    let b = BuildEventArgsFieldFlags::LINE_NUMBER;
    let c = a | b;
    assert_eq!(c.bits(), 0x0100 | 0x0400);
}

#[test]
fn bitand_intersects_flags() {
    let a = BuildEventArgsFieldFlags::from_raw(0x0007);
    let b = BuildEventArgsFieldFlags::MESSAGE;
    let c = a & b;
    assert_eq!(c.bits(), 0x0004);
}

#[test]
fn all_17_flags_are_distinct_powers_of_two() {
    let all_flags = [
        BuildEventArgsFieldFlags::BUILD_EVENT_CONTEXT,
        BuildEventArgsFieldFlags::HELP_KEYWORD,
        BuildEventArgsFieldFlags::MESSAGE,
        BuildEventArgsFieldFlags::SENDER_NAME,
        BuildEventArgsFieldFlags::THREAD_ID,
        BuildEventArgsFieldFlags::TIMESTAMP,
        BuildEventArgsFieldFlags::SUBCATEGORY,
        BuildEventArgsFieldFlags::CODE,
        BuildEventArgsFieldFlags::FILE,
        BuildEventArgsFieldFlags::PROJECT_FILE,
        BuildEventArgsFieldFlags::LINE_NUMBER,
        BuildEventArgsFieldFlags::COLUMN_NUMBER,
        BuildEventArgsFieldFlags::END_LINE_NUMBER,
        BuildEventArgsFieldFlags::END_COLUMN_NUMBER,
        BuildEventArgsFieldFlags::ARGUMENTS,
        BuildEventArgsFieldFlags::IMPORTANCE,
        BuildEventArgsFieldFlags::EXTENDED,
    ];
    assert_eq!(all_flags.len(), 17);
    for flag in &all_flags {
        let b = flag.bits();
        assert!(
            b.is_power_of_two(),
            "flag value {b:#x} is not a power of two"
        );
    }
}

#[test]
fn extended_flag_is_bit_16() {
    // The Extended flag is bit 16 (0x10000) — it does not fit in a u16.
    assert_eq!(BuildEventArgsFieldFlags::EXTENDED.bits(), 1 << 16);
}
