#import <AppKit/AppKit.h>
#import <objc/message.h>
#import <objc/runtime.h>
#import <dlfcn.h>
#import <math.h>
#import <stdint.h>

// Plates exported from Icon Composer, in this order:
// 0 default, 1 dark, 2 clear light, 3 clear dark, 4 tinted light, 5 tinted dark.
static const uint8_t *blobs[6];
static size_t blob_lens[6];
static int applied_kind = -1;
static float applied_accent[3];

void sissy_set_icon_blob(int kind, const uint8_t *bytes, size_t len) {
    if (kind < 0 || kind > 5) {
        return;
    }
    blobs[kind] = bytes;
    blob_lens[kind] = len;
}

static unsigned theme_value(id obj, const char *name) {
    return ((unsigned (*)(id, SEL))objc_msgSend)(obj, sel_registerName(name));
}

static int current_kind(void) {
    dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY);
    Class config = objc_getClass("SLSIconAppearanceConfiguration");
    if (!config) {
        return 0;
    }
    id obj = ((id (*)(Class, SEL))objc_msgSend)(
        config, sel_registerName("fetchCurrentIconAppearanceConfiguration"));
    if (!obj) {
        return 0;
    }
    unsigned theme = theme_value(obj, "iconAppearanceTheme");
    int dark = theme_value(obj, "appearanceTheme") != 0;
    int kind = 0;
    switch (theme) {
    case 0:
        kind = dark ? 1 : 0;
        break;
    case 2:
        kind = 1;
        break;
    case 3:
        kind = dark ? 3 : 2;
        break;
    case 4:
        kind = 2;
        break;
    case 5:
        kind = 3;
        break;
    case 6:
        kind = dark ? 5 : 4;
        break;
    case 7:
        kind = 4;
        break;
    case 8:
        kind = 5;
        break;
    default:
        kind = 0;
        break;
    }
    return kind;
}

static void rgb_to_hsv(float r, float g, float b, float *h, float *s, float *v) {
    float max = fmaxf(r, fmaxf(g, b));
    float min = fminf(r, fminf(g, b));
    float d = max - min;
    *v = max;
    *s = max <= 0.0001f ? 0.f : d / max;
    if (d <= 0.0001f) {
        *h = 0.f;
        return;
    }
    if (max == r) {
        *h = fmodf((g - b) / d, 6.f);
    } else if (max == g) {
        *h = (b - r) / d + 2.f;
    } else {
        *h = (r - g) / d + 4.f;
    }
    *h /= 6.f;
    if (*h < 0.f) {
        *h += 1.f;
    }
}

static void hsv_to_rgb(float h, float s, float v, float *r, float *g, float *b) {
    float hh = h * 6.f;
    int sector = (int)hh;
    float f = hh - (float)sector;
    float p = v * (1.f - s);
    float q = v * (1.f - s * f);
    float t = v * (1.f - s * (1.f - f));
    switch (sector % 6) {
    case 0: *r = v; *g = t; *b = p; break;
    case 1: *r = q; *g = v; *b = p; break;
    case 2: *r = p; *g = v; *b = t; break;
    case 3: *r = p; *g = q; *b = v; break;
    case 4: *r = t; *g = p; *b = v; break;
    default: *r = v; *g = p; *b = q; break;
    }
}

static NSBitmapImageRep *eightBitRep(NSBitmapImageRep *rep) {
    if (rep.bitsPerSample == 8 && rep.samplesPerPixel == 4 &&
        (rep.bitmapFormat & (NSBitmapFormatFloatingPointSamples | NSBitmapFormatAlphaFirst)) == 0) {
        return rep;
    }
    NSInteger width = rep.pixelsWide;
    NSInteger height = rep.pixelsHigh;
    NSBitmapImageRep *out = [[NSBitmapImageRep alloc]
        initWithBitmapDataPlanes:NULL
                          pixelsWide:width
                          pixelsHigh:height
                       bitsPerSample:8
                     samplesPerPixel:4
                            hasAlpha:YES
                            isPlanar:NO
                      colorSpaceName:NSCalibratedRGBColorSpace
                         bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
                          bytesPerRow:width * 4
                         bitsPerPixel:32];
    if (rep.bitsPerSample == 16 && rep.samplesPerPixel == 4 && rep.bitmapData != NULL &&
        out.bitmapData != NULL) {
        BOOL alphaFirst = (rep.bitmapFormat & NSBitmapFormatAlphaFirst) != 0;
        BOOL premultiplied = (rep.bitmapFormat & NSBitmapFormatAlphaNonpremultiplied) == 0;
        for (NSInteger y = 0; y < height; y++) {
            const uint16_t *src = (const uint16_t *)(rep.bitmapData + y * rep.bytesPerRow);
            unsigned char *dst = out.bitmapData + y * out.bytesPerRow;
            for (NSInteger x = 0; x < width; x++) {
                const uint16_t *s = src + x * 4;
                uint16_t a = alphaFirst ? s[0] : s[3];
                uint16_t channels[3] = {alphaFirst ? s[1] : s[0], alphaFirst ? s[2] : s[1],
                                        alphaFirst ? s[3] : s[2]};
                if (premultiplied && a > 0 && a < 65535) {
                    for (int c = 0; c < 3; c++) {
                        uint32_t scaled = (uint32_t)channels[c] * 65535u / a;
                        channels[c] = scaled > 65535u ? 65535u : (uint16_t)scaled;
                    }
                }
                dst[x * 4 + 0] = (unsigned char)(channels[0] >> 8);
                dst[x * 4 + 1] = (unsigned char)(channels[1] >> 8);
                dst[x * 4 + 2] = (unsigned char)(channels[2] >> 8);
                dst[x * 4 + 3] = (unsigned char)(a >> 8);
            }
        }
        return out;
    }
    return rep;
}

