class BlankFinalMissingViaThis {
    final int value;

    BlankFinalMissingViaThis() {
        this(true);
    }

    BlankFinalMissingViaThis(boolean flag) {
        if (flag) {
            value = 1;
        }
    }
}
