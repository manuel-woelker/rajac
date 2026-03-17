package verify.finals;

public class BlankFinalAssignedInBranches {
    private final int value;

    public BlankFinalAssignedInBranches(boolean flag) {
        if (flag) {
            value = 1;
        } else {
            value = 2;
        }
    }

    public int value() {
        return value;
    }
}
