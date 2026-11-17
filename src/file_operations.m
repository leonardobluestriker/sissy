#import <Foundation/Foundation.h>
#include <stdlib.h>
#include <string.h>
// FileManager performs copy-then-remove when crossing volumes, retaining OS
// metadata handling and refusing an existing destination. Never overwrites.
char *sissy_move_item(const unsigned char *source, size_t sourceLen,
                     const unsigned char *target, size_t targetLen) {
    @autoreleasepool {
        NSString *from = [[NSString alloc] initWithBytes:source length:sourceLen encoding:NSUTF8StringEncoding];
        NSString *to = [[NSString alloc] initWithBytes:target length:targetLen encoding:NSUTF8StringEncoding];
        NSError *error = nil;
        if (from && to && [[[NSFileManager alloc] init] moveItemAtURL:[NSURL fileURLWithPath:from]
            toURL:[NSURL fileURLWithPath:to] error:&error]) return NULL;
        return strdup((error.localizedDescription ?: @"Invalid file path").UTF8String);
    }
}
void sissy_move_error_free(char *error) { free(error); }
// HFS+ decomposes Unicode filenames while APFS generally preserves spelling.
// Compare canonical Unicode forms when verifying a cross-filesystem move.
char *sissy_normalized_path(const unsigned char *path, size_t len) {
    @autoreleasepool {
        NSString *value = [[NSString alloc] initWithBytes:path length:len encoding:NSUTF8StringEncoding];
        return value ? strdup(value.precomposedStringWithCanonicalMapping.UTF8String) : NULL;
    }
}
