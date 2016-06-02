pub fn get_host_name() -> Result<String, ()> {
    get_host_name_native() 
}

#[cfg(windows)]
fn get_host_name_native() -> Result<String, ()> {
    extern crate winapi;
    extern crate kernel32;

    const MAX_COMPUTERNAME_LENGTH: usize = 31;

    let mut buf = Vec::<u16>::with_capacity(MAX_COMPUTERNAME_LENGTH + 1);
    unsafe {
        let capacity = buf.capacity();
        buf.set_len(capacity);

        let mut len: winapi::DWORD = buf.capacity() as winapi::DWORD - 1;
        if kernel32::GetComputerNameW(buf.as_mut_ptr(), &mut len as *mut winapi::DWORD) == winapi::FALSE
        {
            return Err(());
        }
        buf.set_len(len as usize);
    };
    match String::from_utf16(&buf) {
        Ok(s) => Ok(s),
        Err(_) => Err(()),
    }
}

#[cfg(unix)]
fn get_host_name_native() -> Result<String, ()> {
    extern crate libc;

    extern {
       fn gethostname(name: *mut libc::c_char, size: libc::size_t) -> libc::c_int;
    }

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
