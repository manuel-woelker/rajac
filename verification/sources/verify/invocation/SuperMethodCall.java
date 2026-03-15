package verify.invocation;

class SuperMethodCallBase {
    int value() {
        return 3;
    }
}

public class SuperMethodCall extends SuperMethodCallBase {
    public int value() {
        return super.value() + 4;
    }
}
