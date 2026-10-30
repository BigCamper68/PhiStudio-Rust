#import <AppKit/AppKit.h>
void phistudio_set_app_icon(const unsigned char *bytes, unsigned long length) {
    @autoreleasepool {
        NSData *data = [NSData dataWithBytes:bytes length:length];
        NSImage *image = [[NSImage alloc] initWithData:data];
        if (image) [NSApplication sharedApplication].applicationIconImage = image;
    }
}
