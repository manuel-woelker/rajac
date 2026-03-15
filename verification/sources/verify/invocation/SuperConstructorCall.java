package verify.invocation;

class SuperConstructorCallBase {
    SuperConstructorCallBase(int value) {
    }
}

public class SuperConstructorCall extends SuperConstructorCallBase {
    public SuperConstructorCall() {
        super(3);
    }

    public static int build() {
        return 1;
    }
}
