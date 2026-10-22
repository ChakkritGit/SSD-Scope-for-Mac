// Original diagnostic, MIT. Cached USB descriptors only.
// Never opens, captures, resets, reconfigures or claims a USB device/interface.
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/IOCFPlugIn.h>
#include <IOKit/usb/IOUSBLib.h>
#include <stdio.h>

static int number(io_service_t entry, CFStringRef key) {
    CFTypeRef value = IORegistryEntryCreateCFProperty(entry, key, kCFAllocatorDefault, 0);
    int result = -1;
    if (value) {
        if (CFGetTypeID(value) == CFNumberGetTypeID())
            CFNumberGetValue((CFNumberRef)value, kCFNumberIntType, &result);
        CFRelease(value);
    }
    return result;
}

int main(void) {
    io_iterator_t iterator = IO_OBJECT_NULL;
    IOReturn status = IOServiceGetMatchingServices(kIOMainPortDefault,
        IOServiceMatching("IOUSBHostDevice"), &iterator);
    if (status != kIOReturnSuccess) {
        printf("enumeration: 0x%08x\n", status); return 1;
    }
    io_service_t device;
    int found = 0;
    while ((device = IOIteratorNext(iterator))) {
        if (number(device, CFSTR("idVendor")) != 0x0bda ||
            number(device, CFSTR("idProduct")) != 0x9201) {
            IOObjectRelease(device); continue;
        }
        found++;
        IOCFPlugInInterface **plugin = NULL;
        SInt32 score = 0;
        status = IOCreatePlugInInterfaceForService(device,
            kIOUSBDeviceUserClientTypeID, kIOCFPlugInInterfaceID, &plugin, &score);
        printf("RTL9201 plugin: 0x%08x\n", status);
        if (status == kIOReturnSuccess && plugin) {
            IOUSBDeviceInterface650 **usb = NULL;
            HRESULT result = (*plugin)->QueryInterface(plugin,
                CFUUIDGetUUIDBytes(kIOUSBDeviceInterfaceID650), (LPVOID *)&usb);
            printf("device interface: 0x%08x\n", (unsigned)result);
            if (result == 0 && usb) {
                UInt8 count = 0;
                status = (*usb)->GetNumberOfConfigurations(usb, &count);
                printf("configurations: %u (status 0x%08x)\n", count, status);
                for (UInt8 i = 0; status == 0 && i < count; i++) {
                    IOUSBConfigurationDescriptorPtr config = NULL;
                    IOReturn s = (*usb)->GetConfigurationDescriptorPtr(usb, i, &config);
                    printf("cached configuration %u: 0x%08x\n", i, s);
                    if (s != 0 || !config) continue;
                    const UInt8 *bytes = (const UInt8 *)config;
                    unsigned total = bytes[2] | ((unsigned)bytes[3] << 8);
                    if (total < 9 || total > 65535) continue;
                    for (unsigned offset = 0; offset + 2 <= total;) {
                        unsigned length = bytes[offset];
                        if (length < 2 || offset + length > total) break;
                        const UInt8 *d = bytes + offset;
                        if (d[1] == 4 && length >= 9)
                            printf("interface=%u alternate=%u class=%02x subclass=%02x protocol=%02x endpoints=%u\n",
                                d[2], d[3], d[5], d[6], d[7], d[4]);
                        if (d[1] == 5 && length >= 7)
                            printf("endpoint=%02x attributes=%02x\n", d[2], d[3]);
                        offset += length;
                    }
                }
                (*usb)->Release(usb);
            }
            IODestroyPlugInInterface(plugin);
        }
        IOObjectRelease(device);
    }
    IOObjectRelease(iterator);
    if (!found) { puts("Target USB ID not found"); return 2; }
    return 0;
}
