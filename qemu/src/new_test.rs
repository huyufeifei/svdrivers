use crate::mutex::Mutex;
use crate::my_impl::{MyHalImpl, SafeIoRegion};
use crate::trap::ext_interrupt::{register_device_to_plic, DeviceBase};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, Ordering};
use fdt::node::FdtNode;
use fdt::standard_nodes::Compatible;
use fdt::Fdt;
use safe_virtio_drivers::device::block::VirtIOBlk;
use safe_virtio_drivers::device::console::VirtIOConsole;
use safe_virtio_drivers::device::gpu::VirtIOGpu;
use safe_virtio_drivers::device::input::VirtIOInput;
use safe_virtio_drivers::device::net::{VirtIONet, VirtIONetRaw};
use safe_virtio_drivers::error::{MmioError, VirtIoError};
use safe_virtio_drivers::transport::mmio::MmioTransport;
use safe_virtio_drivers::transport::{DeviceType, Transport};
use spin::Once;

static BLK: Once<Arc<Mutex<VirtIOBlk<MyHalImpl, MmioTransport>>>> = Once::new();
static CONSOLE: Once<Arc<Mutex<VirtIOConsole<MyHalImpl, MmioTransport>>>> = Once::new();
static GPU: Once<Arc<Mutex<VirtIOGpu<MyHalImpl, MmioTransport>>>> = Once::new();
static INPUT: Once<Arc<Mutex<VirtIOInput<MyHalImpl, MmioTransport>>>> = Once::new();
static NET: Once<Arc<Mutex<VirtIONet<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }>>>> =
    Once::new();
static NET_RAW: Once<
    Arc<Mutex<VirtIONetRaw<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }>>>,
> = Once::new();

pub fn init_dt(dtb: usize) {
    info!("device tree @ {:#x}", dtb);
    // Safe because the pointer is a valid pointer to unaliased memory.
    let fdt = unsafe { Fdt::from_ptr(dtb as *const u8).unwrap() };
    walk_dt(fdt);
}

pub fn test_all_devices() {
    virtio_blk();
    virtio_gpu();
    virtio_input();
    virtio_console();
    virtio_net();
}

fn walk_dt(fdt: Fdt) {
    for node in fdt.all_nodes() {
        if let Some(compatible) = node.compatible() {
            if compatible.all().any(|s| s == "virtio,mmio") {
                virtio_probe(node);
            }
        }
    }
}

fn virtio_probe(node: FdtNode) {
    if let Some(reg) = node.reg().and_then(|mut reg| reg.next()) {
        let paddr = reg.starting_address as usize;
        let size = reg.size.unwrap();
        let vaddr = paddr;
        info!("walk dt addr={:#x}, size={:#x}", paddr, size);
        info!(
            "Device tree node {}: {:?}",
            node.name,
            node.compatible().map(Compatible::first),
        );
        let io_region = SafeIoRegion::new(vaddr, size);
        let transport = MmioTransport::new(Box::new(io_region));
        let transport = match transport {
            Err(VirtIoError::MmioError(MmioError::ZeroDeviceId)) => {
                warn!("Invalid magic value");
                return;
            }
            Ok(t) => t,
            _ => {
                warn!("Error creating VirtIO MMIO transport");
                return;
            }
        };
        let irq = node.interrupts().unwrap().next().unwrap();
        warn!("Detected virtio MMIO device with irq {}", irq);
        warn!(
            "Detected virtio MMIO device with vendor id {:#X}, device type {:?}, version {:?}",
            transport.vendor_id(),
            transport.device_type(),
            transport.version(),
        );
        virtio_device(transport, irq);
    }
}

