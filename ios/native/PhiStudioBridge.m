#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <AVFAudio/AVFAudio.h>
#import <objc/runtime.h>

extern void phistudio_ios_document(uint64_t token, const char *path,
                                   const char *name, const char *error, int canceled);
extern void phistudio_ios_input(int action, const char *text);
extern void phistudio_ios_suspend(void);

static __weak UIViewController *controller;
static CGRect keyboardFrame;
static NSArray<NSString *> *keyboardLabels;
static BOOL ps_hide_home_indicator(id object, SEL selector) {
    (void)object; (void)selector;
    return YES;
}

// This responder only opens after a deliberate egui text-field tap. It never
// claims focus at startup, after returning from Files, or after app activation.
@interface PSKeyboardView : UIView <UIKeyInput, UITextInputTraits>
@end
@implementation PSKeyboardView
- (BOOL)canBecomeFirstResponder { return YES; }
- (UIView *)hitTest:(CGPoint)point withEvent:(UIEvent *)event { return nil; }
- (BOOL)hasText { return YES; }
- (UIKeyboardType)keyboardType { return UIKeyboardTypeDefault; }
- (UIKeyboardAppearance)keyboardAppearance { return UIKeyboardAppearanceDark; }
- (UITextAutocorrectionType)autocorrectionType { return UITextAutocorrectionTypeNo; }
- (UITextAutocapitalizationType)autocapitalizationType { return UITextAutocapitalizationTypeNone; }
- (UITextSmartQuotesType)smartQuotesType { return UITextSmartQuotesTypeNo; }
- (UITextSmartDashesType)smartDashesType { return UITextSmartDashesTypeNo; }
- (UITextSmartInsertDeleteType)smartInsertDeleteType { return UITextSmartInsertDeleteTypeNo; }
- (void)insertText:(NSString *)text {
    if ([text isEqualToString:@"\n"]) phistudio_ios_input(2, NULL);
    else phistudio_ios_input(0, text.UTF8String);
}
- (void)deleteBackward { phistudio_ios_input(1, NULL); }
- (void)copy:(id)sender { phistudio_ios_input(3, NULL); }
- (void)cut:(id)sender { phistudio_ios_input(6, NULL); }
- (void)paste:(id)sender {
    NSString *text = UIPasteboard.generalPasteboard.string;
    if (text) phistudio_ios_input(4, text.UTF8String);
}
- (void)selectAll:(id)sender { phistudio_ios_input(5, NULL); }
- (void)done:(id)sender {
    phistudio_ios_input(2, NULL);
    [self resignFirstResponder];
}
- (UIView *)inputAccessoryView {
    UIToolbar *bar = [[UIToolbar alloc] initWithFrame:CGRectMake(0, 0, 500, 44)];
    bar.barStyle = UIBarStyleBlack;
    NSArray<NSString *> *labels = keyboardLabels ?: @[@"Select all", @"Copy", @"Paste", @"Done"];
    bar.items = @[
        [[UIBarButtonItem alloc] initWithTitle:labels[0] style:UIBarButtonItemStylePlain target:self action:@selector(selectAll:)],
        [[UIBarButtonItem alloc] initWithTitle:labels[1] style:UIBarButtonItemStylePlain target:self action:@selector(copy:)],
        [[UIBarButtonItem alloc] initWithTitle:labels[2] style:UIBarButtonItemStylePlain target:self action:@selector(paste:)],
        [[UIBarButtonItem alloc] initWithBarButtonSystemItem:UIBarButtonSystemItemFlexibleSpace target:nil action:nil],
        [[UIBarButtonItem alloc] initWithTitle:labels[3] style:UIBarButtonItemStyleDone target:self action:@selector(done:)],
    ];
    return bar;
}
@end
static PSKeyboardView *keyboard;

void ps_set_language(const char *selectAll, const char *copy, const char *paste, const char *done) {
    keyboardLabels = @[[NSString stringWithUTF8String:selectAll], [NSString stringWithUTF8String:copy],
                       [NSString stringWithUTF8String:paste], [NSString stringWithUTF8String:done]];
    if (keyboard.isFirstResponder) [keyboard reloadInputViews];
}

