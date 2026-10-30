#import <AVFoundation/AVFoundation.h>
#import <stdatomic.h>
#import <stdbool.h>
#import <stdio.h>
#import <unistd.h>

static atomic_bool movieCanceled;
static _Thread_local char movieError[2048];
static int fail(NSString *message) { snprintf(movieError,sizeof(movieError),"%s",message.UTF8String ?: "Video encoder failed"); return 0; }
const char *ps_video_error(void) { return movieError; }
void ps_video_reset(void) { atomic_store(&movieCanceled,false); }
void ps_video_cancel(void) { atomic_store(&movieCanceled,true); }
// The existing UIKit bridge already keeps the application awake.
void ps_video_release(void) { }

@interface PSMovie : NSObject {
@public
    AVAssetWriter *writer;
    AVAssetWriterInput *video, *audio;
    AVAssetWriterInputPixelBufferAdaptor *pixels;
    CMAudioFormatDescriptionRef audioFormat;
    FILE *wav;
    int width, height, fps;
    uint64_t frames, audioPosition, audioFrames;
    BOOL done;
    atomic_bool stopping;
    dispatch_queue_t audioQueue;
    dispatch_semaphore_t audioFinished;
    NSString *audioError;
}
- (BOOL)ready:(AVAssetWriterInput *)input;
- (BOOL)audioUntil:(uint64_t)limit;
- (void)startAudio;
@end
@implementation PSMovie
- (BOOL)ready:(AVAssetWriterInput *)input {
    double deadline=NSDate.timeIntervalSinceReferenceDate+30.;
    while(!input.readyForMoreMediaData) {
        if(atomic_load(&movieCanceled) || atomic_load(&stopping)) return fail(@"Video export canceled");
        @synchronized(self) { if(audioError)return fail(audioError); }
        if(writer.status!=AVAssetWriterStatusWriting) return fail(writer.error.localizedDescription ?: @"Video encoder stopped");
        if(NSDate.timeIntervalSinceReferenceDate>deadline) return fail([NSString stringWithFormat:@"Video encoder timed out waiting for %@ input",input.mediaType]);
        usleep(1000);
    }
    if(atomic_load(&movieCanceled))return fail(@"Video export canceled");
    return YES;
}
- (BOOL)audioUntil:(uint64_t)limit {
    if(!audio)return YES;
    limit=MIN(limit,audioFrames);
    while(audioPosition<limit) {
        if(![self ready:audio])return NO;
        size_t count=(size_t)MIN((uint64_t)2048,limit-audioPosition), length=count*4;
        CMBlockBufferRef block=NULL;
        OSStatus result=CMBlockBufferCreateWithMemoryBlock(kCFAllocatorDefault,NULL,length,kCFAllocatorDefault,NULL,0,length,0,&block);
        if(result!=noErr)return fail(@"Could not allocate video audio buffer");
        uint8_t pcm[8192];
        if(fread(pcm,1,length,wav)!=length){CFRelease(block);return fail(@"Could not read movie audio");}
        result=CMBlockBufferReplaceDataBytes(pcm,block,0,length);
        CMSampleTimingInfo timing={CMTimeMake(1,48000),CMTimeMake((int64_t)audioPosition,48000),kCMTimeInvalid};
        size_t sampleSize=4;CMSampleBufferRef sample=NULL;
        if(result==noErr) result=CMSampleBufferCreateReady(kCFAllocatorDefault,block,audioFormat,(CMItemCount)count,1,&timing,1,&sampleSize,&sample);
        CFRelease(block);
        if(result!=noErr)return fail(@"Could not prepare movie audio samples");
        BOOL ok=[audio appendSampleBuffer:sample];CFRelease(sample);
        if(!ok)return fail(writer.error.localizedDescription ?: @"Could not encode movie audio");
        audioPosition+=count;
    }
    return YES;
}
- (void)startAudio {
    if(!audio)return;
    audioQueue=dispatch_queue_create("com.phistudio.movie-audio",DISPATCH_QUEUE_SERIAL);
    audioFinished=dispatch_semaphore_create(0);
    __weak PSMovie *weakSelf=self;
    [audio requestMediaDataWhenReadyOnQueue:audioQueue usingBlock:^{
        @autoreleasepool {
            PSMovie *m=weakSelf;if(!m)return;
            @try {
                while(m->audio.readyForMoreMediaData && m->audioPosition<m->audioFrames
                        && !atomic_load(&movieCanceled) && !atomic_load(&m->stopping)) {
                    // This callback is the only owner of the PCM stream/audio input.
                    if(![m audioUntil:MIN(m->audioPosition+2048,m->audioFrames)]) {
                        @synchronized(m) {m->audioError=[NSString stringWithUTF8String:movieError];}
                        break;
                    }
                }
                BOOL failed;@synchronized(m){failed=m->audioError!=nil;}
                if(m->audioPosition==m->audioFrames || failed || atomic_load(&movieCanceled) || atomic_load(&m->stopping)) {
                    [m->audio markAsFinished];
                    dispatch_semaphore_signal(m->audioFinished);
                }
            } @catch(NSException *exception) {
                @synchronized(m){m->audioError=exception.reason;}
                dispatch_semaphore_signal(m->audioFinished);
            }
        }
    }];
}
- (void)dealloc {
    if(!done)[writer cancelWriting];
    if(wav)fclose(wav);
    if(audioFormat)CFRelease(audioFormat);
}
@end

