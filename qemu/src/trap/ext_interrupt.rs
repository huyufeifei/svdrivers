use crate::arch::hart_id;
use crate::mutex::Mutex;
use crate::println;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use plic::{Mode, PLIC};
use spin::Once;

pub static PLIC: Once<PLIC<1>> = Once::new();
pub static DEVICE_TABLE: Mutex<BTreeMap<usize, Arc<Mutex<dyn DeviceBase>>>> =
    Mutex::new(BTreeMap::new());

pub fn init_plic(plic_addr: usize) {
    let privileges = [2; 1];
    let plic = PLIC::new(plic_addr, privileges);
    PLIC.call_once(|| plic);
    println!("Init qemu plic success");
}

/// Register a device to PLIC.
pub fn register_device_to_plic(irq: usize, device: Arc<Mutex<dyn DeviceBase>>) {
    let hard_id = hart_id();
    let mut table = DEVICE_TABLE.lock();
    table.insert(irq, device);
    println!(
        "PLIC enable irq {} for hart {}, priority {}",
        irq, hard_id, 1
    );
    let plic = PLIC.get().unwrap();
    plic.set_threshold(hard_id as u32, Mode::Machine, 1);
    plic.set_threshold(hard_id as u32, Mode::Supervisor, 0);
    plic.complete(hard_id as u32, Mode::Supervisor, irq as u32);
    plic.set_priority(irq as u32, 1);
    plic.enable(hard_id as u32, Mode::Supervisor, irq as u32);
}

pub fn external_interrupt_handler() {
    let plic = PLIC.get().unwrap();
    let hart_id = hart_id();
    let irq = plic.claim(hart_id as u32, Mode::Supervisor);
    info!("external_interrupt_handler: irq: {}", irq);
    let table = DEVICE_TABLE.lock();
    let device = table
        .get(&(irq as usize))
        .or_else(|| panic!("no device for irq {}", irq))
        .unwrap();
    info!("find device for irq {}", irq);
    device.lock().handle_irq();
    plic.complete(hart_id as u32, Mode::Supervisor, irq);
}

pub trait DeviceBase: Send + Sync {
    fn handle_irq(&mut self);
}
