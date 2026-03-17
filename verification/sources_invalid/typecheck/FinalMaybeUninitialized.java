class FinalMaybeUninitialized {
    int run(boolean flag) {
        final int value;
        if (flag) {
            value = 1;
        }
        return value;
    }
}
