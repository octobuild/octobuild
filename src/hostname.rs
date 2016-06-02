extern crate libc;

extern {
    pub fn gethostname(name: *mut libc::c_char, size: libc::size_t) -> libc::c_int;
}

pub fn get_host_name() -> Result<String, ()> {
    let mut buf = Vec::<u8>::with_capacity(0x100);
    unsafe {
        let capacity = buf.capacity();
        buf.set_len(capacity);
    }
    let err = unsafe {
        gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len() as libc::size_t)
    } as isize;
    match err {
        0 => {
            let mut i = 0;
            while i < buf.len()
            {
                if buf[i] == 0
                {
                    buf.resize(i, 0);
                    break;
                }
                i += 1;
            }
            Ok(String::from_utf8_lossy(&buf).into_owned())
        },
        _ => {
            Err(())
        }
    }
}