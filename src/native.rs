//! Read-only IORegistry access. No USB interface is seized and no disk is opened for writing.
use anyhow::{Context, Result, ensure};
use core_foundation_sys::{
    base::{CFRelease, CFTypeRef, kCFAllocatorDefault},
    data::{CFDataGetBytePtr, CFDataGetLength},
    dictionary::CFMutableDictionaryRef,
    propertylist::{CFPropertyListCreateData, kCFPropertyListXMLFormat_v1_0},
};
use plist::{Dictionary, Value};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CString, c_char},
    ptr,
};

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOBSDNameMatching(port: u32, options: u32, name: *const c_char) -> CFMutableDictionaryRef;
    fn IOServiceGetMatchingService(port: u32, matching: CFMutableDictionaryRef) -> u32;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> i32;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut CFMutableDictionaryRef,
        allocator: *const std::ffi::c_void,
        options: u32,
    ) -> i32;
    fn IOObjectGetClass(entry: u32, name: *mut c_char) -> i32;
    fn IOObjectRelease(entry: u32) -> i32;
    fn IORegistryEntryGetRegistryEntryID(entry: u32, id: *mut u64) -> i32;
}

struct Entry(u32);
impl Drop for Entry {
    fn drop(&mut self) {
        unsafe {
            IOObjectRelease(self.0);
        }
    }
}
struct Cf(CFTypeRef);
impl Drop for Cf {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.0);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsbBridge {
    pub vendor_id: u16,
    pub product_id: u16,
    pub revision: u16,
    pub product: String,
    pub protocol: Option<u64>,
    pub link_bps: Option<u64>,
    pub driver: String,
    pub ata_smart_exposed: bool,
}
impl UsbBridge {
    pub fn label(&self) -> String {
        if (self.vendor_id, self.product_id) == (0x0bda, 0x9201) {
            "Realtek RTL9201 · USB → SATA".into()
        } else {
            format!("USB {:04x}:{:04x}", self.vendor_id, self.product_id)
        }
    }
    pub fn usb_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor_id, self.product_id)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Counters {
    pub registry_id: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct RegistrySnapshot {
    pub usb: Option<UsbBridge>,
    pub counters: Option<Counters>,
}

fn property_dict(entry: u32) -> Result<Dictionary> {
    let mut properties = ptr::null_mut();
    ensure!(
        unsafe {
            IORegistryEntryCreateCFProperties(entry, &mut properties, kCFAllocatorDefault, 0)
        } == 0
            && !properties.is_null(),
        "Cannot read IORegistry properties"
    );
    let properties = Cf(properties.cast());
    let mut error = ptr::null_mut();
    let data = unsafe {
        CFPropertyListCreateData(
            kCFAllocatorDefault,
            properties.0,
            kCFPropertyListXMLFormat_v1_0,
            0,
            &mut error,
        )
    };
    if !error.is_null() {
        unsafe {
            CFRelease(error.cast());
        }
    }
    ensure!(!data.is_null(), "Cannot serialize IORegistry properties");
    let owned = Cf(data.cast());
    let length = unsafe { CFDataGetLength(data) };
    ensure!(
        length > 0 && length <= 8 * 1024 * 1024,
        "Unexpected IORegistry property size"
    );
    let bytes = unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(data), length as usize) };
    let result = crate::disks::parse_plist(bytes);
    drop(owned);
    result
}
pub fn inspect(id: &str) -> Result<RegistrySnapshot> {
    ensure!(crate::disks::valid_id(id), "Invalid device identifier");
    let name = CString::new(id)?;
    let matcher = unsafe { IOBSDNameMatching(0, 0, name.as_ptr()) };
    ensure!(!matcher.is_null(), "Cannot match disk");
    // IOServiceGetMatchingService consumes the matching dictionary.
    let mut entry = Entry(unsafe { IOServiceGetMatchingService(0, matcher) });
    ensure!(entry.0 != 0, "Disk is no longer connected");
    let mut snapshot = RegistrySnapshot::default();
    let mut driver = String::new();
    let mut protocol = None;
    let mut ata = false;
    for _ in 0..64 {
        let mut class = [0_i8; 128];
        let class_name = if unsafe { IOObjectGetClass(entry.0, class.as_mut_ptr()) } == 0 {
            unsafe { std::ffi::CStr::from_ptr(class.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };
        if let Ok(p) = property_dict(entry.0) {
            ata |= p.get("SMART Capable").and_then(Value::as_boolean) == Some(true)
                || p.get("SATSMARTCapable").and_then(Value::as_boolean) == Some(true);
            if class_name == "IOBlockStorageDriver"
                && let Some(stats) = p.get("Statistics").and_then(Value::as_dictionary)
                && let (Some(read), Some(write)) = (
                    stats
                        .get("Bytes (Read)")
                        .and_then(Value::as_unsigned_integer),
                    stats
                        .get("Bytes (Write)")
                        .and_then(Value::as_unsigned_integer),
                )
            {
                let mut registry_id = 0;
                if unsafe { IORegistryEntryGetRegistryEntryID(entry.0, &mut registry_id) } == 0 {
                    snapshot.counters = Some(Counters {
                        registry_id,
                        read_bytes: read,
                        write_bytes: write,
                    });
                }
            }
            if driver.is_empty()
                && class_name.contains("MassStorage")
                && class_name.contains("Driver")
            {
                driver = class_name.clone();
            }
            if let Some(v) = p.get("USB Device Info").and_then(Value::as_dictionary) {
                protocol = v
                    .get("bInterfaceProtocol")
                    .and_then(Value::as_unsigned_integer)
                    .or(protocol);
            }
            if class_name == "IOUSBHostDevice" {
                let num = |key: &str| p.get(key).and_then(Value::as_unsigned_integer);
                snapshot.usb = Some(UsbBridge {
                    vendor_id: num("idVendor").context("Missing USB vendor ID")? as u16,
                    product_id: num("idProduct").context("Missing USB product ID")? as u16,
                    revision: num("bcdDevice").unwrap_or(0) as u16,
                    product: p
                        .get("USB Product Name")
                        .and_then(Value::as_string)
                        .unwrap_or("USB storage")
                        .into(),
                    link_bps: num("UsbLinkSpeed"),
                    protocol,
                    driver,
                    ata_smart_exposed: ata,
                });
                break;
            }
        }
        let mut parent = 0;
        if unsafe { IORegistryEntryGetParentEntry(entry.0, c"IOService".as_ptr(), &mut parent) }
            != 0
            || parent == 0
        {
            break;
        }
        entry = Entry(parent);
    }
    Ok(snapshot)
}
