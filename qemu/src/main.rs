#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(asm_const)]
#![allow(unused)]
#[macro_use]
extern crate log;

extern crate alloc;

use crate::my_impl::{MyHalImpl, SafeIoRegion};
use crate::sbi::system_shutdown;
use crate::trap::ext_interrupt;
use alloc::boxed::Box;
use alloc::vec;
use core::ptr::NonNull;
use core::sync::atomic::AtomicUsize;
use fdt::{node::FdtNode, standard_nodes::Compatible, Fdt};
use log::LevelFilter;
use spin::Lazy;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use virtio_drivers::transport::{DeviceType, Transport};
use virtio_impl::HalImpl;

mod virtio_impl;

mod boot;
mod my_impl;
mod sbi;
#[cfg(feature = "tcp")]
mod tcp;
mod timer;
mod trap;
#[macro_use]
mod console;
mod arch;
mod logging;
mod mutex;
mod new_test;
mod old_impl;
mod old_test;

extern "C" {
    fn end();
}

static DMA_PADDR: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(end as usize));

pub const NET_QUEUE_SIZE: usize = 16;
pub const NET_BUFFER_LEN: usize = 2048;
#[no_mangle]
extern "C" fn main(_hartid: usize, device_tree_paddr: usize) {
    logging::init_logger();
    // initialize PLIC
    ext_interrupt::init_plic(0xc000000);
    new_test::init_dt(device_tree_paddr);
    // old_test::init_dt(device_tree_paddr);
    trap::init_trap_subsystem();
    new_test::test_all_devices();
    // old_test::test_all_devices();
    info!("test end");
    system_shutdown();
}
