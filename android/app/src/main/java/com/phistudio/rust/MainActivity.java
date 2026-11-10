// Adapted for PhiStudio from Mivik/prpr-miniquad 138679c (MIT/Apache-2.0).
package com.phistudio.rust;

import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.opengles.GL10;

import android.app.Activity;
import android.os.Bundle;
import android.util.Log;

import android.view.View;
import android.view.Surface;
import android.view.Window;
import android.view.WindowManager.LayoutParams;
import android.view.SurfaceView;
import android.view.SurfaceHolder;
import android.view.MotionEvent;
import android.view.KeyEvent;
import android.view.inputmethod.InputMethodManager;

import android.content.Context;
import android.content.Intent;


import quad_native.QuadNative;

// note: //% is a special miniquad's pre-processor for plugins
// when there are no plugins - //% whatever will be replaced to an empty string
// before compiling

//% IMPORTS

class QuadSurface
    extends
        SurfaceView
    implements
        View.OnTouchListener,
        View.OnKeyListener,
        SurfaceHolder.Callback {

    public QuadSurface(Context context){
        super(context);
        getHolder().addCallback(this);

        setFocusable(true);
        setFocusableInTouchMode(true);
        requestFocus();
        setOnTouchListener(this);
        setOnKeyListener(this);
    }

    private boolean textInputRequested = false;
    void setTextInputRequested(boolean requested) { textInputRequested = requested; }
    @Override
    public boolean onCheckIsTextEditor() { return textInputRequested; }

    @Override
    public android.view.inputmethod.InputConnection onCreateInputConnection(
            android.view.inputmethod.EditorInfo attributes) {
        if (!textInputRequested) return null;
        attributes.inputType = android.text.InputType.TYPE_CLASS_TEXT
                | android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
        attributes.imeOptions = android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI
                | android.view.inputmethod.EditorInfo.IME_ACTION_DONE;
        return new android.view.inputmethod.BaseInputConnection(this, false) {
            @Override
            public boolean commitText(CharSequence text, int position) {
                text.toString().codePoints().forEach(QuadNative::surfaceOnCharacter);
                return true;
            }
            @Override
            public boolean deleteSurroundingText(int before, int after) {
                for (int i = 0; i < before; i++) {
                    QuadNative.surfaceOnKeyDown(KeyEvent.KEYCODE_DEL);
                    QuadNative.surfaceOnKeyUp(KeyEvent.KEYCODE_DEL);
                }
                return true;
            }
            @Override
            public boolean sendKeyEvent(KeyEvent event) {
                if (isMediaVolumeKey(event.getKeyCode())) {
                    return ((Activity)getContext()).dispatchKeyEvent(event);
                }
                if (event.getAction() == KeyEvent.ACTION_DOWN) {
                    QuadNative.surfaceOnKeyDown(event.getKeyCode());
                } else if (event.getAction() == KeyEvent.ACTION_UP) {
                    QuadNative.surfaceOnKeyUp(event.getKeyCode());
                }
                return true;
            }
            @Override
            public boolean performEditorAction(int action) {
                QuadNative.surfaceOnKeyDown(KeyEvent.KEYCODE_ENTER);
                QuadNative.surfaceOnKeyUp(KeyEvent.KEYCODE_ENTER);
                return true;
            }
        };
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        Log.i("SAPP", "surfaceCreated");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceCreated(surface);
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        Log.i("SAPP", "surfaceDestroyed");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceDestroyed(surface);
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder,
                               int format,
                               int width,
                               int height) {
        Log.i("SAPP", "surfaceChanged");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceChanged(surface, width, height);

    }

    @Override
    public boolean onTouch(View v, MotionEvent event) {
        int pointerCount = event.getPointerCount();
        int action = event.getActionMasked();

        switch(action) {
        case MotionEvent.ACTION_MOVE: {
            for (int i = 0; i < pointerCount; i++) {
                final int id = event.getPointerId(i);
                final float x = event.getX(i);
                final float y = event.getY(i);
                QuadNative.surfaceOnTouch(id, 0, x, y, event.getEventTime());
            }
            break;
        }
        case MotionEvent.ACTION_UP: {
            final int id = event.getPointerId(0);
            final float x = event.getX(0);
            final float y = event.getY(0);
            QuadNative.surfaceOnTouch(id, 1, x, y, event.getEventTime());
            break;
        }
        case MotionEvent.ACTION_DOWN: {
            final int id = event.getPointerId(0);
            final float x = event.getX(0);
            final float y = event.getY(0);
            QuadNative.surfaceOnTouch(id, 2, x, y, event.getEventTime());
            break;
        }
        case MotionEvent.ACTION_POINTER_UP: {
            final int pointerIndex = event.getActionIndex();
            final int id = event.getPointerId(pointerIndex);
            final float x = event.getX(pointerIndex);
            final float y = event.getY(pointerIndex);
            QuadNative.surfaceOnTouch(id, 1, x, y, event.getEventTime());
            break;
        }
        case MotionEvent.ACTION_POINTER_DOWN: {
            final int pointerIndex = event.getActionIndex();
            final int id = event.getPointerId(pointerIndex);
            final float x = event.getX(pointerIndex);
            final float y = event.getY(pointerIndex);
            QuadNative.surfaceOnTouch(id, 2, x, y, event.getEventTime());
            break;
        }
        case MotionEvent.ACTION_CANCEL: {
            for (int i = 0; i < pointerCount; i++) {
                final int id = event.getPointerId(i);
                final float x = event.getX(i);
                final float y = event.getY(i);
                QuadNative.surfaceOnTouch(id, 3, x, y, event.getEventTime());
            }
            break;
        }
        default:
            break;
        }

        return true;
    }

    private static boolean isMediaVolumeKey(int keyCode) {
        return keyCode == KeyEvent.KEYCODE_VOLUME_UP || keyCode == KeyEvent.KEYCODE_VOLUME_DOWN
                || keyCode == KeyEvent.KEYCODE_VOLUME_MUTE;
    }

    // docs says getCharacters are deprecated
    // but somehow on non-latyn input all keyCode and all the relevant fields in the KeyEvent are zeros
    // and only getCharacters has some usefull data
    @SuppressWarnings("deprecation")
    @Override
    public boolean onKey(View v, int keyCode, KeyEvent event) {
        // Let Activity handle media volume, including held/repeated presses.
        if (isMediaVolumeKey(keyCode)) {
            return false;
        }
        if (event.getAction() == KeyEvent.ACTION_DOWN && keyCode != 0) {
            QuadNative.surfaceOnKeyDown(keyCode);
        }

        if (event.getAction() == KeyEvent.ACTION_UP && keyCode != 0) {
            QuadNative.surfaceOnKeyUp(keyCode);
        }

        if (event.getAction() == KeyEvent.ACTION_UP || event.getAction() == KeyEvent.ACTION_MULTIPLE) {
            int character = event.getUnicodeChar();
            if (character == 0) {
                String characters = event.getCharacters();
                if (characters != null && characters.length() > 0) {
                    character = characters.charAt(0);
                }
            }

            if (character != 0) {
                QuadNative.surfaceOnCharacter(character);
            }
        }

        return true;
    }

    public Surface getNativeSurface() {
        return getHolder().getSurface();
    }
}