fn virtio_device(transport: MmioTransport, irq: usize) {
    match transport.device_type().unwrap() {
        DeviceType::Block => {
            let mut blk = VirtIOBlk::<MyHalImpl, MmioTransport>::new(transport)
                .expect("failed to create blk driver");
            let blk = Arc::new(Mutex::new(blk));
            // register_device_to_plic(irq,blk.clone());
            BLK.call_once(|| blk);
        }
        DeviceType::Input => {
            let mut input = VirtIOInput::<MyHalImpl, MmioTransport>::new(transport)
                .expect("input driver create failed");
            let input = Arc::new(Mutex::new(input));
            // register_device_to_plic(irq,input.clone());
            INPUT.call_once(|| input);
        }
        DeviceType::Console => {
            let mut console = VirtIOConsole::<MyHalImpl, MmioTransport>::new(transport)
                .expect("failed to create console driver");
            let console = Arc::new(Mutex::new(console));
            // register_device_to_plic(irq,console.clone());
            CONSOLE.call_once(|| console);
        }
        DeviceType::GPU => {
            let mut gpu = VirtIOGpu::<MyHalImpl, MmioTransport>::new(transport)
                .expect("failed to create gpu driver");
            let gpu = Arc::new(Mutex::new(gpu));
            // register_device_to_plic(irq,gpu.clone());
            GPU.call_once(|| gpu);
        }
        DeviceType::Network => {
            #[cfg(not(feature = "tcp"))]
            {
                let mut net =
                    VirtIONetRaw::<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }>::new(
                        transport,
                    )
                    .expect("failed to create net driver");
                let net = Arc::new(Mutex::new(net));
                register_device_to_plic(irq, net.clone());
                NET_RAW.call_once(|| net);
            }
            #[cfg(feature = "tcp")]
            {
                let net = VirtIONet::<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }>::new(
                    transport,
                    crate::NET_BUFFER_LEN,
                )
                .expect("failed to create net driver");
                let net = Arc::new(Mutex::new(net));
                register_device_to_plic(irq, net.clone());
                NET.call_once(|| net);
            }
        }
        t => warn!("Unrecognized virtio device: {:?}", t),
    }
}
fn virtio_blk() {
    let mut blk = BLK.get().unwrap().lock();
    let mut input = vec![0xffu8; 512];
    let mut output = vec![0; 512];
    let iter = 10 * 1024 * 1024 / 512;
    for i in 0..iter {
        for x in input.iter_mut() {
            *x = i as u8;
        }
        blk.write_blocks(i, &input).expect("failed to write");
        blk.read_blocks(i, &mut output).expect("failed to read");
        assert_eq!(input, output);
    }
    blk.flush().expect("failed to flush");
    info!("virtio-blk test finished");
}

fn virtio_gpu() {
    let mut gpu = GPU.get().unwrap().lock();
    let (width, height) = gpu.resolution().expect("failed to get resolution");
    let width = width as usize;
    let height = height as usize;
    info!("GPU resolution is {}x{}", width, height);
    let fb = gpu.setup_framebuffer().expect("failed to get fb");
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            fb[idx] = x as u8;
            fb[idx + 1] = y as u8;
            fb[idx + 2] = (x + y) as u8;
        }
    }
    gpu.flush().expect("failed to flush");
    // delay some time
    info!("virtio-gpu show graphics....");
    for _ in 0..10000 {
        for _ in 0..100000 {
            unsafe {
                core::arch::asm!("nop");
            }
        }
    }
    info!("virtio-gpu test finished");
}

fn virtio_input() {
    let mut input = INPUT.get().unwrap().lock();
    info!("testing input... Press ESC or right-click to continue.");
    loop {
        input.ack_interrupt().expect("fail to ack");
        if let Some(e) = input.pop_pending_event().expect("pop failed") {
            info!("input: {:?}", e);
            if e.event_type == 1 && (e.code == 1 || e.code == 273) && e.value == 0 {
                println!("ESC or right-click pressed, exit input test.");
                break;
            }
        }
    }
    // TODO: handle external interrupt
}

