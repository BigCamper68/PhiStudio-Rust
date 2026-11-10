package com.phistudio.rust;

import android.app.Activity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileOutputStream;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Android URI access only. Rust owns chart parsing and project workspaces. */
final class DocumentBridge {
    private static final int REQUEST = 4101;
    private final Activity activity;
    private final ExecutorService io = Executors.newSingleThreadExecutor();
    private long token;
    private boolean pending;
    private String exportPath;
    private String mediaKind;
    private long snapshotLimit = 256L*1024*1024;
    DocumentBridge(Activity activity, Bundle state) {
        this.activity = activity;
        if (state != null) {
            token = state.getLong("document.token");
            pending = state.getBoolean("document.pending", false);
            exportPath = state.getString("document.exportPath");
            mediaKind = state.getString("document.mediaKind");
            snapshotLimit = state.getLong("document.limit", 256L*1024*1024);
        }
    }
    void saveState(Bundle state) {
        state.putLong("document.token", token);
        state.putBoolean("document.pending", pending);
        state.putString("document.exportPath", exportPath);
        state.putString("document.mediaKind", mediaKind);
        state.putLong("document.limit", snapshotLimit);
    }
    void request(long requestedToken) {
        if (pending) {
            MainActivity.documentResult(requestedToken,-1,"","","Another file picker is still open");
            return;
        }
        token = requestedToken; pending = true; exportPath = null; mediaKind = null; snapshotLimit = 256L*1024*1024;
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType("*/*");
        intent.putExtra(Intent.EXTRA_MIME_TYPES,
            new String[]{"application/zip","application/x-zip-compressed","application/octet-stream"});
        try { activity.startActivityForResult(intent,REQUEST); }
        catch (RuntimeException error) { pending=false; MainActivity.documentResult(requestedToken,-1,"","",describe(error)); }
    }
    void requestMedia(String kind, long requestedToken) {
        if (pending) { MainActivity.documentResult(requestedToken,-1,"","","Another file picker is still open"); return; }
        boolean shader = "shader".equals(kind);
        boolean resource = "resource".equals(kind);
        boolean plugin = "plugin".equals(kind);
        boolean skin = "skin".equals(kind);
        boolean hitsound = "hitsound".equals(kind);
        boolean audio = "audio".equals(kind) || hitsound;
        if (!shader && !resource && !plugin && !audio && !skin && !"illustration".equals(kind) && !"line-texture".equals(kind)) { MainActivity.documentResult(requestedToken,-1,"","","Unknown media type"); return; }
        token = requestedToken; pending = true; exportPath = null; mediaKind = kind;
        snapshotLimit = (shader ? 1L : skin || plugin ? 256L : hitsound ? 8L : audio || resource ? 128L : 32L)*1024*1024;
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType(shader || audio || skin || resource || plugin ? "*/*" : "image/*");
        if (audio) intent.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{"audio/*","application/ogg","video/mp4","application/octet-stream"});
        try { activity.startActivityForResult(intent, REQUEST); }
        catch (RuntimeException error) { pending=false; mediaKind=null; MainActivity.documentResult(requestedToken,-1,"","",describe(error)); }
    }
    void requestExportVideo(String path, String name, long requestedToken) {
        requestExport(path, name, requestedToken, "video/mp4");
    }
    void requestExport(String path, String name, long requestedToken) {
        requestExport(path, name, requestedToken, name.endsWith(".phisounds.json") ? "application/json" : "application/zip");
    }
    void requestExportFile(String path, String name, long requestedToken) {
        requestExport(path, name, requestedToken, "application/octet-stream");
    }
    private void requestExport(String path, String name, long requestedToken, String mime) {
        if (pending) { MainActivity.documentResult(requestedToken,-1,"","","Another file picker is still open"); return; }
        token = requestedToken; pending = true; exportPath = path; mediaKind = null;
        Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        intent.addCategory(Intent.CATEGORY_OPENABLE);
        intent.setType(mime);
        intent.putExtra(Intent.EXTRA_TITLE, name);
        try { activity.startActivityForResult(intent, REQUEST); }
        catch (RuntimeException error) { pending=false; exportPath=null; MainActivity.documentResult(requestedToken,-1,"","",describe(error)); }
    }
    boolean accept(int request, int result, Intent data) {
        if (request != REQUEST) return false;
        if (!pending) return true;
        final long currentToken=token;
        pending=false;
        final String exporting = exportPath;
        final boolean readingMedia = mediaKind != null;
        final long limit = snapshotLimit;
        mediaKind = null;
        exportPath = null;
        if (result != Activity.RESULT_OK || data == null || data.getData() == null) {
            MainActivity.documentResult(currentToken,0,"","",""); return true;
        }
        final Uri uri=data.getData();
        io.execute(() -> {
            File copied = null;
            try {
                if (exporting != null) {
                    try (InputStream input = new FileInputStream(exporting);
                         OutputStream output = activity.getContentResolver().openOutputStream(uri, "wt")) {
                        if (output == null) throw new IOException("Could not write the project package");
                        copy(input, output, new File(exporting).length());
                    }
                    MainActivity.documentResult(currentToken,1,"","","");
                    return;
                }
                String name=displayName(uri);
                copied=File.createTempFile(readingMedia ? "phistudio-media-" : "phistudio-import-", readingMedia ? ".media" : ".package",activity.getCacheDir());
                try (InputStream input=activity.getContentResolver().openInputStream(uri);
                     FileOutputStream output=new FileOutputStream(copied)) {
                    if (input==null) throw new IOException("Could not read the file");
                    copy(input,output,limit);
                    output.getFD().sync();
                }
                MainActivity.documentResult(currentToken,1,copied.getAbsolutePath(),name,"");
                copied=null; // Ownership passes to the Rust importer.
            } catch (Exception error) {
                MainActivity.documentResult(currentToken,-1,"","",describe(error));
            } finally { if (copied!=null) copied.delete(); }
        });
        return true;
    }
    private String displayName(Uri uri) {
        try (Cursor cursor=activity.getContentResolver().query(uri,new String[]{OpenableColumns.DISPLAY_NAME},null,null,null)) {
            if (cursor!=null && cursor.moveToFirst() && !cursor.isNull(0)) return cursor.getString(0);
        } catch (RuntimeException ignored) { }
        return "Imported package";
    }
    private static void copy(InputStream input, OutputStream output, long limit) throws IOException {
        byte[] buffer=new byte[64*1024]; long total=0; int count;
        while ((count=input.read(buffer))!=-1) {
            total+=count;
            if (total>limit) throw new IOException("File exceeds the size limit");
            output.write(buffer,0,count);
        }
        output.flush();
    }
    private static String describe(Exception error) {
        return error.getMessage()==null ? error.getClass().getSimpleName() : error.getMessage();
    }
    void close() { io.shutdown(); }
}