@interface PSServices : NSObject
@end
@implementation PSServices
- (void)keyboardChanged:(NSNotification *)notification {
    if ([notification.name isEqualToString:UIKeyboardWillHideNotification]) {
        keyboardFrame = CGRectZero;
    } else {
        keyboardFrame = [notification.userInfo[UIKeyboardFrameEndUserInfoKey] CGRectValue];
    }
}
- (void)suspend:(NSNotification *)notification {
    [keyboard resignFirstResponder];
    keyboardFrame = CGRectZero;
    phistudio_ios_suspend();
}
- (void)audioInterrupted:(NSNotification *)notification {
    if ([notification.userInfo[AVAudioSessionInterruptionTypeKey] unsignedIntegerValue] == AVAudioSessionInterruptionTypeBegan) {
        [self suspend:notification];
    }
}
- (void)activate:(NSNotification *)notification {
    [AVAudioSession.sharedInstance setActive:YES error:nil];
}
@end
static PSServices *services;

void ps_initialize(void *pointer) {
    controller = (__bridge UIViewController *)pointer;
    if (UIDevice.currentDevice.userInterfaceIdiom == UIUserInterfaceIdiomPad) {
        class_addMethod(controller.class, @selector(prefersHomeIndicatorAutoHidden), (IMP)ps_hide_home_indicator, "B@:");
        [controller setNeedsUpdateOfHomeIndicatorAutoHidden];
    }
    keyboard = [[PSKeyboardView alloc] initWithFrame:CGRectMake(0, 0, 1, 1)];
    keyboard.backgroundColor = UIColor.clearColor;
    [controller.view addSubview:keyboard];
    services = [PSServices new];
    NSNotificationCenter *center = NSNotificationCenter.defaultCenter;
    [center addObserver:services selector:@selector(keyboardChanged:) name:UIKeyboardWillChangeFrameNotification object:nil];
    [center addObserver:services selector:@selector(keyboardChanged:) name:UIKeyboardWillHideNotification object:nil];
    [center addObserver:services selector:@selector(suspend:) name:UIApplicationWillResignActiveNotification object:nil];
    [center addObserver:services selector:@selector(activate:) name:UIApplicationDidBecomeActiveNotification object:nil];
    [center addObserver:services selector:@selector(audioInterrupted:) name:AVAudioSessionInterruptionNotification object:nil];
    [AVAudioSession.sharedInstance setCategory:AVAudioSessionCategoryPlayback
                                        mode:AVAudioSessionModeDefault
                                     options:AVAudioSessionCategoryOptionMixWithOthers error:nil];
    [AVAudioSession.sharedInstance setActive:YES error:nil];
    UIApplication.sharedApplication.idleTimerDisabled = YES;
}

float ps_density(void) {
    return (float)(controller.view.window.screen ?: UIScreen.mainScreen).scale;
}
void ps_viewport_insets(float *out) {
    UIView *view = controller.view;
    UIEdgeInsets safe = view.safeAreaInsets;
    // Let the editor extend to the iPad's bottom edge. The home indicator hides
    // automatically; an actual docked keyboard still reserves its full height.
    if (UIDevice.currentDevice.userInterfaceIdiom == UIUserInterfaceIdiomPad) safe.bottom = 0;
    CGRect frame = [view convertRect:keyboardFrame fromView:nil];
    CGRect overlap = CGRectIntersection(view.bounds, frame);
    // A floating iPad keyboard must not collapse the whole editor viewport.
    if (!CGRectIsNull(overlap) && overlap.size.width >= view.bounds.size.width * 0.8
        && CGRectGetMaxY(overlap) >= CGRectGetMaxY(view.bounds) - 1) {
        safe.bottom = MAX(safe.bottom, overlap.size.height);
    }
    float scale = ps_density();
    out[0] = safe.left * scale;
    out[1] = safe.top * scale;
    out[2] = safe.right * scale;
    out[3] = safe.bottom * scale;
}
void ps_show_keyboard(int show) {
    if (show) [keyboard becomeFirstResponder];
    else [keyboard resignFirstResponder];
}
void ps_set_clipboard(const char *text) {
    UIPasteboard.generalPasteboard.string = [NSString stringWithUTF8String:text];
}