public class MainActivity extends Activity {
    //% MAIN_ACTIVITY_BODY

    private QuadSurface view;
    private View editorContainer;
    private boolean editorFullscreen;
    private KeyboardController keyboard;
    private volatile boolean resumed = false;
    private DocumentBridge documents;
    private static java.lang.ref.WeakReference<MainActivity> current = new java.lang.ref.WeakReference<>(null);
    private static native void setStorageRoot(String path);
    private static native void setDisplayDensity(float density);
    private static native void notifySuspended();
    private static native void notifyBackRequested();
    static native void documentResult(long token,int status,String path,String name,String error);

    public static void videoReset() {
        VideoEncoder.reset(); MainActivity activity=current.get();
        if(activity!=null)activity.runOnUiThread(() -> activity.getWindow().addFlags(LayoutParams.FLAG_KEEP_SCREEN_ON));
    }
    public static void videoRelease() {
        MainActivity activity=current.get();
        if(activity!=null)activity.runOnUiThread(() -> activity.getWindow().clearFlags(LayoutParams.FLAG_KEEP_SCREEN_ON));
    }
    public static void videoCancel() { VideoEncoder.cancel(); }
    public static VideoEncoder videoCreate(String configuration) throws Exception { return new VideoEncoder(configuration); }
    public static void requestExportVideo(String path, String name, long token) {
        MainActivity activity = current.get();
        if (activity == null) { documentResult(token,-1,"","","The app window is closed"); return; }
        activity.runOnUiThread(() -> activity.documents.requestExportVideo(path, name, token));
    }
    public static void requestExportPackage(String path, String name, long token) {
        MainActivity activity = current.get();
        if (activity == null) { documentResult(token,-1,"","","The app window is closed"); return; }
        activity.runOnUiThread(() -> activity.documents.requestExport(path, name, token));
    }
    public static void requestExportFile(String path, String name, long token) {
        MainActivity activity = current.get();
        if (activity == null) { documentResult(token,-1,"","","The app window is closed"); return; }
        activity.runOnUiThread(() -> activity.documents.requestExportFile(path, name, token));
    }
    public static void requestMedia(String kind, long token) {
        MainActivity activity = current.get();
        if (activity == null) { documentResult(token,-1,"","","The app window is closed"); return; }
        activity.runOnUiThread(() -> activity.documents.requestMedia(kind, token));
    }
    public static void requestPackage(long token) {
        MainActivity activity=current.get();
        if (activity==null) { documentResult(token,-1,"","","The app window is closed"); return; }
        activity.runOnUiThread(() -> activity.documents.request(token));
    }

