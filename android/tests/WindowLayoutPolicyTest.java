package com.phistudio.rust;

import java.util.Arrays;

public final class WindowLayoutPolicyTest {
    private static void check(int[] actual, int... expected) {
        if (!Arrays.equals(actual, expected)) throw new AssertionError(Arrays.toString(actual));
    }

    public static void main(String[] args) {
        int[] cameraLeft = {52, 0, 0, 0};
        check(WindowLayoutPolicy.padding(false, true, new int[]{0, 24, 0, 32}, cameraLeft), 52, 24, 0, 32);
        check(WindowLayoutPolicy.padding(true, true, new int[4], cameraLeft), 0, 0, 0, 0);
        check(WindowLayoutPolicy.padding(true, true, new int[]{0, 0, 0, 32}, cameraLeft), 0, 0, 0, 32);
        check(WindowLayoutPolicy.padding(true, false, new int[]{52, 0, 0, 300}, cameraLeft), 0, 0, 0, 300);
        check(WindowLayoutPolicy.padding(false, false, new int[]{52, 24, 0, 300}, cameraLeft), 52, 24, 0, 300);
        check(WindowLayoutPolicy.padding(true, false, new int[]{0, 0, 52, 0}, new int[]{0, 0, 52, 0}), 0, 0, 0, 0);
        check(WindowLayoutPolicy.padding(true, true, new int[]{0, 0, 0, 300}, new int[]{0, 42, 0, 0}), 0, 0, 0, 300);
        System.out.println("Window layout policy PASS");
    }
}
