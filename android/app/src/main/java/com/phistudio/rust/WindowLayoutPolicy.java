package com.phistudio.rust;

/** Keep system/keyboard padding while allowing an explicit immersive cutout layout. */
final class WindowLayoutPolicy {
    private WindowLayoutPolicy() {}

    static int[] padding(boolean fullscreen, boolean separateSystemBars, int[] system, int[] cutout) {
        int[] padding = new int[4];
        for (int i = 0; i < 4; i++) {
            if (!fullscreen) padding[i] = Math.max(system[i], cutout[i]);
            else if (separateSystemBars) padding[i] = system[i];
            // Legacy system-window insets include the cutout. Keep larger insets
            // such as the resized keyboard or a visible navigation bar.
            else padding[i] = system[i] > cutout[i] ? system[i] : 0;
        }
        return padding;
    }
}
