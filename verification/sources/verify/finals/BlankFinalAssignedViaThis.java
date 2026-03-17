package verify.finals;

public class BlankFinalAssignedViaThis {
    private final int value;

    public BlankFinalAssignedViaThis() {
        this(1);
    }

    public BlankFinalAssignedViaThis(int value) {
        this.value = value;
    }

    public int value() {
        return value;
    }
}
