class DuplicateMultiCatchAlternative {
    int run() {
        try {
            throw new RuntimeException();
        } catch (RuntimeException | RuntimeException err) {
            return 1;
        }
    }
}
