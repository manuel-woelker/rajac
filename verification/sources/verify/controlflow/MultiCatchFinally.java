package verify.controlflow;

public class MultiCatchFinally {
    public static int run() {
        int value = 0;
        try {
            throw new IllegalArgumentException();
        } catch (IllegalArgumentException | IllegalStateException err) {
            value = 1;
        } finally {
            cleanup();
        }

        return value;
    }

    private static void cleanup() {
    }
}