@interface PSDocumentDelegate : NSObject <UIDocumentPickerDelegate>
@property(nonatomic) uint64_t token;
@property(nonatomic) uint64_t limit;
@property(nonatomic) BOOL exporting;
@property(nonatomic, strong) NSURL *exportDirectory;
@end
static PSDocumentDelegate *document;

static void documentResult(PSDocumentDelegate *delegate, NSString *path, NSString *name,
                           NSString *error, BOOL canceled) {
    phistudio_ios_document(delegate.token, path.UTF8String, name.UTF8String, error.UTF8String, canceled);
    if (delegate.exportDirectory) {
        [NSFileManager.defaultManager removeItemAtURL:delegate.exportDirectory error:nil];
    }
    if (document == delegate) document = nil;
}

// Copy cloud/provider files on a worker, with the same bounded snapshots used
// on Android. No provider URL is retained by projects, drafts or resource skins.
static NSString *copySnapshot(NSURL *source, uint64_t limit, NSError **error) {
    NSFileManager *fm = NSFileManager.defaultManager;
    NSURL *directory = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:@"PhiStudio-imports"] isDirectory:YES];
    if (![fm createDirectoryAtURL:directory withIntermediateDirectories:YES attributes:nil error:error]) return nil;
    NSString *filename = [NSUUID.UUID.UUIDString stringByAppendingPathExtension:source.pathExtension];
    NSURL *target = [directory URLByAppendingPathComponent:filename];
    NSInputStream *input = [NSInputStream inputStreamWithURL:source];
    NSOutputStream *output = [NSOutputStream outputStreamWithURL:target append:NO];
    [input open]; [output open];
    uint8_t buffer[65536];
    uint64_t total = 0;
    BOOL ok = YES;
    while (ok) {
        NSInteger count = [input read:buffer maxLength:sizeof(buffer)];
        if (count < 0) { *error = input.streamError; ok = NO; break; }
        if (count == 0) break;
        total += (uint64_t)count;
        if (total > limit) {
            *error = [NSError errorWithDomain:@"PhiStudio" code:1 userInfo:@{NSLocalizedDescriptionKey:@"The selected file exceeds the import size limit."}];
            ok = NO; break;
        }
        NSInteger offset = 0;
        while (offset < count) {
            NSInteger written = [output write:buffer + offset maxLength:(NSUInteger)(count - offset)];
            if (written <= 0) { *error = output.streamError; ok = NO; break; }
            offset += written;
        }
    }
    [input close]; [output close];
    if (!ok) {
        [fm removeItemAtURL:target error:nil];
        if (!*error) *error = [NSError errorWithDomain:@"PhiStudio" code:2 userInfo:@{NSLocalizedDescriptionKey:@"Could not copy the selected file."}];
        return nil;
    }
    return target.path;
}

@implementation PSDocumentDelegate
- (void)documentPickerWasCancelled:(UIDocumentPickerViewController *)picker {
    documentResult(self, nil, nil, nil, YES);
}
- (void)documentPicker:(UIDocumentPickerViewController *)picker didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
    if (self.exporting) { documentResult(self, nil, nil, nil, NO); return; }
    NSURL *url = urls.firstObject;
    if (!url) { documentResult(self, nil, nil, @"No file was selected.", NO); return; }
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        BOOL scoped = [url startAccessingSecurityScopedResource];
        __block NSString *path = nil;
        __block NSError *copyError = nil;
        NSError *coordinateError = nil;
        NSFileCoordinator *coordinator = [[NSFileCoordinator alloc] initWithFilePresenter:nil];
        [coordinator coordinateReadingItemAtURL:url options:0 error:&coordinateError byAccessor:^(NSURL *readURL) {
            path = copySnapshot(readURL, self.limit, &copyError);
        }];
        if (scoped) [url stopAccessingSecurityScopedResource];
        NSString *error = (coordinateError ?: copyError).localizedDescription;
        if (!path && !error) error = @"Could not read the selected file.";
        dispatch_async(dispatch_get_main_queue(), ^{
            documentResult(self, path, url.lastPathComponent, error, NO);
        });
    });
}
@end

