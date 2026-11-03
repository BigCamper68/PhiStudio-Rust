package com.phistudio.rust;

/** UI-thread keyboard requests survive window-focus startup, but not cancellation. */
final class KeyboardController {
    interface Target {
        boolean isResumed();
        boolean hasWindowFocus();
        boolean hasViewFocus();
        boolean requestFocus();
        void setTextInputRequested(boolean requested);
        void post(Runnable action);
        void restartInput();
        boolean show();
        void hide();
    }

    private final Target target;
    private long generation;
    private boolean requested;
    private boolean pending;
    private boolean restartPending;

    KeyboardController(Target target) { this.target = target; }

    void request(boolean show) {
        generation++;
        restartPending |= show && (!requested || !target.hasViewFocus() || !target.hasWindowFocus());
        requested = show && target.isResumed();
        pending = requested;
        target.setTextInputRequested(requested);
        if (!requested) {
            target.hide();
        } else {
            schedule(generation);
        }
    }

    void windowFocusChanged(boolean focused) {
        if (!focused && requested) restartPending = true;
        // Completed requests must not reopen IME after Back or a focus restore.
        if (focused && pending && requested && target.isResumed()) {
            schedule(++generation);
        }
    }

    private boolean current(long request) {
        return request == generation && requested && pending
                && target.isResumed() && target.hasWindowFocus();
    }

    private void schedule(long request) {
        if (!current(request)) return;
        // ViewRoot's input-method focus update must finish before showing IME.
        target.post(() -> {
            if (!current(request) || !target.requestFocus()) return;
            if (restartPending) {
                target.restartInput();
                restartPending = false;
            }
            if (target.show()) {
                pending = false;
            } else {
                // A newly restarted InputConnection can need one more UI turn.
                target.post(() -> {
                    if (current(request) && target.requestFocus() && target.show()) {
                        pending = false;
                    }
                });
            }
        });
    }
}
