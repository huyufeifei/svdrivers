use core::mem::size_of;

use crate::error::VirtIoResult;
use crate::hal::Hal;
use crate::queue::{DescFlag, Descriptor, VirtIoQueue};
use crate::transport::Transport;
use crate::volatile::{ReadVolatile, WriteVolatile};
use alloc::{boxed::Box, vec};

mod ty;

use ty::*;

const QUEUE_EVENT: u16 = 0;
const QUEUE_STATUS: u16 = 1;
const SUPPORTED_FEATURES: InputFeature = InputFeature::empty(); // InputFeature::RING_EVENT_IDX;

// a parameter that can change
const QUEUE_SIZE: usize = 32;

/// Virtual human interface devices such as keyboards, mice and tablets.
///
/// An instance of the virtio device represents one such input device.
/// Device behavior mirrors that of the evdev layer in Linux,
/// making pass-through implementations on top of evdev easy.
pub struct VirtIOInput<H: Hal<QUEUE_SIZE>, T: Transport> {
    transport: T,
    event_queue: VirtIoQueue<H, QUEUE_SIZE>,
    status_queue: VirtIoQueue<H, QUEUE_SIZE>,
    event_buf: Box<[InputEvent; QUEUE_SIZE]>,
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> VirtIOInput<H, T> {
    /// Create a new VirtIO-Input driver.
    pub fn new(mut transport: T) -> VirtIoResult<Self> {
        transport.begin_init(SUPPORTED_FEATURES)?;
        let event_buf = Box::new([InputEvent::default(); QUEUE_SIZE]);

        let mut event_queue = VirtIoQueue::new(&mut transport, QUEUE_EVENT)?;
        let status_queue = VirtIoQueue::new(&mut transport, QUEUE_STATUS)?;
        for (i, event) in event_buf.iter().enumerate() {
            // Safe because the buffer lasts as long as the queue.
            // let token = unsafe { event_queue.add(&[], &mut [event.as_bytes_mut()])? };
            let token = event_queue.add(vec![Descriptor::new::<QUEUE_SIZE, H>(
                event as *const InputEvent as _,
                size_of::<InputEvent>() as _,
                DescFlag::WRITE,
            )])?;
            assert_eq!(token, i as _);
        }
        if event_queue.should_notify() {
            transport.notify(QUEUE_EVENT)?;
        }

        transport.finish_init()?;

        Ok(VirtIOInput {
            transport,
            event_queue,
            status_queue,
            event_buf,
        })
    }

    /// Acknowledge interrupt and process events.
    pub fn ack_interrupt(&mut self) -> VirtIoResult<bool> {
        self.transport.ack_interrupt()
    }

    /// Pop the pending event.
    pub fn pop_pending_event(&mut self) -> VirtIoResult<Option<InputEvent>> {
        if let Some(token) = self.event_queue.peek_used() {
            let _ = self.event_queue.pop_used(token)?;
            let event_saved = self.event_buf[token as usize];
            let new_token = self.event_queue.add(vec![Descriptor::new::<QUEUE_SIZE, H>(
                &self.event_buf[token as usize] as *const InputEvent as _,
                size_of::<InputEvent>() as _,
                DescFlag::WRITE,
            )])?;
            assert_eq!(new_token, token);
            if self.event_queue.should_notify() {
                self.transport.notify(QUEUE_EVENT)?;
            }
            Ok(Some(event_saved))
        } else {
            Ok(None)
        }
    }

    /// Query a specific piece of information by `select` and `subsel`, and write
    /// result to `out`, return the result size.
    pub fn query_config_select(
        &mut self,
        select: InputConfigSelect,
        subsel: u8,
        out: &mut [u8],
    ) -> VirtIoResult<u8> {
        let config = InputConfig::default();
        let io_region = self.transport.io_region();
        config.select.write(select as _, io_region)?;
        config.subsel.write(subsel, io_region)?;
        let size = config.size.read(io_region)?;
        let data = config.data.read(io_region)?;
        out[..size as usize].copy_from_slice(&data[..size as usize]);
        Ok(size)
    }
}

impl<H: Hal<QUEUE_SIZE>, T: Transport> Drop for VirtIOInput<H, T> {
    fn drop(&mut self) {
        // Clear any pointers pointing to DMA regions, so the device doesn't try to access them
        // after they have been freed.
        self.transport
            .queue_unset(QUEUE_EVENT)
            .expect("failed to unset event queue");
        self.transport
            .queue_unset(QUEUE_STATUS)
            .expect("failed to unset status queue");
    }
}