static BOOL pickerReady(uint64_t token) {
    if (!controller || controller.presentedViewController || document) {
        phistudio_ios_document(token, NULL, NULL, "Close the current system dialog before choosing a file.", 0);
        return NO;
    }
    [keyboard resignFirstResponder];
    return YES;
}
void ps_pick_document(const char *kindValue, uint64_t token) {
    if (!pickerReady(token)) return;
    NSString *kind = [NSString stringWithUTF8String:kindValue];
    document = [PSDocumentDelegate new];
    document.token = token;
    document.limit = 256ULL * 1024 * 1024;
    NSArray<UTType *> *types;
    if ([kind isEqualToString:@"shader"]) {
        types = @[UTTypeData];
        document.limit = 1024 * 1024;
    } else if ([kind isEqualToString:@"resource"]) {
        types = @[UTTypeData];
        document.limit = 128ULL * 1024 * 1024;
    } else if (([kind isEqualToString:@"illustration"] || [kind isEqualToString:@"line-texture"])) {
        types = @[UTTypePNG, UTTypeJPEG, UTTypeWebP, UTTypeBMP, UTTypeGIF];
        document.limit = 32ULL * 1024 * 1024;
    } else if ([kind isEqualToString:@"audio"] || [kind isEqualToString:@"hitsound"]) {
        // OGG providers do not consistently advertise public.audio.
        types = @[UTTypeAudio, UTTypeData];
        document.limit = ([kind isEqualToString:@"hitsound"] ? 8ULL : 128ULL) * 1024 * 1024;
    } else {
        // PEZ and resource-pack ZIP files may have provider-specific UTIs.
        types = @[UTTypeZIP, UTTypeData];
    }
    UIDocumentPickerViewController *picker = [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:types asCopy:YES];
    picker.delegate = document;
    picker.allowsMultipleSelection = NO;
    [controller presentViewController:picker animated:YES completion:nil];
}
void ps_export_document(const char *pathValue, const char *nameValue, uint64_t token) {
    if (!pickerReady(token)) return;
    NSString *path = [NSString stringWithUTF8String:pathValue];
    NSString *name = [[NSString stringWithUTF8String:nameValue] lastPathComponent];
    PSDocumentDelegate *delegate = [PSDocumentDelegate new];
    delegate.token = token;
    delegate.exporting = YES;
    delegate.exportDirectory = [NSURL fileURLWithPath:[NSTemporaryDirectory() stringByAppendingPathComponent:NSUUID.UUID.UUIDString] isDirectory:YES];
    document = delegate;
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSError *error = nil;
        NSURL *target = [delegate.exportDirectory URLByAppendingPathComponent:name];
        NSFileManager *fm = NSFileManager.defaultManager;
        BOOL ok = [fm createDirectoryAtURL:delegate.exportDirectory withIntermediateDirectories:YES attributes:nil error:&error]
            && [fm copyItemAtURL:[NSURL fileURLWithPath:path] toURL:target error:&error];
        dispatch_async(dispatch_get_main_queue(), ^{
            if (!ok) { documentResult(delegate, nil, nil, error.localizedDescription ?: @"Could not prepare export.", NO); return; }
            UIDocumentPickerViewController *picker = [[UIDocumentPickerViewController alloc] initForExportingURLs:@[target] asCopy:YES];
            picker.delegate = delegate;
            [controller presentViewController:picker animated:YES completion:nil];
        });
    });
}
