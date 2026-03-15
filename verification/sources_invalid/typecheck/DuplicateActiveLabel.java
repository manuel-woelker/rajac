class DuplicateActiveLabel {
    void run() {
        outer:
        while (true) {
            outer:
            while (true) {
                break outer;
            }
        }
    }
}
