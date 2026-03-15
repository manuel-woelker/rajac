package verify.invocation;

class StaticDispatchHelper {
    static int value() {
        return 5;
    }
}

public class StaticDispatch {
    public static int call() {
        return StaticDispatchHelper.value();
    }
}
