#import <Foundation/Foundation.h>
#import <Security/Security.h>
#include <string.h>
#include <stdlib.h>

// Called on a host I/O thread. Credential bytes never enter ordinary hooks.
int ps_credentials(const char *operation, const char *owner, const char *name,
                   const char *payload, char **output) {
    @autoreleasepool {
        if (!operation || !owner || !name || !output) return -1;
        *output = NULL;
        NSString *plugin = [NSString stringWithUTF8String:owner];
        NSString *key = [NSString stringWithUTF8String:name];
        if (!plugin || !key || plugin.length > 128 || key.length > 128) return -1;
        NSCharacterSet *allowed = [NSCharacterSet characterSetWithCharactersInString:
                                  @"abcdefghijklmnopqrstuvwxyz0123456789._-"];
        if (plugin.length == 0 || key.length == 0 ||
            [plugin rangeOfCharacterFromSet:allowed.invertedSet].location != NSNotFound ||
            [key rangeOfCharacterFromSet:allowed.invertedSet].location != NSNotFound) return -1;
        NSDictionary *query = @{(__bridge id)kSecClass:(__bridge id)kSecClassGenericPassword,
            (__bridge id)kSecAttrService:[@"PhiStudio.plugins." stringByAppendingString:plugin],
            (__bridge id)kSecAttrAccount:key};
        if (strcmp(operation, "remove") == 0) {
            OSStatus status = SecItemDelete((__bridge CFDictionaryRef)query);
            return status == errSecSuccess || status == errSecItemNotFound ? 0 : -1;
        }
        if (strcmp(operation, "read") == 0) {
            NSMutableDictionary *read = query.mutableCopy;
            read[(__bridge id)kSecReturnData] = @YES;
            read[(__bridge id)kSecMatchLimit] = (__bridge id)kSecMatchLimitOne;
            CFTypeRef result = NULL;
            OSStatus status = SecItemCopyMatching((__bridge CFDictionaryRef)read, &result);
            if (status == errSecItemNotFound) return 0;
            if (status != errSecSuccess || !result) return -1;
            NSData *data = CFBridgingRelease(result);
            if (![data isKindOfClass:[NSData class]] || data.length > 32 * 1024) return -1;
            NSString *text = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
            if (!text) return -1;
            *output = strdup(text.UTF8String);
            return *output ? 0 : -1;
        }
        if (strcmp(operation, "write") == 0 && payload) {
            NSString *text = [NSString stringWithUTF8String:payload];
            NSData *data = [text dataUsingEncoding:NSUTF8StringEncoding];
            if (!data || data.length > 32 * 1024) return -1;
            NSDictionary *attributes = @{(__bridge id)kSecValueData:data,
                (__bridge id)kSecAttrAccessible:(__bridge id)kSecAttrAccessibleWhenUnlockedThisDeviceOnly};
            OSStatus status = SecItemUpdate((__bridge CFDictionaryRef)query,
                                            (__bridge CFDictionaryRef)attributes);
            if (status == errSecItemNotFound) {
                NSMutableDictionary *add = query.mutableCopy;
                [add addEntriesFromDictionary:attributes];
                status = SecItemAdd((__bridge CFDictionaryRef)add, NULL);
            }
            return status == errSecSuccess ? 0 : -1;
        }
        return -1;
    }
}

void ps_free_credentials(char *value) {
    if (value) { memset(value, 0, strlen(value)); free(value); }
}
