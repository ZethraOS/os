#include <stdio.h>
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/usb/IOUSBLib.h>

int main() {
    printf("=== macOS IOKit USB Probe ===\n");
    CFMutableDictionaryRef matchingDict = IOServiceMatching("IOUSBHostDevice");
    if (!matchingDict) {
        printf("Failed to create matching dictionary for IOUSBHostDevice\n");
        return 1;
    }

    io_iterator_t iter;
    kern_return_t kr = IOServiceGetMatchingServices(kIOMainPortDefault, matchingDict, &iter);
    if (kr != KERN_SUCCESS) {
        printf("IOServiceGetMatchingServices failed: 0x%08x\n", kr);
        return 1;
    }

    io_service_t device;
    int count = 0;
    while ((device = IOIteratorNext(iter)) != 0) {
        count++;
        CFStringRef name = (CFStringRef)IORegistryEntryCreateCFProperty(device, CFSTR("USB Product Name"), kCFAllocatorDefault, 0);
        CFNumberRef vid = (CFNumberRef)IORegistryEntryCreateCFProperty(device, CFSTR("idVendor"), kCFAllocatorDefault, 0);
        CFNumberRef pid = (CFNumberRef)IORegistryEntryCreateCFProperty(device, CFSTR("idProduct"), kCFAllocatorDefault, 0);
        
        char nameBuf[256] = "Unknown";
        if (name) {
            CFStringGetCString(name, nameBuf, sizeof(nameBuf), kCFStringEncodingUTF8);
            CFRelease(name);
        }
        int v = 0, p = 0;
        if (vid) { CFNumberGetValue(vid, kCFNumberIntType, &v); CFRelease(vid); }
        if (pid) { CFNumberGetValue(pid, kCFNumberIntType, &p); CFRelease(pid); }

        printf("Device %d: %s [VID: 0x%04x, PID: 0x%04x]\n", count, nameBuf, v, p);
        IOObjectRelease(device);
    }
    IOObjectRelease(iter);
    printf("Total IOUSBHostDevice instances: %d\n", count);

    // Also check AppleUSBHostPort
    matchingDict = IOServiceMatching("AppleUSBHostPort");
    if (matchingDict) {
        kr = IOServiceGetMatchingServices(kIOMainPortDefault, matchingDict, &iter);
        if (kr == KERN_SUCCESS) {
            printf("\n--- AppleUSBHostPort list ---\n");
            while ((device = IOIteratorNext(iter)) != 0) {
                io_name_t devName;
                IORegistryEntryGetName(device, devName);
                CFNumberRef portNum = (CFNumberRef)IORegistryEntryCreateCFProperty(device, CFSTR("port-status"), kCFAllocatorDefault, 0);
                int status = -1;
                if (portNum) { CFNumberGetValue(portNum, kCFNumberIntType, &status); CFRelease(portNum); }
                printf("Port: %s | status: %d (0x%x)\n", devName, status, status);
                IOObjectRelease(device);
            }
            IOObjectRelease(iter);
        }
    }

    return 0;
}
