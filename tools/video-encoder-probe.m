// Execute the same AVFoundation encoder on the macOS CI host. Device UI is separate.
#import <AVFoundation/AVFoundation.h>
#import <stdio.h>
#import <string.h>
#import <math.h>
extern void ps_video_reset(void),ps_video_cancel(void),ps_video_close(void *);
extern void *ps_video_create(const char *);
extern int ps_video_frame(void *,const uint8_t *,size_t,uint64_t),ps_video_finish(void *);
extern const char *ps_video_error(void);
#define CHECK(x) do { if(!(x)){fprintf(stderr,"FAIL line %d: %s\n",__LINE__,ps_video_error());return 1;} }while(0)
int main(int argc,const char **argv){ @autoreleasepool {
    CHECK(argc==2);
    NSString *root=[NSString stringWithUTF8String:argv[1]];
    NSString *out=[root stringByAppendingPathComponent:@"native-avfoundation.mp4"];
    NSString *wav=[root stringByAppendingPathComponent:@"native-audio.wav"];
    NSMutableData *pcm=[NSMutableData dataWithLength:44+96000*4];uint8_t *h=pcm.mutableBytes;
    memcpy(h,"RIFF",4);uint32_t v=96000*4+36;memcpy(h+4,&v,4);memcpy(h+8,"WAVEfmt ",8);v=16;memcpy(h+16,&v,4);uint16_t s=1;memcpy(h+20,&s,2);s=2;memcpy(h+22,&s,2);v=48000;memcpy(h+24,&v,4);v=192000;memcpy(h+28,&v,4);s=4;memcpy(h+32,&s,2);s=16;memcpy(h+34,&s,2);memcpy(h+36,"data",4);v=96000*4;memcpy(h+40,&v,4);
    int16_t *samples=(int16_t *)(h+44);for(int i=0;i<96000;i++){samples[i*2]=samples[i*2+1]=(int16_t)(sin(i*2.*M_PI*440./48000.)*8000.);}
    CHECK([pcm writeToFile:wav atomically:YES]);
    [[NSFileManager defaultManager] removeItemAtPath:out error:nil];
    NSDictionary *config=@{@"path":out,@"wav":wav,@"width":@320,@"height":@180,@"fps":@30,@"frames":@60,@"bitrate":@2000000,@"audioBitrate":@192000};
    NSString *jsonText=[[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:config options:0 error:nil] encoding:NSUTF8StringEncoding];
    const char *json=jsonText.UTF8String;
    ps_video_reset();void *encoder=ps_video_create(json);CHECK(encoder);
    uint8_t *pixels=malloc(320*180*4);
    for(int frame=0;frame<60;frame++){
        for(int y=0;y<180;y++)for(int x=0;x<320;x++){size_t at=(y*320+x)*4;pixels[at]=y<90?230:20;pixels[at+1]=30;pixels[at+2]=y<90?20:230;pixels[at+3]=255;}
        CHECK(ps_video_frame(encoder,pixels,320*180*4,(uint64_t)frame));
    }
    CHECK(ps_video_finish(encoder));ps_video_close(encoder);
    AVURLAsset *asset=[AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:out] options:nil];
    CHECK(fabs(CMTimeGetSeconds(asset.duration)-2.)<0.05);
    CHECK([asset tracksWithMediaType:AVMediaTypeVideo].count==1);CHECK([asset tracksWithMediaType:AVMediaTypeAudio].count==1);
    NSError *error=nil;AVAssetReader *reader=[AVAssetReader assetReaderWithAsset:asset error:&error];
    AVAssetReaderTrackOutput *video=[AVAssetReaderTrackOutput assetReaderTrackOutputWithTrack:[asset tracksWithMediaType:AVMediaTypeVideo][0] outputSettings:@{(NSString *)kCVPixelBufferPixelFormatTypeKey:@(kCVPixelFormatType_32BGRA)}];
    [reader addOutput:video];CHECK([reader startReading]);int frames=0;CMSampleBufferRef sample;
    while((sample=[video copyNextSampleBuffer])){
        CHECK(fabs(CMTimeGetSeconds(CMSampleBufferGetPresentationTimeStamp(sample))-frames/30.)<0.00001);
        CVPixelBufferRef pixel=CMSampleBufferGetImageBuffer(sample);CVPixelBufferLockBaseAddress(pixel,kCVPixelBufferLock_ReadOnly);
        uint8_t *data=CVPixelBufferGetBaseAddress(pixel);size_t stride=CVPixelBufferGetBytesPerRow(pixel);
        uint8_t *top=data+10*stride+10*4,*bottom=data+170*stride+10*4;
        CHECK(top[2]>190 && top[0]<60 && bottom[0]>190 && bottom[2]<60);
        CVPixelBufferUnlockBaseAddress(pixel,kCVPixelBufferLock_ReadOnly);CFRelease(sample);frames++;
    }
    CHECK(frames==60 && reader.status==AVAssetReaderStatusCompleted);
    reader=[AVAssetReader assetReaderWithAsset:asset error:&error];
    AVAssetReaderTrackOutput *audio=[AVAssetReaderTrackOutput assetReaderTrackOutputWithTrack:[asset tracksWithMediaType:AVMediaTypeAudio][0] outputSettings:@{AVFormatIDKey:@(kAudioFormatLinearPCM),AVLinearPCMBitDepthKey:@16,AVLinearPCMIsFloatKey:@NO,AVLinearPCMIsNonInterleaved:@NO}];
    [reader addOutput:audio];CHECK([reader startReading]);uint64_t count=0;double sum=0;
    while((sample=[audio copyNextSampleBuffer])){
        CMBlockBufferRef block=CMSampleBufferGetDataBuffer(sample);size_t length=CMBlockBufferGetDataLength(block);int16_t *data=malloc(length);CMBlockBufferCopyDataBytes(block,0,length,data);
        for(size_t i=0;i<length/2;i++){double v=data[i]/32768.;sum+=v*v;count++;}free(data);CFRelease(sample);
    }
    CHECK(count>=190000 && count<=194048 && sqrt(sum/count)>0.1);
    NSMutableDictionary *cancelConfig=[config mutableCopy];NSString *cancelPath=[root stringByAppendingPathComponent:@"canceled.mp4"];cancelConfig[@"path"]=cancelPath;
    [[NSFileManager defaultManager] removeItemAtPath:cancelPath error:nil];
    NSString *cancelText=[[NSString alloc] initWithData:[NSJSONSerialization dataWithJSONObject:cancelConfig options:0 error:nil] encoding:NSUTF8StringEncoding];
    ps_video_reset();encoder=ps_video_create(cancelText.UTF8String);CHECK(encoder);ps_video_cancel();CHECK(!ps_video_frame(encoder,pixels,320*180*4,0));ps_video_close(encoder);free(pixels);
    [[NSFileManager defaultManager] removeItemAtPath:cancelPath error:nil];
    puts("PASS: AVFoundation H.264/AAC, 60 frames, exact PTS/duration, orientation, decoded audio and cancellation");
    return 0;
} }
