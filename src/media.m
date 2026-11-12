#import <AppKit/AppKit.h>
#import <AVKit/AVKit.h>
#import <AVFoundation/AVFoundation.h>
#import <Quartz/Quartz.h>
#import <QuickLookThumbnailing/QuickLookThumbnailing.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <ImageIO/ImageIO.h>
#include <stdlib.h>
#include <string.h>

static NSURL *fileURL(const unsigned char *path, size_t len) {
    NSString *name = [[NSString alloc] initWithBytes:path length:len encoding:NSUTF8StringEncoding];
    return name ? [NSURL fileURLWithPath:name] : nil;
}

// Runs on a Rust worker. A bounded wait keeps unsupported providers from
// blocking the UI; no media or thumbnail files are written by Sissy.
unsigned char *sissy_thumbnail(const unsigned char *path, size_t len, size_t *outLen) {
    @autoreleasepool {
        *outLen = 0;
        NSURL *url = fileURL(path, len);
        if (!url) return NULL;
        QLThumbnailGenerationRequest *request = [[QLThumbnailGenerationRequest alloc]
            initWithFileAtURL:url size:CGSizeMake(96, 96) scale:2
            representationTypes:QLThumbnailGenerationRequestRepresentationTypeAll];
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        __block NSData *png = nil;
        NSLock *lock = [[NSLock alloc] init];
        [[QLThumbnailGenerator sharedGenerator] generateBestRepresentationForRequest:request
            completionHandler:^(QLThumbnailRepresentation *thumbnail, NSError *error) {
                (void)error;
                if (thumbnail.CGImage) {
                    NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithCGImage:thumbnail.CGImage];
                    NSData *data = [rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
                    [lock lock]; png = data; [lock unlock];
                }
                dispatch_semaphore_signal(done);
            }];
        if (dispatch_semaphore_wait(done, dispatch_time(DISPATCH_TIME_NOW, 5 * NSEC_PER_SEC))) {
            [[QLThumbnailGenerator sharedGenerator] cancelRequest:request];
        }
        [lock lock]; NSData *data = png; [lock unlock];
        if (!data) return NULL;
        unsigned char *bytes = malloc(data.length);
        if (bytes) { memcpy(bytes, data.bytes, data.length); *outLen = data.length; }
        return bytes;
    }
}
void sissy_media_free(unsigned char *bytes) { free(bytes); }

@interface SissyPreviewController : NSWindowController <NSWindowDelegate>
@property(nonatomic, strong) AVPlayer *player;
@property(nonatomic, strong) AVPlayerItem *observedItem;
@property(nonatomic, strong) QLPreviewView *quickLook;
@property(nonatomic, strong) NSTextField *message;
- (void)clearMedia;
@end
@implementation SissyPreviewController
- (void)clearMedia {
    [self.player pause];
    if (self.observedItem) [self.observedItem removeObserver:self forKeyPath:@"status"];
    self.observedItem = nil;
    [self.player replaceCurrentItemWithPlayerItem:nil];
    self.player = nil;
    [self.quickLook close];
    self.quickLook = nil;
}
- (void)windowWillClose:(NSNotification *)note { (void)note; [self clearMedia]; }
- (void)observeValueForKeyPath:(NSString *)key ofObject:(id)object
                       change:(NSDictionary *)change context:(void *)context {
    (void)key; (void)change; (void)context;
    if (object != self.observedItem) return;
    if (self.observedItem.status == AVPlayerItemStatusFailed) {
        NSString *reason = self.observedItem.error.localizedDescription ?: @"";
        AVPlayerItem *failedItem = self.observedItem;
        dispatch_async(dispatch_get_main_queue(), ^{
            if (self.observedItem != failedItem) return;
            self.message.stringValue = reason;
            self.message.hidden = NO;
        });
    }
}
@end
static SissyPreviewController *preview;

