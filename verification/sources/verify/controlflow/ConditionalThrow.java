class ConditionalThrow {
    int run(boolean fail) {
        if (fail) {
            throw new RuntimeException();
        }
        return 1;
    }
}
