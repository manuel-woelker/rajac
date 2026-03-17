public class BlankFinalInitializerBlockDuplicateAssignment {
    private final int value;

    {
        value = 1;
    }

    public BlankFinalInitializerBlockDuplicateAssignment() {
        value = 2;
    }
}
