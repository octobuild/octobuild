#![feature(collections)]
#![feature(asm)]
#![feature(io)]
#![allow(non_snake_case)]

extern crate winapi;
extern crate "advapi32-sys" as advapi32;

use std::old_io::{File, Open, ReadWrite, IoError, SeekStyle};
use std::ptr;
use std::mem::*;
use winapi::*;
use advapi32::*;

fn main() {
	log("BEGIN");
	
	unsafe {
		let service_name = service_name();
		let service_table: &[*const SERVICE_TABLE_ENTRYW] = &[
			&SERVICE_TABLE_ENTRYW {
				lpServiceName: service_name.as_ptr(),
				lpServiceProc: service_main,
			},
			ptr::null()
		];
		//asm!(" int3");
		let result = StartServiceCtrlDispatcherW(*service_table.as_ptr());
		log(format! ("StartServiceCtrlDispatcherW: {}", result).as_slice());
	}
	log("END");
}

fn service_name() -> Vec<u16> {
	let name = "hello";
	let mut result: Vec<u16> = name.utf16_units().collect();
	result.push(0);
	result
}

fn create_service_status(current_state: DWORD) -> SERVICE_STATUS {
	SERVICE_STATUS {
		dwServiceType: SERVICE_WIN32_OWN_PROCESS,
		dwCurrentState: current_state,
		dwControlsAccepted: SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN,
		dwWin32ExitCode: 0,
		dwServiceSpecificExitCode: 0,
		dwCheckPoint: 0,
		dwWaitHint: 0,
	}
}

static mut service_handle : Option<SERVICE_STATUS_HANDLE> = None;

unsafe extern "system" fn service_main(
	dwNumServicesArgs: DWORD,
	lpServiceArgVectors: *mut LPWSTR,
) {
	log("service_main: BEGIN");
	let service_name = service_name();
	let handle = RegisterServiceCtrlHandlerExW(service_name.as_ptr(), control_handler, ptr::null_mut()); 
	service_handle = Some(handle);
	SetServiceStatus (handle, &mut create_service_status(SERVICE_START_PENDING));
	SetServiceStatus (handle, &mut create_service_status(SERVICE_RUNNING));
	log("service_main: END");
}

unsafe extern "system" fn control_handler(
	dwControl: DWORD,
	dwEventType: DWORD,
	lpEventData: LPVOID,
	lpContext: LPVOID
) -> DWORD {
	match dwControl {
		SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
			log("control_handler: STOP");
			match service_handle {
				Some(handle) => {SetServiceStatus (handle, &mut create_service_status(SERVICE_STOPPED));}
				None => {}
			}
		}
		_ => {}
	};
	0
}

fn log(message: &str) -> Result<(), IoError> {
	let mut file = try! (File::open_mode(&Path::new("C:\\Bozaro\\github\\winapi-rs\\advapi32-sys\\target\\log.txt"), Open, ReadWrite));
	file.seek(0, SeekStyle::SeekEnd);
	file.write_line(message)
}
