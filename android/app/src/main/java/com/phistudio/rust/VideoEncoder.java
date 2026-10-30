package com.phistudio.rust;

import android.media.*;
import android.opengl.*;
import android.os.Build;
import android.view.Surface;
import org.json.JSONObject;
import java.io.*;
import java.nio.*;
import java.util.concurrent.CancellationException;

/** Offline H.264/AAC encoder. Called exclusively by Rust's bounded export worker. */
public final class VideoEncoder implements AutoCloseable {
    private static volatile boolean canceled;
    public static void reset() { canceled = false; }
    public static void cancel() { canceled = true; }
    private static void check() { if (canceled) throw new CancellationException("Video export canceled"); }
    private final int width, height, fps;
    private final long frames;
    private MediaCodec codec;
    private MediaMuxer muxer;
    private MediaExtractor audio;
    private File audioFile;
    private int videoTrack = -1, audioTrack = -1;
    private boolean started, finished;
    private Surface surface;
    private EGLDisplay display = EGL14.EGL_NO_DISPLAY;
    private EGLContext context = EGL14.EGL_NO_CONTEXT;
    private EGLSurface eglSurface = EGL14.EGL_NO_SURFACE;
    private int program, texture;
    private FloatBuffer vertices, coordinates;
    private ByteBuffer pixels;
    private final MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();

