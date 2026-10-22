// Original diagnostic, MIT. Queries interface availability only.
// Does not obtain exclusive access, create a task, or send any SCSI command.
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/IOBSD.h>
#include <IOKit/IOCFPlugIn.h>
#include <IOKit/scsi/SCSITaskLib.h>
#include <stdio.h>
#include <string.h>
#include <ctype.h>

int main(int argc, char **argv) {
    if (argc != 2 || strncmp(argv[1], "disk", 4) || !argv[1][4]) return 2;
    for (const char *p = argv[1] + 4; *p; p++) if (!isdigit((unsigned char)*p)) return 2;
    io_service_t entry = IOServiceGetMatchingService(kIOMainPortDefault,
        IOBSDNameMatching(kIOMainPortDefault, 0, argv[1]));
    if (!entry) return 3;
    for (unsigned depth = 0; entry && depth < 32; depth++) {
        io_name_t class_name;
        if (IOObjectGetClass(entry, class_name) == 0) {
            printf("ancestor: %s\n", class_name);
            if (strstr(class_name, "SCSIPeripheral")) {
                IOCFPlugInInterface **plugin = NULL;
                SInt32 score = 0;
                IOReturn status = IOCreatePlugInInterfaceForService(entry,
                    kIOSCSITaskDeviceUserClientTypeID, kIOCFPlugInInterfaceID, &plugin, &score);
                printf("SCSI plugin: 0x%08x\n", status);
                if (status == 0 && plugin) {
                    SCSITaskDeviceInterface **scsi = NULL;
                    HRESULT query = (*plugin)->QueryInterface(plugin,
                        CFUUIDGetUUIDBytes(kIOSCSITaskDeviceInterfaceID), (LPVOID *)&scsi);
                    printf("SCSI device interface: 0x%08x\n", (unsigned)query);
                    if (query == 0 && scsi) (*scsi)->Release(scsi);
                    IODestroyPlugInInterface(plugin);
                }
            }
        }
        io_registry_entry_t parent = IO_OBJECT_NULL;
        IOReturn status = IORegistryEntryGetParentEntry(entry, kIOServicePlane, &parent);
        IOObjectRelease(entry); entry = status == 0 ? parent : IO_OBJECT_NULL;
    }
    if (entry) IOObjectRelease(entry);
    return 0;
}
