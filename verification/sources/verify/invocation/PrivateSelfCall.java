package verify.invocation;

public class PrivateSelfCall {
    private int increment(int value) {
        return value + 1;
    }

    public int call() {
        return increment(4);
    }
}
