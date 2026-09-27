ENTRY(helper_entry)
PROVIDE(HELPER_BASE = 0);
SECTIONS {
    .entry HELPER_BASE : { *(.entry) }
    .text HELPER_BASE + 4 : { *(.text*) *(.rodata*) }
    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment*) *(.ARM.attributes*) }
    ASSERT(SIZEOF(.text) <= 380, "Flash helper exceeds allocated transfer RAM")
}
