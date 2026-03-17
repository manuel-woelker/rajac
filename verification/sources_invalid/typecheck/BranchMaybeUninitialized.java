class BranchMaybeUninitialized {
    int run(boolean flag) {
        int value;
        if (flag) {
            value = 1;
        }
        return value;
    }
}
