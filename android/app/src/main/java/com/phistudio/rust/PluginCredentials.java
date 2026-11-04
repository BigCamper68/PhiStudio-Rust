package com.phistudio.rust;

import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/** Authenticated encrypted records; the AES key never leaves AndroidKeyStore. */
final class PluginCredentials {
    private static final String ALIAS = "PhiStudio.PluginCredentials.v1";
    private static final String NAME = "phistudio_plugin_credentials_v1";

    private static SecretKey key() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        if (store.containsAlias(ALIAS)) return (SecretKey) store.getKey(ALIAS, null);
        KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(ALIAS,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build());
        return generator.generateKey();
    }

    private static boolean identifier(String value) {
        return value != null && value.length() <= 128
                && value.matches("[a-z][a-z0-9_-]*(\\.[a-z][a-z0-9_-]*)*");
    }

    static synchronized String call(Context context, String operation, String owner,
            String name, String payload) {
        if (context == null || !identifier(owner) || !identifier(name))
            throw new IllegalArgumentException("Invalid credential owner or key");
        String record = owner + ":" + name;
        byte[] aad = record.getBytes(StandardCharsets.UTF_8);
        SharedPreferences preferences = context.getSharedPreferences(NAME, Context.MODE_PRIVATE);
        try {
            if ("remove".equals(operation)) {
                if (!preferences.edit().remove(record).commit()) throw new Exception();
                return "ok";
            }
            if ("read".equals(operation)) {
                String encoded = preferences.getString(record, null);
                if (encoded == null) return null;
                if (encoded.length() > 64 * 1024) throw new Exception();
                String[] parts = encoded.split("\\.", 2);
                if (parts.length != 2) throw new Exception();
                byte[] iv = Base64.decode(parts[0], Base64.NO_WRAP);
                if (iv.length != 12) throw new Exception();
                Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
                cipher.init(Cipher.DECRYPT_MODE, key(), new GCMParameterSpec(128, iv));
                cipher.updateAAD(aad);
                byte[] value = cipher.doFinal(Base64.decode(parts[1], Base64.NO_WRAP));
                if (value.length > 32 * 1024) throw new Exception();
                return new String(value, StandardCharsets.UTF_8);
            }
            if ("write".equals(operation)) {
                if (payload == null) throw new Exception();
                byte[] value = payload.getBytes(StandardCharsets.UTF_8);
                if (value.length > 32 * 1024) throw new Exception();
                Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
                cipher.init(Cipher.ENCRYPT_MODE, key());
                cipher.updateAAD(aad);
                String encrypted = Base64.encodeToString(cipher.getIV(), Base64.NO_WRAP) + "."
                        + Base64.encodeToString(cipher.doFinal(value), Base64.NO_WRAP);
                if (!preferences.edit().putString(record, encrypted).commit()) throw new Exception();
                return "ok";
            }
            throw new IllegalArgumentException("Unknown credential operation");
        } catch (Exception error) {
            // Never include a secret, ciphertext, request payload or provider error.
            throw new IllegalStateException("Protected credential operation failed");
        }
    }
    private PluginCredentials() {}
}
