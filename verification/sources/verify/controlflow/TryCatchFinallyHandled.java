package verify.controlflow;

public class TryCatchFinallyHandled {
    public static int run() {
        int value = 0;
        try {
            throw new RuntimeException();
        } catch (RuntimeException err) {
            value = 1;
        } finally {
            cleanup();
        }

        return value;
    }

    private static void cleanup() {
    }
}