// Icon Composer exports the tinted plate in a fixed purple. The system accent
// (green, blue, …) has to replace that hue, keeping the glass highlights.
static NSImage *imageByRecoloring(NSData *data, NSColor *accent, BOOL lightPlate) {
    NSBitmapImageRep *source = [[NSBitmapImageRep alloc] initWithData:data];
    NSBitmapImageRep *rep = source == nil ? nil : eightBitRep(source);
    if (rep == nil || rep.bitmapData == NULL) {
        return [[NSImage alloc] initWithData:data];
    }
    NSColor *resolved = [accent colorUsingColorSpace:[NSColorSpace sRGBColorSpace]];
    if (resolved == nil) {
        return [[NSImage alloc] initWithData:data];
    }
    CGFloat ah = 0, as = 0, av = 0, aa = 0;
    [resolved getHue:&ah saturation:&as brightness:&av alpha:&aa];
    unsigned char *bytes = rep.bitmapData;
    BOOL premultiplied = (rep.bitmapFormat & NSBitmapFormatAlphaNonpremultiplied) == 0;
    for (NSInteger y = 0; y < rep.pixelsHigh; y++) {
        unsigned char *row = bytes + y * rep.bytesPerRow;
        for (NSInteger x = 0; x < rep.pixelsWide; x++) {
            unsigned char *p = row + x * 4;
            float alpha = p[3] / 255.f;
            if (alpha < 0.02f) {
                continue;
            }
            float scale = premultiplied && alpha > 0.f ? 1.f / alpha : 1.f;
            float r = fminf(p[0] / 255.f * scale, 1.f);
            float g = fminf(p[1] / 255.f * scale, 1.f);
            float b = fminf(p[2] / 255.f * scale, 1.f);
            float h, s, v;
            rgb_to_hsv(r, g, b, &h, &s, &v);
            if (lightPlate) {
                if (v > 0.84f) {
                    r = g = b = v;
                } else {
                    float shade = 0.40f + v * 0.38f;
                    hsv_to_rgb((float)ah, fmaxf((float)as, 0.55f), shade, &r, &g, &b);
                }
            } else if (s < 0.08f) {
                continue;
            } else {
                hsv_to_rgb((float)ah, s, v, &r, &g, &b);
            }
            if (premultiplied) {
                r *= alpha;
                g *= alpha;
                b *= alpha;
            }
            p[0] = (unsigned char)lrintf(fminf(r, 1.f) * 255.f);
            p[1] = (unsigned char)lrintf(fminf(g, 1.f) * 255.f);
            p[2] = (unsigned char)lrintf(fminf(b, 1.f) * 255.f);
        }
    }
    NSImage *image = [[NSImage alloc] initWithSize:NSMakeSize(rep.pixelsWide, rep.pixelsHigh)];
    [image addRepresentation:rep];
    return image;
}

static NSColor *systemAccent(void) {
    return [NSColor controlAccentColor];
}

static void apply_kind(int kind) {
    if (kind < 0 || kind > 5 || blobs[kind] == NULL || blob_lens[kind] == 0) {
        kind = 0;
    }
    if (blobs[kind] == NULL) {
        return;
    }
    BOOL tinted = kind == 4 || kind == 5;
    NSColor *accent = tinted ? systemAccent() : nil;
    NSColor *resolved = [accent colorUsingColorSpace:[NSColorSpace sRGBColorSpace]];
    float ar = resolved ? resolved.redComponent : 0.f;
    float ag = resolved ? resolved.greenComponent : 0.f;
    float ab = resolved ? resolved.blueComponent : 0.f;
    if (kind == applied_kind && fabsf(ar - applied_accent[0]) < 0.015f &&
        fabsf(ag - applied_accent[1]) < 0.015f && fabsf(ab - applied_accent[2]) < 0.015f) {
        return;
    }
    NSData *data = [NSData dataWithBytes:blobs[kind] length:blob_lens[kind]];
    NSImage *image = tinted ? imageByRecoloring(data, accent, kind == 4) : [[NSImage alloc] initWithData:data];
    if (image == nil) {
        return;
    }
    [NSApp setApplicationIconImage:image];
    applied_kind = kind;
    applied_accent[0] = ar;
    applied_accent[1] = ag;
    applied_accent[2] = ab;
}

void sissy_apply_icon_appearance(void) { apply_kind(current_kind()); }

static void appearance_changed(NSNotification *note) {
    (void)note;
    applied_kind = -1;
    apply_kind(current_kind());
}

void sissy_start_icon_observer(void) {
    static int started = 0;
    if (started) {
        return;
    }
    started = 1;
    dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY);
    NSString *name = nil;
    void *sym = dlsym(RTLD_DEFAULT, "kSLSCoordinatedIconAppearanceConfigurationChangeNotificationName");
    if (sym != NULL) {
        name = *(__unsafe_unretained NSString **)sym;
    }
    void (^handler)(NSNotification *) = ^(NSNotification *note) {
        appearance_changed(note);
    };
    NSOperationQueue *main = [NSOperationQueue mainQueue];
    if (name != nil) {
        [[NSDistributedNotificationCenter defaultCenter] addObserverForName:name
                                                                      object:nil
                                                                       queue:main
                                                                  usingBlock:handler];
        [[NSNotificationCenter defaultCenter] addObserverForName:name
                                                           object:nil
                                                            queue:main
                                                       usingBlock:handler];
    }
    [[NSDistributedNotificationCenter defaultCenter]
        addObserverForName:@"AppleInterfaceThemeChangedNotification"
                    object:nil
                     queue:main
                usingBlock:handler];
}