void *ps_video_create(const char *json) {
    @autoreleasepool { @try {
        if(atomic_load(&movieCanceled)){fail(@"Video export canceled");return NULL;}
        NSError *error=nil;
        NSDictionary *c=[NSJSONSerialization JSONObjectWithData:[[NSString stringWithUTF8String:json] dataUsingEncoding:NSUTF8StringEncoding] options:0 error:&error];
        if(!c){fail(error.localizedDescription);return NULL;}
        PSMovie *m=[PSMovie new];m->width=[c[@"width"] intValue];m->height=[c[@"height"] intValue];m->fps=[c[@"fps"] intValue];m->frames=[c[@"frames"] unsignedLongLongValue];
        m->writer=[[AVAssetWriter alloc] initWithURL:[NSURL fileURLWithPath:c[@"path"]] fileType:AVFileTypeMPEG4 error:&error];
        if(!m->writer){fail(error.localizedDescription);return NULL;}
        m->writer.shouldOptimizeForNetworkUse=YES;
        NSDictionary *videoSettings=@{AVVideoCodecKey:AVVideoCodecTypeH264,AVVideoWidthKey:@(m->width),AVVideoHeightKey:@(m->height),AVVideoCompressionPropertiesKey:@{AVVideoAverageBitRateKey:c[@"bitrate"],AVVideoExpectedSourceFrameRateKey:@(m->fps),AVVideoMaxKeyFrameIntervalKey:@(m->fps*2),AVVideoAllowFrameReorderingKey:@NO}};
        if(![m->writer canApplyOutputSettings:videoSettings forMediaType:AVMediaTypeVideo]){fail(@"This device cannot encode the selected video resolution and FPS");return NULL;}
        m->video=[AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo outputSettings:videoSettings];
        m->video.expectsMediaDataInRealTime=NO;
        m->pixels=[AVAssetWriterInputPixelBufferAdaptor assetWriterInputPixelBufferAdaptorWithAssetWriterInput:m->video sourcePixelBufferAttributes:@{(NSString *)kCVPixelBufferPixelFormatTypeKey:@(kCVPixelFormatType_32BGRA),(NSString *)kCVPixelBufferWidthKey:@(m->width),(NSString *)kCVPixelBufferHeightKey:@(m->height),(NSString *)kCVPixelBufferIOSurfacePropertiesKey:@{}}];
        if(![m->writer canAddInput:m->video]){fail(@"Could not create video encoder input");return NULL;}[m->writer addInput:m->video];
        if(c[@"wav"] && c[@"wav"]!=NSNull.null) {
            m->wav=fopen([c[@"wav"] fileSystemRepresentation],"rb");
            if(!m->wav || fseek(m->wav,44,SEEK_SET)!=0){fail(@"Could not open movie audio");return NULL;}
            AudioStreamBasicDescription asbd={0};asbd.mSampleRate=48000;asbd.mFormatID=kAudioFormatLinearPCM;asbd.mFormatFlags=kLinearPCMFormatFlagIsSignedInteger|kAudioFormatFlagIsPacked;asbd.mBytesPerPacket=4;asbd.mFramesPerPacket=1;asbd.mBytesPerFrame=4;asbd.mChannelsPerFrame=2;asbd.mBitsPerChannel=16;
            if(CMAudioFormatDescriptionCreate(kCFAllocatorDefault,&asbd,0,NULL,0,NULL,NULL,&m->audioFormat)!=noErr){fail(@"Could not configure movie audio");return NULL;}
            m->audio=[AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeAudio outputSettings:@{AVFormatIDKey:@(kAudioFormatMPEG4AAC),AVSampleRateKey:@48000,AVNumberOfChannelsKey:@2,AVEncoderBitRateKey:c[@"audioBitrate"]} sourceFormatHint:m->audioFormat];
            m->audio.expectsMediaDataInRealTime=NO;
            if(![m->writer canAddInput:m->audio]){fail(@"Could not create audio encoder input");return NULL;}[m->writer addInput:m->audio];
            m->audioFrames=(m->frames*48000+m->fps-1)/m->fps;
        }
        if(![m->writer startWriting]){fail(m->writer.error.localizedDescription);return NULL;}
        [m->writer startSessionAtSourceTime:kCMTimeZero];
        [m startAudio];
        return (__bridge_retained void *)m;
    } @catch(NSException *exception){fail(exception.reason);return NULL;} }
}
int ps_video_frame(void *handle,const uint8_t *rgba,size_t length,uint64_t index) {
    @autoreleasepool { @try {
        PSMovie *m=(__bridge PSMovie *)handle;
        if(length!=(size_t)m->width*m->height*4)return fail(@"Invalid video frame");
        if(![m ready:m->video])return 0;
        CVPixelBufferRef pixel=NULL;
        if(CVPixelBufferPoolCreatePixelBuffer(kCFAllocatorDefault,m->pixels.pixelBufferPool,&pixel)!=kCVReturnSuccess)return fail(@"Could not allocate movie pixel buffer");
        CVPixelBufferLockBaseAddress(pixel,0);uint8_t *out=CVPixelBufferGetBaseAddress(pixel);size_t stride=CVPixelBufferGetBytesPerRow(pixel);
        for(int y=0;y<m->height;y++)for(int x=0;x<m->width;x++) {
            const uint8_t *s=rgba+((size_t)y*m->width+x)*4;uint8_t *d=out+(size_t)y*stride+x*4;
            d[0]=s[2];d[1]=s[1];d[2]=s[0];d[3]=255;
        }
        CVPixelBufferUnlockBaseAddress(pixel,0);
        BOOL ok=[m->pixels appendPixelBuffer:pixel withPresentationTime:CMTimeMake((int64_t)index,m->fps)];CVPixelBufferRelease(pixel);
        return ok ? 1 : fail(m->writer.error.localizedDescription ?: @"Could not encode movie frame");
    } @catch(NSException *exception){return fail(exception.reason);} }
}
int ps_video_finish(void *handle) {
    @autoreleasepool { @try {
        PSMovie *m=(__bridge PSMovie *)handle;
        [m->video markAsFinished];
        if(m->audio) {
            double audioDeadline=NSDate.timeIntervalSinceReferenceDate+60.;
            while(dispatch_semaphore_wait(m->audioFinished,dispatch_time(DISPATCH_TIME_NOW,100*NSEC_PER_MSEC))!=0) {
                if(atomic_load(&movieCanceled) || NSDate.timeIntervalSinceReferenceDate>audioDeadline)return fail(@"Video audio canceled or timed out");
                if(m->writer.status!=AVAssetWriterStatusWriting)return fail(m->writer.error.localizedDescription ?: @"Video encoder stopped");
            }
            @synchronized(m){if(m->audioError)return fail(m->audioError);}
        }
        [m->writer endSessionAtSourceTime:CMTimeMake((int64_t)m->frames,m->fps)];
        dispatch_semaphore_t finished=dispatch_semaphore_create(0);
        [m->writer finishWritingWithCompletionHandler:^{dispatch_semaphore_signal(finished);}];
        double deadline=NSDate.timeIntervalSinceReferenceDate+60.;
        while(dispatch_semaphore_wait(finished,dispatch_time(DISPATCH_TIME_NOW,100*NSEC_PER_MSEC))!=0) {
            if(atomic_load(&movieCanceled) || NSDate.timeIntervalSinceReferenceDate>deadline){[m->writer cancelWriting];return fail(@"Video export canceled or timed out");}
        }
        if(m->writer.status!=AVAssetWriterStatusCompleted)return fail(m->writer.error.localizedDescription ?: @"Video encoder failed");
        m->done=YES;return 1;
    } @catch(NSException *exception){return fail(exception.reason);} }
}
void ps_video_close(void *handle) {
    @autoreleasepool {
        PSMovie *movie=CFBridgingRelease(handle);
        atomic_store(&movie->stopping,true);
        if(!movie->done)[movie->writer cancelWriting];
        if(movie->audioQueue)dispatch_sync(movie->audioQueue, ^{});
    }
}