fn virtio_console() {
    let mut console = CONSOLE.get().unwrap().lock();
    let info = console.info().unwrap();
    println!("VirtIO console {} x {}", info.rows, info.columns);
    for &c in b"Hello console!\n" {
        console.send(c).expect("failed to send to console");
    }
    let c = console.recv(true).unwrap();
    println!("Read {:?} from console.", c);
    println!("virtio-console test finished");
}

static PACKAGE_IN: AtomicBool = AtomicBool::new(false);

static NET_BUF: Mutex<BTreeMap<u16, Box<[u8; 2048]>>> = Mutex::new(BTreeMap::new());
static NET_RES: Mutex<BTreeMap<u16, (usize, usize)>> = Mutex::new(BTreeMap::new());
fn virtio_net() {
    info!("virtio-net test start");
    #[cfg(not(feature = "tcp"))]
    unsafe {
        let mut net = NET_RAW.get().unwrap();
        info!("MAC address: {:02x?}", net.lock().mac_address());
        let token = {
            let mut buf = Box::new([0u8; 2048]);
            let mut net_buf = NET_BUF.lock();
            let token = net
                .lock()
                .receive_begin(buf.as_mut())
                .expect("failed to recv");
            net_buf.insert(token, buf);
            token
        };
        loop {
            if PACKAGE_IN.load(Ordering::Relaxed) {
                PACKAGE_IN.store(false, Ordering::Relaxed);
                let buf = NET_BUF.lock().remove(&token).unwrap();
                let (hdr_len, pkt_len) = NET_RES.lock().remove(&token).unwrap();
                info!(
                    "recv {} bytes: {:02x?}",
                    pkt_len,
                    &buf[hdr_len..hdr_len + pkt_len]
                );
                net.lock()
                    .send(&buf[..hdr_len + pkt_len])
                    .expect("failed to send");
                break;
            }
        }
        info!("virtio-net test finished");
    }

    #[cfg(feature = "tcp")]
    {
        const NET_BUFFER_LEN: usize = 2048;
        let net = NET.get().unwrap().lock();
        info!("MAC address: {:02x?}", net.mac_address());
        // crate::tcp::test_echo_server(net);
    }
}

impl DeviceBase for VirtIOBlk<MyHalImpl, MmioTransport> {
    fn handle_irq(&mut self) {
        self.ack_interrupt().expect("failed to ack interrupt");
    }
}

impl DeviceBase for VirtIOConsole<MyHalImpl, MmioTransport> {
    fn handle_irq(&mut self) {
        self.ack_interrupt().expect("failed to ack interrupt");
    }
}

impl DeviceBase for VirtIOGpu<MyHalImpl, MmioTransport> {
    fn handle_irq(&mut self) {
        self.ack_interrupt().expect("failed to ack interrupt");
    }
}

impl DeviceBase for VirtIOInput<MyHalImpl, MmioTransport> {
    fn handle_irq(&mut self) {
        self.ack_interrupt().expect("failed to ack interrupt");
    }
}

impl DeviceBase for VirtIONet<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }> {
    fn handle_irq(&mut self) {
        warn!("virtio-net interrupt");
        PACKAGE_IN.store(true, Ordering::Relaxed);
        self.ack_interrupt();
    }
}

impl DeviceBase for VirtIONetRaw<MyHalImpl, MmioTransport, { crate::NET_QUEUE_SIZE }> {
    fn handle_irq(&mut self) {
        warn!("virtio-net interrupt");
        // assert_eq!(in_token, token);
        let mut buf = NET_BUF.lock();
        for (token, buf) in buf.iter_mut() {
            let in_token = self.poll_receive(*token).unwrap();
            if in_token {
                let (hdr_len, pkt_len) =
                    unsafe { self.receive_complete(*token) }.expect("failed to recv");
                PACKAGE_IN.store(true, Ordering::Relaxed);
                NET_RES.lock().insert(*token, (hdr_len, pkt_len));
                self.ack_interrupt();
                break;
            }
        }
    }
}
