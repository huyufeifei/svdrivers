use crate::DMA_PADDR;
use alloc::boxed::Box;
use core::sync::atomic::Ordering;
use safe_virtio_drivers::error::VirtIoResult;
use safe_virtio_drivers::hal::{DevicePage, QueuePage, VirtIoDeviceIo};
use safe_virtio_drivers::queue::{AvailRing, Descriptor, QueueLayout, QueueMutRef, UsedRing};
use safe_virtio_drivers::{PhysAddr, VirtAddr, PAGE_SIZE};

pub struct MyHalImpl;

pub struct Page {
    pa: usize,
    size: usize,
}

impl Page {
    pub fn new(pa: usize, size: usize) -> Self {
        Page { pa, size }
    }
}

#[derive(Debug)]
pub struct SafeIoRegion {
    base: usize,
    len: usize,
}

impl SafeIoRegion {
    pub fn new(base: usize, len: usize) -> Self {
        SafeIoRegion { base, len }
    }
}

impl VirtIoDeviceIo for SafeIoRegion {
    #[inline]
    fn read_volatile_u32_at(&self, off: usize) -> VirtIoResult<u32> {
        let ptr = (self.base + off) as *const u32;
        Ok(unsafe { ptr.read_volatile() })
    }
    #[inline]
    fn read_volatile_u8_at(&self, off: usize) -> VirtIoResult<u8> {
        let ptr = (self.base + off) as *const u8;
        Ok(unsafe { ptr.read_volatile() })
    }
    #[inline]
    fn write_volatile_u32_at(&self, off: usize, data: u32) -> VirtIoResult<()> {
        let ptr = (self.base + off) as *mut u32;
        unsafe {
            ptr.write_volatile(data);
        }
        Ok(())
    }
    #[inline]
    fn write_volatile_u8_at(&self, off: usize, data: u8) -> VirtIoResult<()> {
        let ptr = (self.base + off) as *mut u8;
        unsafe {
            ptr.write_volatile(data);
        }
        Ok(())
    }
    fn paddr(&self) -> PhysAddr {
        self.base as PhysAddr
    }

    fn vaddr(&self) -> VirtAddr {
        self.base as VirtAddr
    }
}

impl<const SIZE: usize> safe_virtio_drivers::hal::Hal<SIZE> for MyHalImpl {
    #[inline]
    fn dma_alloc(pages: usize) -> Box<dyn QueuePage<SIZE>> {
        let paddr = DMA_PADDR.fetch_add(PAGE_SIZE * pages, Ordering::SeqCst);
        info!("<dma_alloc>alloc DMA: paddr={:#x}, pages={}", paddr, pages);
        Box::new(Page::new(paddr, PAGE_SIZE * pages))
    }

    #[inline]
    fn dma_alloc_buf(pages: usize) -> Box<dyn DevicePage> {
        let paddr = DMA_PADDR.fetch_add(PAGE_SIZE * pages, Ordering::SeqCst);
        info!(
            "<dma_alloc_buf> alloc DMA: paddr={:#x}, pages={}",
            paddr, pages
        );
        Box::new(Page::new(paddr, PAGE_SIZE * pages))
    }

    #[inline]
    fn to_paddr(va: usize) -> usize {
        va
    }
}

impl DevicePage for Page {
    #[inline]
    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.pa as *mut u8, self.size) }
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.pa as *const u8, self.size) }
    }

    #[inline]
    fn paddr(&self) -> VirtAddr {
        self.pa as VirtAddr
    }

    #[inline]
    fn vaddr(&self) -> PhysAddr {
        self.pa
    }
}

impl<const SIZE: usize> QueuePage<SIZE> for Page {
    fn queue_ref_mut(&mut self, layout: &QueueLayout) -> QueueMutRef<SIZE> {
        let desc_table_offset = layout.descriptor_table_offset;
        let table = unsafe {
            let ptr = (self.pa + desc_table_offset) as *mut Descriptor;
            core::slice::from_raw_parts_mut(ptr, SIZE)
        };
        let avail_ring_offset = layout.avail_ring_offset;
        let avail_ring = unsafe {
            let ptr = (self.pa + avail_ring_offset) as *mut AvailRing<SIZE>;
            &mut *ptr
        };

        let used_ring_offset = layout.used_ring_offset;
        let used_ring = unsafe {
            let ptr = (self.pa + used_ring_offset) as *mut UsedRing<SIZE>;
            &mut *ptr
        };
        QueueMutRef {
            descriptor_table: table,
            avail_ring,
            used_ring,
        }
    }
}
