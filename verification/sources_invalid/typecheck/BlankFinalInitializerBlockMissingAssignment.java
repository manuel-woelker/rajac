public class BlankFinalInitializerBlockMissingAssignment {
    private boolean assign;
    private final int value;

    {
        if (assign) {
            value = 1;
        }
    }

    public BlankFinalInitializerBlockMissingAssignment() {
    }
}
