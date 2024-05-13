use crate::DMA_PADDR;
use core::ptr::NonNull;
use core::sync::atomic::Ordering;
use safe_virtio_drivers::PAGE_SIZE;
use virtio_drivers::{BufferDirection, Hal, PhysAddr};

pub struct HalImpl;

unsafe impl Hal for HalImpl {
    fn dma_alloc(pages: usize, _direction: BufferDirection) -> (PhysAddr, NonNull<u8>) {
        let paddr = DMA_PADDR.fetch_add(PAGE_SIZE * pages, Ordering::SeqCst);
        info!("<dma_alloc>alloc DMA: paddr={:#x}, pages={}", paddr, pages);
        (paddr, NonNull::new(paddr as *mut u8).unwrap())
    }

    unsafe fn dma_dealloc(paddr: PhysAddr, vaddr: NonNull<u8>, pages: usize) -> i32 {
        info!(
            "<dma_dealloc>dealloc DMA: paddr={:#x}, pages={}",
            paddr, pages
        );
        0
    }

    unsafe fn mmio_phys_to_virt(paddr: PhysAddr, _size: usize) -> NonNull<u8> {
        NonNull::new(paddr as *mut u8).unwrap()
    }

    unsafe fn share(buffer: NonNull<[u8]>, _direction: BufferDirection) -> PhysAddr {
        let buffer = buffer.as_ptr();
        buffer as *const u8 as PhysAddr
    }

    unsafe fn unshare(_paddr: PhysAddr, _buffer: NonNull<[u8]>, _direction: BufferDirection) {}
}