    public static String credentialStore(String operation, String owner, String key, String payload) {
        MainActivity activity = current.get();
        if (activity == null) throw new IllegalStateException("Android credential bridge is unavailable");
        return PluginCredentials.call(activity.getApplicationContext(), operation, owner, key, payload);
    }

    static {
        System.loadLibrary("phistudio_app");
    }

    @Override
    public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setVolumeControlStream(android.media.AudioManager.STREAM_MUSIC);
        current = new java.lang.ref.WeakReference<>(this);
        documents = new DocumentBridge(this,savedInstanceState);
        setStorageRoot(getFilesDir().getAbsolutePath());
        setDisplayDensity(getResources().getDisplayMetrics().density);
        QuadNative.initializeContext(this);

        this.requestWindowFeature(Window.FEATURE_NO_TITLE);

        view = new QuadSurface(this);
        final InputMethodManager input = (InputMethodManager)getSystemService(Context.INPUT_METHOD_SERVICE);
        keyboard = new KeyboardController(new KeyboardController.Target() {
            public boolean isResumed() { return resumed; }
            public boolean hasWindowFocus() { return view.hasWindowFocus(); }
            public boolean hasViewFocus() { return view.hasFocus(); }
            public boolean requestFocus() { return view.requestFocus(); }
            public void setTextInputRequested(boolean requested) { view.setTextInputRequested(requested); }
            public void post(Runnable action) { view.post(action); }
            public void restartInput() { input.restartInput(view); }
            public boolean show() { return input.showSoftInput(view, InputMethodManager.SHOW_IMPLICIT); }
            public void hide() { input.hideSoftInputFromWindow(view.getWindowToken(), 0); }
        });
        android.widget.FrameLayout container = new android.widget.FrameLayout(this);
        container.setBackgroundColor(android.graphics.Color.rgb(12,17,29));
        container.addView(view,new android.widget.FrameLayout.LayoutParams(-1,-1));
        editorContainer = container;
        container.setOnApplyWindowInsetsListener((root,insets) -> {
            int[] system;
            int[] cutout = new int[4];
            boolean separate = android.os.Build.VERSION.SDK_INT >= 30;
            if (separate) {
                android.graphics.Insets bars = insets.getInsets(android.view.WindowInsets.Type.systemBars());
                android.graphics.Insets camera = insets.getInsets(android.view.WindowInsets.Type.displayCutout());
                system = new int[]{bars.left, bars.top, bars.right, bars.bottom};
                cutout = new int[]{camera.left, camera.top, camera.right, camera.bottom};
            } else {
                system = new int[]{insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                    insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom()};
                if (android.os.Build.VERSION.SDK_INT >= 28 && insets.getDisplayCutout()!=null) {
                    android.view.DisplayCutout camera = insets.getDisplayCutout();
                    cutout = new int[]{camera.getSafeInsetLeft(), camera.getSafeInsetTop(),
                        camera.getSafeInsetRight(), camera.getSafeInsetBottom()};
                }
            }
            int[] padding = WindowLayoutPolicy.padding(editorFullscreen, separate, system, cutout);
            root.setPadding(padding[0], padding[1], padding[2], padding[3]);
            return insets;
        });
        if (android.os.Build.VERSION.SDK_INT >= 30) getWindow().setDecorFitsSystemWindows(false);
        setContentView(container);
        container.requestApplyInsets();

