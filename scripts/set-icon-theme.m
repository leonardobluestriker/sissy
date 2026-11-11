#import <objc/message.h>
#import <objc/runtime.h>
#import <dlfcn.h>
#import <stdio.h>
#import <stdlib.h>
#import <unistd.h>
int main(int argc, char **argv) {
    unsigned theme = argc > 1 ? (unsigned)atoi(argv[1]) : 1;
    dlopen("/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight", RTLD_LAZY);
    Class config = objc_getClass("SLSIconAppearanceConfiguration");
    id obj = ((id (*)(Class, SEL))objc_msgSend)(
        config, sel_registerName("fetchCurrentIconAppearanceConfiguration"));
    ((void (*)(id, SEL, unsigned))objc_msgSend)(obj, sel_registerName("setIconAppearanceTheme:"), theme);
    ((void (*)(id, SEL))objc_msgSend)(obj, sel_registerName("save"));
    usleep(300000);
    id now = ((id (*)(Class, SEL))objc_msgSend)(
        config, sel_registerName("fetchCurrentIconAppearanceConfiguration"));
    printf("%u\n", ((unsigned (*)(id, SEL))objc_msgSend)(now, sel_registerName("iconAppearanceTheme")));
    return 0;
}
