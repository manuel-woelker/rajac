package verify.finals;

public class BlankFinalAssignedInInitializerBlock {
    private final int value;

    {
        value = 7;
    }

    public BlankFinalAssignedInInitializerBlock() {
    }

    public int value() {
        return value;
    }
}
