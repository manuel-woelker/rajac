package verify.typeops;

interface InterfaceInstanceOfWorker {
    void run();
}

public class InterfaceInstanceOf {
    public static boolean check(Object value) {
        return value instanceof InterfaceInstanceOfWorker;
    }
}
