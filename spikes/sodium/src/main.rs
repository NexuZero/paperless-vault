fn main() {
    unsafe {
        assert_eq!(libsodium_sys::sodium_init(), 0);
        println!("libsodium initialized via FFI, version: {:?}",
            std::ffi::CStr::from_ptr(libsodium_sys::sodium_version_string()));
    }
}
