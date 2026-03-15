package verify.invocation;

interface InterfaceDispatchWorker {
    int value();
}

public class InterfaceDispatch {
    public static int call(InterfaceDispatchWorker worker) {
        return worker.value();
    }
}
