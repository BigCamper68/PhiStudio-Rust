package com.phistudio.rust;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;

/** Runs on a plain JVM: no Android runtime, mocks library or native code needed. */
public final class KeyboardControllerTest {
    private static final class Target implements KeyboardController.Target {
        boolean resumed = true;
        boolean window = true;
        boolean focusable = true;
        boolean focused;
        boolean editing;
        int failures;
        int shown;
        int hidden;
        final ArrayDeque<Runnable> tasks = new ArrayDeque<>();
        final List<String> calls = new ArrayList<>();

        public boolean isResumed() { return resumed; }
        public boolean hasWindowFocus() { return window; }
        public boolean hasViewFocus() { return focused; }
        public boolean requestFocus() {
            calls.add("focus");
            focused = focusable;
            return focused;
        }
        public void setTextInputRequested(boolean requested) { editing = requested; }
        public void post(Runnable action) { tasks.add(action); }
        public void restartInput() {
            check(editing && focused && window, "Restart before editor/window focus");
            calls.add("restart");
        }
        public boolean show() {
            check(editing && focused && window && resumed, "Show before focus or after suspend");
            calls.add("show");
            shown++;
            if (failures > 0) { failures--; return false; }
            return true;
        }
        public void hide() { hidden++; }
        void drain() {
            int count = 0;
            while (!tasks.isEmpty()) {
                check(++count <= 10, "Unbounded keyboard retry");
                tasks.remove().run();
            }
        }
    }

    private static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    public static void main(String[] args) {
        Target target = new Target();
        target.window = false;
        KeyboardController keyboard = new KeyboardController(target);
        keyboard.request(true);
        check(target.editing && target.shown == 0 && target.hidden == 0,
                "An explicit request was discarded before window focus");
        target.window = true;
        keyboard.windowFocusChanged(true);
        check(target.shown == 0, "IME was shown inside the window focus callback");
        target.drain();
        check(target.calls.equals(List.of("focus", "restart", "show")), "Wrong focus/restart/show order");
        target.calls.clear();
        keyboard.request(true);
        target.drain();
        check(target.calls.equals(List.of("focus", "show")), "A caret tap restarted an active composing connection");
        target.window = false;
        keyboard.windowFocusChanged(false);
        target.window = true;
        keyboard.windowFocusChanged(true);
        target.drain();
        check(target.shown == 2, "Restored focus reopened a completed request");

        target = new Target();
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        keyboard.request(false);
        target.drain();
        check(target.shown == 0 && !target.editing, "A canceled posted request reopened IME");
        keyboard.request(true);
        keyboard.request(true);
        target.drain();
        check(target.shown == 1, "A superseded tap was still dispatched");

        target = new Target();
        target.failures = 1;
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        target.tasks.remove().run();
        check(target.shown == 1 && target.tasks.size() == 1, "Input connection did not get a deferred retry");
        target.drain();
        check(target.shown == 2, "Deferred retry was lost");

        target = new Target();
        target.failures = 1;
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        target.tasks.remove().run();
        keyboard.request(false);
        target.drain();
        check(target.shown == 1 && !target.editing, "Closing a field did not cancel the retry");

        target = new Target();
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        target.window = false;
        target.drain();
        check(target.shown == 0, "IME opened after window focus was lost");
        target.window = true;
        keyboard.windowFocusChanged(true);
        target.drain();
        check(target.shown == 1, "An interrupted pending request did not resume");

        target = new Target();
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        target.resumed = false;
        keyboard.request(false);
        target.drain();
        target.resumed = true;
        keyboard.windowFocusChanged(true);
        target.drain();
        check(target.shown == 0 && !target.editing, "Resume reopened a canceled keyboard");

        target = new Target();
        target.focusable = false;
        keyboard = new KeyboardController(target);
        keyboard.request(true);
        target.drain();
        check(target.shown == 0, "IME opened without view focus");
        target.focusable = true;
        keyboard.request(true);
        target.drain();
        check(target.shown == 1, "A fresh field tap could not retry view focus");
        System.out.println("KeyboardController: 8 focus/lifecycle/cancellation checks passed");
    }
}
