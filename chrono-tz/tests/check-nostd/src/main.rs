#![no_std]
#![no_main]

use core::panic::PanicInfo;

use cortex_m_rt::entry;
use sad_chrono_tz::US::Pacific;

#[panic_handler]
fn catch_panic(_e: &PanicInfo) -> ! {
    loop {}
}

#[entry]
fn main() -> ! {
    fn assert_core_error<T: core::error::Error>() {}
    assert_core_error::<sad_chrono_tz::ParseError>();

    assert!(Pacific == Pacific);

    loop {}
}
