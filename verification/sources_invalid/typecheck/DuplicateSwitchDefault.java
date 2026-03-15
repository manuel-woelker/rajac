class DuplicateSwitchDefault {
    int run(int value) {
        switch (value) {
            default:
                return 1;
            default:
                return 2;
        }
    }
}