        QuadNative.activityOnCreate(this);

        //% MAIN_ACTIVITY_ON_CREATE
    }

    @Override
    protected void onResume() {
        super.onResume();
        resumed = true;
        showKeyboard(false);
        QuadNative.activityOnResume();

        //% MAIN_ACTIVITY_ON_RESUME
    }

    @Override
    public void onWindowFocusChanged(boolean focused) {
        super.onWindowFocusChanged(focused);
        if (keyboard != null) keyboard.windowFocusChanged(focused);
        if (focused && editorFullscreen) setFullScreen(true);
    }

    @Override
    public void onBackPressed() {
        Log.w("SAPP", "onBackPressed");

        // TODO: here is the place to handle request_quit/order_quit/cancel_quit

        notifyBackRequested();
    }

    @Override
    protected void onStop() {
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();

        QuadNative.activityOnDestroy();
        QuadNative.releaseContext();
        documents.close();
        if (current.get()==this) current.clear();
    }

    @Override
    protected void onPause() {
        resumed = false;
        showKeyboard(false);
        super.onPause();
        notifySuspended();
        QuadNative.activityOnPause();

        //% MAIN_ACTIVITY_ON_PAUSE
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode,resultCode,data);
        documents.accept(requestCode,resultCode,data);
        //% MAIN_ACTIVITY_ON_ACTIVITY_RESULT
    }

    @Override
    protected void onSaveInstanceState(Bundle state) {
        documents.saveState(state);
        super.onSaveInstanceState(state);
    }

    public void setFullScreen(final boolean fullscreen) {
        runOnUiThread(() -> {
            editorFullscreen = fullscreen;
            View decor = getWindow().getDecorView();
            if (android.os.Build.VERSION.SDK_INT >= 28) {
                LayoutParams attributes = getWindow().getAttributes();
                attributes.layoutInDisplayCutoutMode = fullscreen
                    ? (android.os.Build.VERSION.SDK_INT >= 30
                        ? LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                        : LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES)
                    : LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_DEFAULT;
                getWindow().setAttributes(attributes);
            }
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                getWindow().setDecorFitsSystemWindows(false);
                android.view.WindowInsetsController controller = decor.getWindowInsetsController();
                if (controller != null) {
                    if (fullscreen) {
                        controller.setSystemBarsBehavior(android.view.WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                        controller.hide(android.view.WindowInsets.Type.systemBars());
                    } else controller.show(android.view.WindowInsets.Type.systemBars());
                }
            } else {
                decor.setSystemUiVisibility(fullscreen
                    ? View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    : 0);
            }
            if (editorContainer != null) editorContainer.requestApplyInsets();
        });
    }

    public void showKeyboard(final boolean show) {
        runOnUiThread(() -> {
            if (keyboard != null) keyboard.request(show);
        });
    }
}