// Called from the UI thread. This is an application-owned window, never
// NSWorkspace.openURL or an external Quick Look panel/application.
int sissy_preview(const unsigned char *path, size_t len,
                  const unsigned char *titleBytes, size_t titleLen,
                  const unsigned char *errorBytes, size_t errorLen) {
    NSURL *url = fileURL(path, len);
    if (!url || ![url checkResourceIsReachableAndReturnError:nil]) return 0;
    NSString *title = [[NSString alloc] initWithBytes:titleBytes length:titleLen encoding:NSUTF8StringEncoding];
    NSString *errorText = [[NSString alloc] initWithBytes:errorBytes length:errorLen encoding:NSUTF8StringEncoding];
    if (!preview) {
        NSWindow *window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 800, 580)
            styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable
            backing:NSBackingStoreBuffered defer:NO];
        window.releasedWhenClosed = NO;
        window.minSize = NSMakeSize(400, 300);
        preview = [[SissyPreviewController alloc] initWithWindow:window];
        window.delegate = preview;
        [window center];
    }
    [preview clearMedia];
    preview.window.title = [NSString stringWithFormat:@"%@ — %@", title, url.lastPathComponent];
    NSView *content = [[NSView alloc] initWithFrame:NSMakeRect(0, 0,
        preview.window.contentView.bounds.size.width, preview.window.contentView.bounds.size.height)];
    preview.window.contentView = content;
    UTType *type = nil;
    [url getResourceValue:&type forKey:NSURLContentTypeKey error:nil];
    NSView *viewer = nil;
    if ([type conformsToType:UTTypeMovie] || [type conformsToType:UTTypeAudio]) {
        preview.player = [AVPlayer playerWithURL:url];
        AVPlayerView *playerView = [[AVPlayerView alloc] initWithFrame:content.bounds];
        playerView.player = preview.player;
        playerView.controlsStyle = AVPlayerViewControlsStyleInline;
        playerView.showsSharingServiceButton = NO;
        viewer = playerView;
        preview.observedItem = preview.player.currentItem;
    } else if ([type conformsToType:UTTypeImage]) {
        // Downsample large originals instead of decoding them at full resolution.
        CGImageSourceRef source = CGImageSourceCreateWithURL((__bridge CFURLRef)url, NULL);
        CGImageRef image = source ? CGImageSourceCreateThumbnailAtIndex(source, 0,
            (__bridge CFDictionaryRef)@{(id)kCGImageSourceCreateThumbnailFromImageAlways:@YES,
                (id)kCGImageSourceThumbnailMaxPixelSize:@4096,
                (id)kCGImageSourceCreateThumbnailWithTransform:@YES}) : NULL;
        if (source) CFRelease(source);
        if (image) {
            NSImageView *imageView = [[NSImageView alloc] initWithFrame:content.bounds];
            imageView.image = [[NSImage alloc] initWithCGImage:image size:NSZeroSize];
            imageView.imageScaling = NSImageScaleProportionallyUpOrDown;
            viewer = imageView;
            CGImageRelease(image);
        }
    } else {
        preview.quickLook = [[QLPreviewView alloc] initWithFrame:content.bounds style:QLPreviewViewStyleNormal];
        preview.quickLook.autostarts = NO;
        preview.quickLook.shouldCloseWithWindow = YES;
        preview.quickLook.previewItem = url;
        viewer = preview.quickLook;
    }
    if (viewer) {
        viewer.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
        [content addSubview:viewer];
    }
    preview.message = [NSTextField wrappingLabelWithString:errorText ?: @""];
    preview.message.frame = NSMakeRect(20, content.bounds.size.height - 85,
        content.bounds.size.width - 40, 65);
    preview.message.autoresizingMask = NSViewWidthSizable | NSViewMinYMargin;
    preview.message.hidden = viewer != nil;
    [content addSubview:preview.message];
    if (preview.observedItem) [preview.observedItem addObserver:preview forKeyPath:@"status"
        options:NSKeyValueObservingOptionInitial | NSKeyValueObservingOptionNew context:NULL];
    [preview showWindow:nil];
    [preview.window makeKeyAndOrderFront:nil];
    return 1;
}
