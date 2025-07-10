use crate::mem::phys_to_virt;
use crate::time;
use kspin::SpinNoIrq;
use memory_addr::pa;

const HALT_ADDR: *mut u8 = phys_to_virt(pa!(axconfig::devices::GED_PADDR)).as_mut_ptr();

/// Shutdown the whole system, including all CPUs.
pub fn terminate() -> ! {
    info!("Shutting down...");
    unsafe { HALT_ADDR.write_volatile(0x34) };
    crate::arch::halt();
    warn!("It should shutdown!");
    loop {
        crate::arch::halt();
    }
}

static PARK_MILLER_LEHMER_SEED: SpinNoIrq<u32> = SpinNoIrq::new(0);
const RAND_MAX: u64 = 2_147_483_647;

pub fn random() -> u128 {
    let mut seed = PARK_MILLER_LEHMER_SEED.lock();
    if *seed == 0 {
        *seed = time::current_ticks() as u32;
    }

    let mut ret: u128 = 0;
    for _ in 0..4 {
        *seed = ((u64::from(*seed) * 48271) % RAND_MAX) as u32;
        ret = (ret << 32) | (*seed as u128);
    }
    ret
}
