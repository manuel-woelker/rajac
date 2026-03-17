package verify.controlflow;

public class TryFinallyThrow {
    public static void run() {
        try {
            throw new RuntimeException();
        } finally {
            cleanup();
        }
    }

    private static void cleanup() {
    }
}
