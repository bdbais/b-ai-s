//! Allocatore dell'heap a classi di dimensione (riusa la memoria liberata), come nella v0.2.
//! Un solo core e niente interruzioni: nessun lock.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;

const CLASSES: usize = 21; // 16 B .. 16 MB
const BIG_STEP: usize = 64 << 10;
const BIG_SLOTS: usize = 64;

struct Inner {
    next: usize,
    end: usize,
    free: [usize; CLASSES],
    big: [(usize, usize); BIG_SLOTS], // (indirizzo, dimensione) dei blocchi grandi liberati
    used: usize,
}

pub struct Heap(UnsafeCell<Inner>);
unsafe impl Sync for Heap {}

#[global_allocator]
pub static HEAP: Heap = Heap(UnsafeCell::new(Inner { next: 0, end: 0, free: [0; CLASSES], big: [(0, 0); BIG_SLOTS], used: 0 }));

pub fn init(start: usize, end: usize) {
    let h = unsafe { &mut *HEAP.0.get() };
    h.next = (start + 4095) & !4095;
    h.end = end;
    h.free = [0; CLASSES];
    h.big = [(0, 0); BIG_SLOTS];
    h.used = 0;
}

pub fn stats() -> (usize, usize) {
    let h = unsafe { &*HEAP.0.get() };
    (h.used, h.end.saturating_sub(h.next) + h.used)
}

fn class(size: usize) -> Option<usize> {
    let s = size.max(16).next_power_of_two();
    let c = s.trailing_zeros() as usize - 4;
    if c < CLASSES {
        Some(c)
    } else {
        None
    }
}

impl Inner {
    fn bump(&mut self, size: usize, align: usize) -> *mut u8 {
        let a = (self.next + align - 1) & !(align - 1);
        if a + size > self.end {
            return core::ptr::null_mut();
        }
        self.next = a + size;
        a as *mut u8
    }
}

unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let h = &mut *self.0.get();
        let size = l.size().max(l.align());
        match class(size) {
            Some(c) if size <= (1 << 20) => {
                let bs = 16usize << c;
                let p = if h.free[c] != 0 {
                    let p = h.free[c];
                    h.free[c] = *(p as *const usize);
                    p as *mut u8
                } else {
                    h.bump(bs, bs.min(4096))
                };
                if !p.is_null() {
                    h.used += bs;
                }
                p
            }
            _ => {
                let bs = (size + BIG_STEP - 1) & !(BIG_STEP - 1);
                // primo blocco libero abbastanza grande
                let mut best: Option<usize> = None;
                for (i, &(a, s)) in h.big.iter().enumerate() {
                    if a != 0 && s >= bs && best.map(|b| h.big[b].1 > s).unwrap_or(true) {
                        best = Some(i);
                    }
                }
                let p = if let Some(i) = best {
                    let (a, _) = h.big[i];
                    h.big[i] = (0, 0);
                    a as *mut u8
                } else {
                    h.bump(bs, 4096.max(l.align()))
                };
                if !p.is_null() {
                    h.used += bs;
                }
                p
            }
        }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        let h = &mut *self.0.get();
        let size = l.size().max(l.align());
        match class(size) {
            Some(c) if size <= (1 << 20) => {
                *(p as *mut usize) = h.free[c];
                h.free[c] = p as usize;
                h.used -= 16usize << c;
            }
            _ => {
                let bs = (size + BIG_STEP - 1) & !(BIG_STEP - 1);
                h.used -= bs;
                if let Some(slot) = h.big.iter_mut().find(|s| s.0 == 0) {
                    *slot = (p as usize, bs);
                }
                // se la tabella è piena il blocco si perde: accettabile per un prototipo
            }
        }
    }
}