    public VideoEncoder(String configuration) throws Exception {
        JSONObject c = new JSONObject(configuration);
        width = c.getInt("width"); height = c.getInt("height"); fps = c.getInt("fps"); frames = c.getLong("frames");
        try {
            check();
            if (!c.isNull("wav")) {
                audioFile = new File(c.getString("path") + ".m4a");
                encodeAudio(c.getString("wav"), audioFile.getPath(), c.getInt("audioBitrate"));
                audio = new MediaExtractor(); audio.setDataSource(audioFile.getPath()); audio.selectTrack(0);
            }
            MediaFormat format = MediaFormat.createVideoFormat("video/avc", width, height);
            format.setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface);
            format.setInteger(MediaFormat.KEY_BIT_RATE, c.getInt("bitrate"));
            format.setInteger(MediaFormat.KEY_FRAME_RATE, fps); format.setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2);
            if (Build.VERSION.SDK_INT >= 29) format.setInteger(MediaFormat.KEY_MAX_B_FRAMES, 0);
            String name = new MediaCodecList(MediaCodecList.REGULAR_CODECS).findEncoderForFormat(format);
            if (name == null) throw new IOException("This device cannot encode the selected video resolution and FPS");
            codec = MediaCodec.createByCodecName(name); codec.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE);
            surface = codec.createInputSurface(); codec.start();
            muxer = new MediaMuxer(c.getString("path"), MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4);
            if (audio != null) audioTrack = muxer.addTrack(audio.getTrackFormat(0));
            initializeGL();
        } catch (Exception error) { close(); throw error; }
    }
    private static FloatBuffer floats(float[] data) { FloatBuffer b=ByteBuffer.allocateDirect(data.length*4).order(ByteOrder.nativeOrder()).asFloatBuffer(); b.put(data).position(0); return b; }
    private static void egl(boolean ok) throws IOException { if (!ok) throw new IOException("Video EGL error: " + EGL14.eglGetError()); }
    private void initializeGL() throws IOException {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY);
        int[] versions=new int[2]; egl(EGL14.eglInitialize(display,versions,0,versions,1));
        int[] attrs={EGL14.EGL_RED_SIZE,8,EGL14.EGL_GREEN_SIZE,8,EGL14.EGL_BLUE_SIZE,8,EGL14.EGL_ALPHA_SIZE,8,EGL14.EGL_RENDERABLE_TYPE,EGL14.EGL_OPENGL_ES2_BIT,0x3142,1,EGL14.EGL_NONE};
        EGLConfig[] configs=new EGLConfig[1]; int[] count=new int[1]; egl(EGL14.eglChooseConfig(display,attrs,0,configs,0,1,count,0));
        if(count[0]==0) throw new IOException("No recordable EGL configuration");
        context=EGL14.eglCreateContext(display,configs[0],EGL14.EGL_NO_CONTEXT,new int[]{EGL14.EGL_CONTEXT_CLIENT_VERSION,2,EGL14.EGL_NONE},0);
        eglSurface=EGL14.eglCreateWindowSurface(display,configs[0],surface,new int[]{EGL14.EGL_NONE},0);
        egl(EGL14.eglMakeCurrent(display,eglSurface,eglSurface,context));
        program=GLES20.glCreateProgram();
        int vs=shader(GLES20.GL_VERTEX_SHADER,"attribute vec2 position;attribute vec2 uv;varying vec2 coord;void main(){gl_Position=vec4(position,0.,1.);coord=uv;}");
        int fs=shader(GLES20.GL_FRAGMENT_SHADER,"precision mediump float;uniform sampler2D image;varying vec2 coord;void main(){gl_FragColor=texture2D(image,coord);}");
        GLES20.glAttachShader(program,vs); GLES20.glAttachShader(program,fs); GLES20.glLinkProgram(program);
        GLES20.glDeleteShader(vs); GLES20.glDeleteShader(fs);
        int[] ok=new int[1]; GLES20.glGetProgramiv(program,GLES20.GL_LINK_STATUS,ok,0);
        if(ok[0]==0) throw new IOException(GLES20.glGetProgramInfoLog(program));
        vertices=floats(new float[]{-1,-1,1,-1,-1,1,1,1});
        // Rust sends top-down RGBA; GL's first texture row is its bottom row.
        coordinates=floats(new float[]{0,1,1,1,0,0,1,0});
        int[] ids=new int[1]; GLES20.glGenTextures(1,ids,0); texture=ids[0];
        GLES20.glBindTexture(GLES20.GL_TEXTURE_2D,texture);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_MIN_FILTER,GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_MAG_FILTER,GLES20.GL_LINEAR);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_WRAP_S,GLES20.GL_CLAMP_TO_EDGE);
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_2D,GLES20.GL_TEXTURE_WRAP_T,GLES20.GL_CLAMP_TO_EDGE);
        GLES20.glTexImage2D(GLES20.GL_TEXTURE_2D,0,GLES20.GL_RGBA,width,height,0,GLES20.GL_RGBA,GLES20.GL_UNSIGNED_BYTE,null);
        pixels=ByteBuffer.allocateDirect(width*height*4);
    }
    private static int shader(int type,String source) throws IOException {
        int id=GLES20.glCreateShader(type); GLES20.glShaderSource(id,source); GLES20.glCompileShader(id);
        int[] ok=new int[1]; GLES20.glGetShaderiv(id,GLES20.GL_COMPILE_STATUS,ok,0);
        if(ok[0]==0) {String message=GLES20.glGetShaderInfoLog(id);GLES20.glDeleteShader(id);throw new IOException(message);} return id;
    }
    public void frame(byte[] rgba,long index) throws Exception {
        check(); if(finished || rgba.length!=width*height*4) throw new IOException("Invalid video frame");
        drain(false);
        pixels.clear(); pixels.put(rgba).position(0);
        GLES20.glViewport(0,0,width,height); GLES20.glUseProgram(program);
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0); GLES20.glBindTexture(GLES20.GL_TEXTURE_2D,texture);
        GLES20.glTexSubImage2D(GLES20.GL_TEXTURE_2D,0,0,0,width,height,GLES20.GL_RGBA,GLES20.GL_UNSIGNED_BYTE,pixels);
        int position=GLES20.glGetAttribLocation(program,"position"), uv=GLES20.glGetAttribLocation(program,"uv");
        GLES20.glEnableVertexAttribArray(position); GLES20.glEnableVertexAttribArray(uv);
        GLES20.glVertexAttribPointer(position,2,GLES20.GL_FLOAT,false,0,vertices);
        GLES20.glVertexAttribPointer(uv,2,GLES20.GL_FLOAT,false,0,coordinates);
        GLES20.glUniform1i(GLES20.glGetUniformLocation(program,"image"),0);
        GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP,0,4);
        int glError=GLES20.glGetError(); if(glError!=GLES20.GL_NO_ERROR) throw new IOException("Video GL error: "+glError);
        egl(EGLExt.eglPresentationTimeANDROID(display,eglSurface,index*1_000_000_000L/fps));
        egl(EGL14.eglSwapBuffers(display,eglSurface)); drain(false);
    }
    private void drain(boolean end) throws Exception {
        long deadline=System.nanoTime()+30_000_000_000L;
        while(true) {
            check(); int id=codec.dequeueOutputBuffer(info,end?10_000:0);
            if(id==MediaCodec.INFO_TRY_AGAIN_LATER) { if(!end)return; if(System.nanoTime()>deadline)throw new IOException("Video encoder timed out"); continue; }
            if(id==MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                if(started)throw new IOException("Video encoder format changed twice");
                videoTrack=muxer.addTrack(codec.getOutputFormat());muxer.start();started=true;continue;
            }
            if(id<0)continue;
            ByteBuffer data=codec.getOutputBuffer(id);
            if((info.flags&MediaCodec.BUFFER_FLAG_CODEC_CONFIG)==0 && info.size>0) {
                if(!started || data==null)throw new IOException("Invalid video encoder output");
                data.position(info.offset);data.limit(info.offset+info.size);
                muxer.writeSampleData(videoTrack,data,info);
            }
            boolean eos=(info.flags&MediaCodec.BUFFER_FLAG_END_OF_STREAM)!=0;
            codec.releaseOutputBuffer(id,false); if(eos)return;
            deadline=System.nanoTime()+30_000_000_000L;
        }
    }
    public void finish() throws Exception {
        check();codec.signalEndOfInputStream();drain(true);
        if(!started)throw new IOException("No encoded video frames");
        MediaCodec.BufferInfo end=new MediaCodec.BufferInfo();end.set(0,0,frames*1_000_000L/fps,MediaCodec.BUFFER_FLAG_END_OF_STREAM);
        muxer.writeSampleData(videoTrack,ByteBuffer.allocate(0),end);
        if(audio!=null) {
            ByteBuffer sample=ByteBuffer.allocateDirect(1024*1024); MediaCodec.BufferInfo ai=new MediaCodec.BufferInfo();
            while(true) {
                check();sample.clear();int size=audio.readSampleData(sample,0);long time=audio.getSampleTime();
                if(size<0 || time>=frames*1_000_000L/fps)break;
                ai.set(0,size,Math.max(0,time),audio.getSampleFlags());sample.position(0);sample.limit(size);muxer.writeSampleData(audioTrack,sample,ai);audio.advance();
            }
            muxer.writeSampleData(audioTrack,ByteBuffer.allocate(0),end);
        }
        muxer.stop();started=false;finished=true;
    }
    private static void encodeAudio(String wav,String output,int bitrate) throws Exception {
        MediaCodec encoder=null;MediaMuxer target=null;boolean running=false;
        try(FileInputStream input=new FileInputStream(wav)) {
            if(input.skip(44)!=44)throw new IOException("Invalid movie WAV");
            MediaFormat f=MediaFormat.createAudioFormat("audio/mp4a-latm",48000,2);
            f.setInteger(MediaFormat.KEY_AAC_PROFILE,MediaCodecInfo.CodecProfileLevel.AACObjectLC);f.setInteger(MediaFormat.KEY_BIT_RATE,bitrate);
            encoder=MediaCodec.createEncoderByType("audio/mp4a-latm");encoder.configure(f,null,null,MediaCodec.CONFIGURE_FLAG_ENCODE);encoder.start();
            target=new MediaMuxer(output,MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4);
            byte[] pcm=new byte[65536];long samples=0;boolean inputEnd=false;int track=-1;
            MediaCodec.BufferInfo info=new MediaCodec.BufferInfo();long deadline=System.nanoTime()+30_000_000_000L;
            while(true) {
                check();
                if(!inputEnd) {
                    int id=encoder.dequeueInputBuffer(10_000);
                    if(id>=0) {
                        ByteBuffer b=encoder.getInputBuffer(id);b.clear();int size=input.read(pcm,0,Math.min(pcm.length,b.remaining())/4*4);
                        if(size<0){size=0;inputEnd=true;}else b.put(pcm,0,size);
                        encoder.queueInputBuffer(id,0,size,samples*1_000_000L/48000,inputEnd?MediaCodec.BUFFER_FLAG_END_OF_STREAM:0);samples+=size/4;
                    }
                }
                int id=encoder.dequeueOutputBuffer(info,10_000);
                if(id==MediaCodec.INFO_OUTPUT_FORMAT_CHANGED){track=target.addTrack(encoder.getOutputFormat());target.start();running=true;}
                else if(id>=0) {
                    if(info.size>0 && info.presentationTimeUs>=0 && (info.flags&MediaCodec.BUFFER_FLAG_CODEC_CONFIG)==0) {
                        ByteBuffer data=encoder.getOutputBuffer(id);if(data==null)throw new IOException("Invalid audio encoder output");
                        data.position(info.offset);data.limit(info.offset+info.size);target.writeSampleData(track,data,info);
                    }
                    boolean eos=(info.flags&MediaCodec.BUFFER_FLAG_END_OF_STREAM)!=0;encoder.releaseOutputBuffer(id,false);
                    if(eos)break;deadline=System.nanoTime()+30_000_000_000L;
                } else if(System.nanoTime()>deadline)throw new IOException("Audio encoder timed out");
            }
            target.stop();running=false;
        } finally { if(encoder!=null){try{encoder.stop();}catch(Exception ignored){} encoder.release();} if(target!=null){if(running)try{target.stop();}catch(Exception ignored){}target.release();} }
    }
    @Override public void close() {
        if(display!=EGL14.EGL_NO_DISPLAY) {
            if(context!=EGL14.EGL_NO_CONTEXT){if(texture!=0)GLES20.glDeleteTextures(1,new int[]{texture},0);if(program!=0)GLES20.glDeleteProgram(program);}
            EGL14.eglMakeCurrent(display,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_SURFACE,EGL14.EGL_NO_CONTEXT);
            if(eglSurface!=EGL14.EGL_NO_SURFACE)EGL14.eglDestroySurface(display,eglSurface);
            if(context!=EGL14.EGL_NO_CONTEXT)EGL14.eglDestroyContext(display,context);
            // This display also serves the editor: do not eglTerminate it.
            EGL14.eglReleaseThread();display=EGL14.EGL_NO_DISPLAY;
        }
        if(surface!=null){surface.release();surface=null;}
        if(codec!=null){try{codec.stop();}catch(Exception ignored){}codec.release();codec=null;}
        if(muxer!=null){if(started)try{muxer.stop();}catch(Exception ignored){}muxer.release();muxer=null;}
        if(audio!=null){audio.release();audio=null;}if(audioFile!=null)audioFile.delete();
    }
}
