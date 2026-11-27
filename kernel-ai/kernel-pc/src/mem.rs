//! Funzioni di memoria che il compilatore si aspetta dalla libc (qui non c'è).

use core::arch::asm;

#[no_mangle]
pub unsafe extern "C" fn memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    asm!("rep movsb", inout("rcx") n => _, inout("rdi") dst => _, inout("rsi") src => _, options(nostack, preserves_flags));
    dst
}

#[no_mangle]
pub unsafe extern "C" fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if (dst as usize) <= (src as usize) || (dst as usize) >= (src as usize) + n {
        memcpy(dst, src, n)
    } else {
        // copia all'indietro
        asm!("std", "rep movsb", "cld",
             inout("rcx") n => _, inout("rdi") dst.add(n - 1) => _, inout("rsi") src.add(n - 1) => _, options(nostack));
        dst
    }
}

#[no_mangle]
pub unsafe extern "C" fn memset(dst: *mut u8, c: i32, n: usize) -> *mut u8 {
    asm!("rep stosb", inout("rcx") n => _, inout("rdi") dst => _, in("al") c as u8, options(nostack, preserves_flags));
    dst
}

#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let (x, y) = (core::ptr::read_volatile(a.add(i)), core::ptr::read_volatile(b.add(i)));
        if x != y {
            return x as i32 - y as i32;
        }
        i += 1;
    }
    0
}

#[no_mangle]
pub unsafe extern "C" fn bcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    memcmp(a, b, n)
}

#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut n = 0;
    while core::ptr::read_volatile(s.add(n)) != 0 {
        n += 1;
    }
    n
}

// La libreria core precompilata per il target host fa riferimento allo srotolamento dello stack;
// qui panic = abort, quindi questi simboli non vengono mai chiamati.
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    crate::plat::halt()
}

#[no_mangle]
pub extern "C" fn rust_eh_personality() {}
