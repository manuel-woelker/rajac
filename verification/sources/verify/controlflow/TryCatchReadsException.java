package verify.controlflow;

public class TryCatchReadsException {
    public static void run() {
        try {
            throw new RuntimeException();
        } catch (RuntimeException err) {
            throw err;
        }
    }
}
