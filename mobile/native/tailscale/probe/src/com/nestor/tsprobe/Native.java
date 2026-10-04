package com.nestor.tsprobe;

final class Native {
    static {
        System.loadLibrary("tailscale");
        System.loadLibrary("probe");
    }

    static native String up(String dir, String hostname, String authKey);
    static native String dial(String addr, String payload);
    static native void close();
    static native String logs();

    private Native() {}
}
