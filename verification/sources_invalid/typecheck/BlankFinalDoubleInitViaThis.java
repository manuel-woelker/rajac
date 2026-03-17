class BlankFinalDoubleInitViaThis {
    final int value;

    BlankFinalDoubleInitViaThis() {
        this(1);
        value = 2;
    }

    BlankFinalDoubleInitViaThis(int value) {
        this.value = value;
    }
}
