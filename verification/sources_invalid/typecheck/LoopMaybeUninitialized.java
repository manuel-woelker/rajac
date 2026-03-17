class LoopMaybeUninitialized {
    int run(boolean flag) {
        int value;
        while (flag) {
            value = 1;
        }
        return value;
    }
}
