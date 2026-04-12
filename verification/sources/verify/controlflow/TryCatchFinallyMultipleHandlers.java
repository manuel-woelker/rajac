package verify.controlflow;

public class TryCatchFinallyMultipleHandlers {
    public static int run() {
        int value = 0;
        try {
            throw new IllegalArgumentException();
        } catch (IllegalArgumentException first) {
            value = 1;
        } catch (RuntimeException second) {
            value = 2;
        } finally {
            cleanup();
        }

        return value;
    }

    private static void cleanup() {
    }
}
