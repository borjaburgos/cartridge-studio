.syntax unified
.cpu cortex-m0
.thumb
.section .entry,"ax",%progbits
.global helper_entry
.type helper_entry,%function
.thumb_func
helper_entry:
    /* Thumb ADR r0, #380: aligned PC is entry+4, mailbox is entry+384. */
    .inst.n 0xa05f
    b program_batch
