/* Temporary Cortex-M0 routine for SST39SF040 /WE on GB AUDIO, MBC5.
 * Runs only inside 384 bytes of explicitly allocated INLretro transfer RAM.
 * Never writes MCU flash, clock configuration, stack pointers, or interrupts.
 * Pin mapping: manufacturer's STM_INL6 pinport_al.h (PCB v2.1).
 * SST39SF040 commands/timing: Microchip DS20005022, tables 4-2 and 7-2.
 */
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
#define REG(a) (*(volatile u32 *)(a))
#define A_BSRR REG(0x48000018)
#define A_BRR REG(0x48000028)
#define B_MODE REG(0x48000400)
#define B_DATA REG(0x48000414)
#define C_ADDR REG(0x48000814)

/* The assembly entry obtains this address with PC-relative ADR. */
struct mailbox {
    u32 magic;
    u16 offset;
    u8 bank;
    u8 count;
    u32 result;
    u8 data[116];
};

__attribute__((noinline)) static void cycle(u32 address, u32 data, u32 pin)
{
    C_ADDR = address;
    B_DATA = (B_DATA & 255) | (data << 8);
    A_BRR = pin;
    __asm__ volatile("nop; nop; nop; nop; nop; nop; nop; nop");
    A_BSRR = pin;
}

void program_batch(volatile struct mailbox *m)
{
    m->result = 0;
    if (m->count > 116 || m->bank > 31 || m->offset + m->count > 0x4000)
        return;
    /* An echo operation proves code and mailbox placement without GPIO writes. */
    if (m->magic == 0x4543484f) {
        u32 sum = 0;
        for (u32 i = 0; i < m->count; ++i) sum += m->data[i];
        m->result = 0xec000000 | sum;
        return;
    }
    if (m->magic != 0x53465354) return;
    A_BSRR = 0x2d; /* Flash /WE, mapper /WR, ROM /RD and SRAM /CS idle. */
    B_MODE = (B_MODE & 0xffff) | 0x55550000;
    for (u32 i = 0; i < m->count; ++i) {
        u32 data = m->data[i];
        if (data == 255) continue;
        cycle(0x2000, 1, 8); /* Unlock addresses require physical ROM bank 1. */
        cycle(0x5555, 0xaa, 32);
        cycle(0x2aaa, 0x55, 32);
        cycle(0x5555, 0xa0, 32);
        if (m->bank > 1) cycle(0x2000, m->bank, 8);
        cycle((m->bank ? 0x4000 : 0) + m->offset + i, data, 32);
        /* >20 us at 48 MHz; volatile loop also prevents removal/unrolling. */
        for (volatile u32 delay = 512; delay; --delay) __asm__ volatile("nop");
    }
    B_MODE &= 0xffff; /* Release data bus before returning to firmware. */
    m->result = 0x600d0000 | m->count;
}
