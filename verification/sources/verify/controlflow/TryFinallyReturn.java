package verify.controlflow;

public class TryFinallyReturn {
    public static int run() {
        try {
            return 1;
        } finally {
            cleanup();
        }
    }

    private static void cleanup() {
    }
}
