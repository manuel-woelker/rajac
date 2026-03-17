package verify.finals;

public class FinalAssignedInBranches {
    public static int run(boolean flag) {
        final int value;
        if (flag) {
            value = 1;
        } else {
            value = 2;
        }
        return value;
    }
}
